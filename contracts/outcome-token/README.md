# Outcome Token Contract

Manages per-market, per-side (YES/NO) outcome tokens for the Vatix protocol.
Only the registered market contract may `mint`/`burn` tokens; balances and
total supplies are tracked per `(market_id, user, TokenKind)`.

See `contracts/outcome-token/src/lib.rs` for the full entry-point reference
and `docs/cross-contract-call-graph.md` for how this contract is invoked from
the Market contract.

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
