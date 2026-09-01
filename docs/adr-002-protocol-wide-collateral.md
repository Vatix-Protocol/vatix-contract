# ADR-002: Protocol-Wide Collateral Balance

**Status:** Proposed (Phase 1 implemented)
**Date:** 2026-08-25
**Issue:** [#685](https://github.com/Vatix-Protocol/vatix-contract/issues/685)

---

## Context

Collateral in `MarketContract` has historically been siloed per market:
`Position { market_id, user, total_deposited, locked_collateral, ... }` is
keyed by `(market_id, user)` in storage (`StorageKey::Position(u32,
Address)`), and `deposit_collateral` only ever credits the position for the
single `market_id` passed in.

This forces a user who wants to trade in two markets to deposit collateral
twice — once per market — even though the same USDC could, in principle,
back positions in either market at different times. That is capital
inefficient and a poor UX: a market maker active across many markets needs
`N` separate deposits (and `N` separate withdrawals) instead of one balance
they can allocate wherever they trade.

`contracts/market/src/deposit.rs` carried a long-standing `TODO` describing
this exact gap:

```rust
// TODO: Refactor collateral management
// Current design requires separate deposits per market. Users cannot use
// Market A collateral for Market B trades. refactor will introduce:
// - Global user balance (deposit once, trade anywhere)
// - Better capital efficiency
```

---

## Decision Drivers

| Driver | Weight |
|---|---|
| Capital efficiency for multi-market users | High |
| Backward compatibility with existing withdraw/settlement paths | High |
| Minimizing blast radius of the storage-layout change | High |
| Correctness of collateral accounting under concurrent multi-market trading | High |

---

## Options Considered

### Option A — Keep per-market silos (status quo)

**Pros:** No contract changes needed; storage layout stays simple.
**Cons:** Capital inefficiency and repeated-deposit UX friction persist
indefinitely; does not close the gap identified in issue #685.

### Option B — Full single-pool redesign in one pass

Replace `Position.total_deposited` entirely with a single protocol-wide
balance, and rewrite `deposit_collateral`, `withdraw_unused_collateral`,
`update_position`, and settlement to all read/write that one balance instead
of the per-market field.

**Pros:** Cleanest end state — one balance, no legacy field.
**Cons:** Touches every collateral-adjacent code path
(`deposit.rs`, `withdraw.rs`, `settlement.rs`, `positions.rs`,
`reconciliation.rs`) in a single change, each of which has its own existing
invariants and test coverage (`tests/collateral_invariant_test.rs`,
`tests/locked_le_deposited_invariant_test.rs`,
`tests/proptest_locked_invariant.rs`). A storage-breaking change of this
size across every entrypoint is exactly the kind of change that should ship
incrementally, not as one large low-visibility diff.

### Option C — Additive protocol-wide ledger, incremental migration (chosen)

Introduce a new, user-scoped storage ledger *alongside* the existing
per-market `Position.total_deposited`, wire it into the collateral-adequacy
check that gates new trades, and migrate the remaining consumers
(`withdraw_unused_collateral`, settlement) in a follow-up change once the
core ledger has proven itself.

**Pros:** Closes the capital-inefficiency gap for trading (the primary
complaint) immediately; leaves withdraw/settlement behavior unchanged and
low-risk; each future migration step is independently reviewable.
**Cons:** Two collateral bookkeeping mechanisms coexist during the
migration window; withdrawal remains per-market until Phase 2.

---

## Decision

**Adopt Option C.** Phase 1, implemented alongside this ADR:

1. **New storage keys, scoped by user only** (`contracts/market/src/storage.rs`,
   `STORAGE_VERSION` bumped to `6`):
   - `StorageKey::CollateralBalance(Address)` — a user's total protocol-wide
     deposited collateral, summed across every market they have ever
     deposited into.
   - `StorageKey::TotalLockedCollateral(Address)` — the sum of
     `Position.locked_collateral` across every market the user currently
     holds a position in. Tracked so the adequacy check below is O(1)
     instead of requiring an iteration over every market a user has traded
     in.

2. **`deposit_collateral` (`deposit.rs`)** now credits
   `CollateralBalance(user)` by `amount` on every deposit, in addition to
   incrementing the existing per-market `Position.total_deposited`. The
   legacy field is left untouched so `withdraw_unused_collateral` and
   settlement — which still operate per market — continue to work exactly
   as before.

3. **`MarketContract::update_position` (`lib.rs`)** — the entrypoint that
   gates every buy/sell — replaces its old check
   (`prospective_locked > position.total_deposited`) with a protocol-wide
   check via `positions::check_protocol_collateral`: a trade that would
   *increase* this market's lock is now compared against the user's shared
   `CollateralBalance`, net of whatever is already locked in the user's
   *other* markets (`TotalLockedCollateral(user) - position.locked_collateral`).
   This is the mechanism that lets a user deposit once and trade in any
   market: as long as their total locked collateral across every market
   stays within their total deposited balance, no second deposit is
   required. `TotalLockedCollateral(user)` is updated every time a trade
   changes a market's lock, keeping the aggregate in sync.

4. **`positions.rs`** gains `PositionError::InsufficientProtocolCollateral`
   and the `check_protocol_collateral` helper implementing the invariant
   above.

### Phase 2 (not yet implemented — follow-up)

- Migrate `withdraw_unused_collateral` (`withdraw.rs`) to draw from
  `CollateralBalance(user)` instead of `Position.total_deposited`, so a
  withdrawal can pull from collateral deposited against *any* market.
- Migrate settlement (`settlement.rs`) to release a settled position's
  locked collateral back into `CollateralBalance(user)` rather than only
  crediting the per-market field.
- Once both are migrated and burned in, consider removing
  `Position.total_deposited` entirely (a further storage-breaking change,
  requiring its own `STORAGE_VERSION` bump and migration plan).

---

## Consequences

### Positive
- A user who deposits collateral once can immediately trade in any market
  without a second deposit, as long as their aggregate lock across all
  markets stays within their protocol-wide balance.
- The change is additive at the storage level — no existing field was
  removed or reinterpreted, so single-market flows (the common case
  exercised by the bulk of the existing test suite) are numerically
  unaffected: for a user active in exactly one market,
  `CollateralBalance(user) == Position.total_deposited` and
  `TotalLockedCollateral(user) == Position.locked_collateral`, so the new
  check behaves identically to the old one.

### Negative / Risks
- Two collateral bookkeeping mechanisms (`Position.total_deposited` and
  `CollateralBalance`) coexist until Phase 2 lands. They must be kept
  consistent by construction (every `deposit_collateral` call credits both);
  a future change that adds a new collateral-crediting path must remember to
  update both, or intentionally migrate off the legacy field first.
- Withdrawal and settlement remain per-market in Phase 1: a user cannot yet
  withdraw *from* Market B collateral that was deposited *against* Market A
  even though they could trade it there. This is a known, documented
  limitation closed by Phase 2, not silently dropped.
- `STORAGE_VERSION` bump to `6` is a breaking storage change and requires
  the same migration procedure documented in `STORAGE_MIGRATION_GUIDE.md`
  before this ships to an already-initialized deployment.

### Open Questions
- Should `CollateralBalance` be denominated per collateral token (mirroring
  the treasury's `TokenBalance(Address)` design) once markets with
  different collateral tokens can share a user's balance? Phase 1 assumes a
  single collateral token per deployment, matching the current codebase.
- Should Phase 2 also update `reconciliation.rs`'s invariant checks to
  assert `sum(locked_collateral) <= CollateralBalance` protocol-wide, in
  addition to the existing per-market `locked <= deposited` invariant?
