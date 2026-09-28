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

## Fixture contract (finalized)

The fixtures below are the canonical, deterministic shape indexers and
contributors should rely on. They are derived from the emission sites above
and are asserted by the tests in `tests/treasury_integration_test.rs`.

### Event shape

| Field | Kind | Type | Notes |
| --- | --- | --- | --- |
| `market_id` | topic | `Symbol` | Identifies the market the fee was collected for. |
| `token` | topic | `Address` | SAC/token the fee was denominated in. |
| `fee_amount` | data | `i128` | Strictly `> 0`; the amount actually moved to the treasury. |
| `new_token_balance` | data | `i128` | Treasury token balance after the collection. |
| `new_cumulative_fees` | data | `i128` | Treasury cumulative fees after the collection. |

### Emission invariants

1. **Single emission per collection.** Exactly one `FeeCollected` is emitted
   per successful `collect_fee`; a withdraw that charges a fee emits exactly
   one, and it is not necessarily the last event in the call
   (`fee_calculated_event` and `collateral_withdrawn_event` are emitted
   around it).
2. **Positive amount only.** `fee_amount > 0` always holds. A zero or
   negative fee is rejected with `TreasuryError::InvalidAmount` before any
   event is published.
3. **Zero-fee no-emit.** When `fee_rate_bps == 0` (or the caller is
   fee-waived) the market skips the `collect_fee` cross-contract call, so no
   `FeeCollected` is emitted at all — never one with `fee_amount = 0`.
4. **Topic ordering.** Topics are ordered `[market_id, token]`; data fields
   are ordered `[fee_amount, new_token_balance, new_cumulative_fees]`.
5. **Determinism.** Fixtures are pure functions of the withdraw inputs and
   the treasury's prior state; no timestamps, randomness, or environment
   values are embedded, so the same inputs always yield the same event.
6. **No secrets.** Fixtures and their logs contain only public on-chain
   values (market id, token address, amounts); no keys, seeds, or
   credentials are ever included.

### Fixture vectors

These mirror the `test-vectors/` cases and are the values the integration
tests assert.

| Case | `fee_rate_bps` | `fee_amount` | Emits `FeeCollected`? |
| --- | --- | --- | --- |
| Fee-charging withdraw | `> 0` | `> 0` | Yes — one event, topics `[market_id, token]`, data `[fee_amount, new_token_balance, new_cumulative_fees]`. |
| Zero-fee withdraw | `0` | `0` | No — `collect_fee` is not called; no event emitted. |
| Fee-waived caller | `> 0` | `0` | No — waived callers skip the call; no event emitted. |
| Invalid fee | any | `<= 0` | No — `collect_fee` fails closed with `TreasuryError::InvalidAmount`. |

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
- `FEE_COLLECTED_EVENT.md` — this fixture contract, cross-linked from
  `SECURITY.md` and `AUTH_TABLE.md` for Stellar Wave contributors.

## Rollback

No production logic, storage layout, or mainnet-affecting behavior changed;
this is test coverage plus documentation. Rollback is reverting the test and
doc commit — no flag or kill-switch is required because no money-path code
was modified.
