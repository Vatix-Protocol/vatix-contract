# Settlement Idempotency (double-settle protection)

## Issue

`settle_position` must be idempotent-safe: a second call for the same
position should error or no-op without paying out twice.

## What was already in place

The market contract already tracked settlement state per-position and
guarded against re-settlement before this change:

- `Position.is_settled: bool` (`contracts/market/src/types.rs`) is persisted
  storage, not derived — it survives across calls.
- `validate_settlement_eligibility` (`contracts/market/src/settlement.rs`)
  returns `ContractError::PositionAlreadySettled` whenever
  `position.is_settled` is already `true`, *before* any payout math or token
  transfer runs.
- `compute_settlement` (shared by `settle_position`, `batch_settle_positions`,
  and `settle_positions_page`) calls that validation first, so every
  settlement code path — single, batch, and paginated — is protected the
  same way.
- The position is persisted with `is_settled = true` *before* the collateral
  transfer is issued, so even a reentrant call during the transfer would see
  the flag already set.
- The winning/losing outcome-token balances are burned on settlement
  (`OutcomeTokenContractClient::burn`), so the settled shares can't be
  transferred or redeemed a second time either.

## What this change adds

`test_second_settle_position_cannot_double_pay` in
`contracts/market/src/settlement.rs`: a dedicated negative test that goes
beyond asserting the second call errors. It asserts the *effect*:

- The second (and third, and fourth) `settle_position` call returns
  `Err(PositionAlreadySettled)`.
- The user's and contract's token balances are byte-for-byte unchanged after
  each rejected repeat call — proving no partial or duplicate payout leaked
  through.
- The stored `Position` is unchanged across the repeat attempts.

This directly satisfies the acceptance criteria ("second settle cannot drain
funds twice") with an explicit, repeatable regression test rather than
relying on the existing full-flow test's incidental error check.

## Workspace-level integration tests

`tests/settlement_test.rs` adds three workspace-level integration tests that
exercise the same guards through the cross-crate boundary with a real SAC
token, complementing the unit-level coverage in `settlement.rs`:

- **`second_settle_cannot_double_pay`** — the YES-winner path: resolves a
  market `result = Some(true)`, settles once, then verifies that three
  repeat calls each return `Err(PositionAlreadySettled)` and leave both the
  user's and the contract's token balances unchanged.

- **`second_settle_cannot_double_pay_no_winner_path`** — the no-winner
  refund path: resolves a market with `result = None`, settles once (refund
  of full deposited collateral), then verifies that a repeat call returns
  `Err(PositionAlreadySettled)` without moving any additional funds.

- **`settlement_before_resolution_is_rejected`** — confirms `settle_position`
  on an unresolved (Active) market returns `Err(MarketNotResolved)`.

Both idempotency tests assert the *effect* (token balances byte-for-byte
identical after each rejected repeat call), not just the error code — so a
future regression that returns the right error but still moves tokens would
also be caught.

## Status

The settlement idempotency guard was correct in production code all along.
The blocking issue was that the settlement test call-sites in
`contracts/market/src/settlement.rs` used stale API signatures that no longer
matched the current `#[contractimpl]` entrypoints:

- `initialize_market` was called with 5 arguments — missing the required
  `metadata_uri: Option<String>` sixth parameter.
- `resolve_market` was called with 3 arguments (`market_id_str`, `outcome`,
  `signature`) — missing the required `resolver: Address` first parameter and
  the `expires_at: u64` fifth parameter.

These mismatches caused compile-time failures across every settlement integration
test, leaving the `PositionAlreadySettled` guard completely untested.

### Fix applied

All `client.initialize_market(...)` calls in `contracts/market/src/settlement.rs`
updated to pass `&None` as the `metadata_uri` argument. All
`client.resolve_market(...)` calls updated to pass `&resolver` as the first
argument and `&(end_time + 86_400)` as the `expires_at` argument, satisfying
the fail-closed expiry check introduced in #701.

No production logic was changed.

## Files touched

- `contracts/market/src/settlement.rs` — all test call-sites updated to match
  current API signatures. Production logic unchanged.
- `contracts/market/src/test.rs` — three merge-corrupted test functions
  (`test_get_market_returns_all_fields`, `test_upgrade_order_safety_complete_sequence`,
  `test_get_market_reflects_closed_to_deposits`) restored to correct form; stale
  `initialize_market` and `resolve_market` signatures fixed throughout.
- `tests/market_test.rs` — stale call-sites in all integration tests updated.
- `tests/client_entrypoints_test.rs` — stale call-sites updated.
- `tests/event_snapshot_test.rs` — stale call-sites updated.
- `tests/storage_limits_test.rs` — stale call-sites updated.
- `tests/positions_test.rs` — stale call-site updated.
- `tests/proptest_locked_invariant.rs` — stale call-site updated.
- `tests/collateral_invariant_test.rs` — stale call-site updated.
- `tests/settlement_test.rs` — workspace-level integration tests (already
  used correct API signatures; no changes needed).
- `SETTLEMENT_IDEMPOTENCY.md` — this file updated to document the fix.
