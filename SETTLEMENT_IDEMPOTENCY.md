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

## Files touched

- `contracts/market/src/settlement.rs` — new test only; no production logic
  changed, since the idempotency guard was already correct.
