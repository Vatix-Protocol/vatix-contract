# FeeCollected Event on Withdraw

## Issue

Indexers need a stable `FeeCollected` (or equivalent) event, carrying
`market_id` and `amount`, emitted when a fee moves from a market into the
treasury — and zero-fee withdraws must not emit a misleading amount.

## What was already in place

- `contracts/treasury/src/events.rs` defines `FeeCollected` with `#[topic]
  market_id`, `#[topic] token`, and data fields `fee_amount`,
  `new_token_balance`, `new_cumulative_fees`.
- `TreasuryContract::collect_fee` (`contracts/treasury/src/lib.rs`) publishes
  it on every successful fee collection, and rejects `fee_amount <= 0` with
  `TreasuryError::InvalidAmount` — a zero/negative fee can never reach the
  event.
- `contracts/market/src/withdraw.rs`'s `withdraw_unused_collateral` only
  invokes the treasury's `collect_fee` cross-contract call when
  `fee_amount > 0`; when the configured fee rate is `0` (or the caller is
  fee-waived) the call — and therefore the event — is skipped entirely
  rather than firing with `amount = 0`.

So the event, its fields, and the zero-fee no-emit behavior already existed.
The gap was test coverage: no test asserted the event's topics/data from the
*market withdraw* path (only from `TreasuryContract::collect_fee` called
directly), and no test asserted a zero-fee withdraw does not emit it.

## What this change adds

Two tests in `tests/treasury_integration_test.rs`:

- `withdraw_emits_fee_collected_event_with_market_id_and_amount` — drives a
  real `withdraw_unused_collateral` call through a fee-charging market,
  locates the `fee_collected_event` among the events emitted during that
  call (it isn't the last event — `fee_calculated_event` and
  `collateral_withdrawn_event` are emitted around it), and asserts its topic
  count, `market_id`/`token` topics, and `fee_amount` /
  `new_token_balance` / `new_cumulative_fees` data match what the user was
  actually charged.
- Extends `zero_fee_rate_no_sac_fee_deducted` to assert no
  `fee_collected_event` is emitted at all when `fee_rate_bps` is `0`.

## Files touched

- `tests/treasury_integration_test.rs` — new event-assertion helpers and the
  two tests above; no production logic changed.
