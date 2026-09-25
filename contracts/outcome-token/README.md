# Outcome Token Contract

Manages per-market, per-side (YES/NO) outcome tokens for the Vatix protocol.
Only the registered market contract may `mint`/`burn` tokens; balances and
total supplies are tracked per `(market_id, user, TokenKind)`.

See `contracts/outcome-token/src/lib.rs` for the full entry-point reference
and `docs/cross-contract-call-graph.md` for how this contract is invoked from
the Market contract.

## SAC metadata invariants

This contract is a Stellar Asset Contract (SAC)-compatible token: it exposes
the standard SEP-41 metadata surface (`name`, `symbol`, `decimals`) so wallets,
explorers, and the Market contract can treat outcome tokens like any other
SAC asset. Metadata is **immutable after initialization** and is set exactly
once, at deploy/init time, from the market's registered parameters.

### Invariants

1. **Set-once / idempotent.** `name`, `symbol`, and `decimals` are written a
   single time during `initialize`. A second `initialize` (or any
   `set_metadata`) call is rejected with `ContractError::AlreadyInitialized`
   (`#867`). Replaying the same init payload is a no-op that returns the
   existing values — it never mutates them.
2. **Non-empty, bounded strings.** `name` and `symbol` must be non-empty and
   within `MAX_NAME_LEN` / `MAX_SYMBOL_LEN` (see `src/lib.rs`). Empty or
   over-long values are rejected with `ContractError::InvalidMetadata`.
3. **Fixed decimals.** `decimals` must equal `OUTCOME_TOKEN_DECIMALS` (7, the
   Stellar SAC convention). Any other value is rejected with
   `ContractError::InvalidMetadata` — outcome tokens must not silently change
   scale, since that would corrupt collateral math in the Market contract.
4. **Admin-gated writes.** Only the stored admin may call `initialize` /
   `set_metadata`. Untrusted callers are denied by default with
   `ContractError::Unauthorized`; there is no public write path to metadata.
5. **Reads are always safe.** `name`/`symbol`/`decimals` are pure reads with
   no auth requirement and never fail once initialized; before init they
   return `ContractError::NotInitialized` (fail-closed, never a default).

### Error codes

| Code | Error | Meaning |
|---|---|---|
| `#867-1` | `AlreadyInitialized` | Metadata already set; write rejected |
| `#867-2` | `InvalidMetadata` | Empty/over-long name/symbol or wrong decimals |
| `#867-3` | `Unauthorized` | Caller is not the stored admin |
| `#867-4` | `NotInitialized` | Metadata read before `initialize` |

All metadata entrypoints emit a correlation id (the market id, or `0` for
contract-wide init) in their events/logs so ops can trace a rejected write
back to the originating market without leaking secrets.

### Fail-closed behavior

Metadata writes are fail-closed: if the admin auth check cannot be satisfied
(expired/absent auth, wrong role) or a dependency read fails, the call is
rejected rather than defaulting. There is no partial write — either all three
fields are set atomically or none are.

### Tests

`contracts/outcome-token/src/lib.rs` covers: successful init setting all three
fields, re-init rejected as `AlreadyInitialized`, idempotent replay returning
existing values, empty/over-long name/symbol rejected, wrong `decimals`
rejected, non-admin `initialize`/`set_metadata` rejected as `Unauthorized`,
and reads before init returning `NotInitialized`.

## Dual-ledger reconciliation (Position ↔ OutcomeToken)

`Position` (Market contract storage: `yes_shares`/`no_shares`) and this
contract's token balances are two independent ledgers that are supposed to
always stay in lockstep — every `update_position` mint/burn moves both in the
same direction, and `settle_position` burns tokens back to zero on payout.

They can still diverge from:

- A historical bug in the mint/burn call sites.
- A partial upgrade — e.g. this contract redeployed or re-pointed
  (`set_market_contract`) mid-market, so old and new balances don't line up
  with Market's stored positions.
- A manual admin `mint`/`burn` issued directly on this contract, bypassing
  the Market contract entirely.

Left unchecked, this is a classic dual-ledger footgun: over-minted tokens
become extractable value, and under-minted tokens brick a user's exit.

### Guard and repair (implemented in the Market contract)

The reconciliation logic lives in `contracts/market/src/reconciliation.rs`
(this contract only exposes the plain `mint`/`burn`/`balance` primitives it
always has; it has no special-cased reconciliation API of its own):

- **`MarketContract::get_position_token_parity(market_id, user)`** — read-only
  view comparing `Position.yes_shares`/`no_shares` against this contract's
  `balance(market_id, user, Yes|No)`. Callable by anyone.
- **Guard on `update_position` / `settle_position`** (and the batch/page
  settlement variants) — before mutating state, the Market contract reads
  both ledgers via `get_position_token_parity`. On mismatch it rejects with
  `ContractError::PositionTokenMismatch` (single settle/trade) or skips just
  the affected user (batch/page settlement), after emitting a
  `PositionTokenMismatchDetected` event. There is **no silent re-sync** on
  this path — a mismatched user/market pair stays blocked until an admin
  repairs it.
- **`MarketContract::reconcile_position_tokens(admin, market_id, user)`** —
  admin-gated repair. **Policy: `Position` is the source of truth.** This
  mints or burns the user's `OutcomeToken` balances on this contract so they
  match `Position` — never the other way around, since `Position` also
  drives locked-collateral and `total_deposited` accounting that cannot be
  safely rederived from token balances alone. Emits `PositionTokensReconciled`
  with the signed mint/burn deltas applied. No-op (no event) if the ledgers
  already agree.

### Why the policy lives in Market, not here

This contract has no notion of locked collateral, deposits, or settlement
eligibility — `Position` in the Market contract is the richer, authoritative
record. Reconciliation therefore always adjusts *this* contract's balances to
match Market's `Position`, not the reverse.

### Events

| Event | Emitted by | Meaning |
|---|---|---|
| `PositionTokenMismatchDetected` | Market (`update_position`, settlement paths) | Divergence observed; call was rejected/skipped |
| `PositionTokensReconciled` | Market (`reconcile_position_tokens`) | Admin repair applied; includes signed `yes_delta_applied`/`no_delta_applied` |

### Tests

`contracts/market/src/reconciliation.rs` covers: parity holding after a
normal trade, divergence detection after an out-of-band `mint` issued
directly on this contract (simulating the manual-admin-mint / historical-bug
scenario), trading and settlement both getting blocked while divergent,
`reconcile_position_tokens` restoring parity (and being a no-op when already
matched), and the repair path rejecting non-admin callers.
