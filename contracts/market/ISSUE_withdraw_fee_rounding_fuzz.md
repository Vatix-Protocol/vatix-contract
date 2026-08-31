# Fuzz case for withdraw fee rounding

## Issue

> `fee_amount = amount * bps / 10_000` can round; fuzz edge amounts near zero
> and max. Extend `withdraw_fuzz` or add cases. Assert `fee + payout == amount`
> (or documented dust rule). No underflow/overflow on edge amounts. Rounding
> rule documented.

## What was added

`contracts/market/src/withdraw_fuzz.rs` gets a new `fee_rounding_invariants`
proptest module (2,000 cases per property) exercising
`validation::calculate_fee(amount, fee_rate_bps)` — the function backing
`withdraw_unused_collateral`'s fee step:

- `prop_fee_rounding_never_overflows_or_exceeds_amount` — general case over
  `amount in 1..=10_000_000_000` and the full `fee_rate_bps in 0..=10_000`
  range: `0 <= fee_amount <= amount`, and the floor-division dust rule
  `amount * bps == fee_amount * 10_000 + dust` with `0 <= dust < 10_000`.
- `prop_fee_rounding_near_zero_amount` — `amount in 1..=1_000`: tiny amounts
  below a rate's bps granularity floor `fee_amount` to exactly `0`.
- `prop_fee_rounding_near_max_amount` — `amount` near
  `validate_amount_reasonable`'s ceiling (`i128::MAX / 2`, the largest amount
  the contract will ever accept). **This is where a real overflow edge case
  lives**: at this magnitude, `amount * fee_rate_bps` itself can exceed
  `i128::MAX` for any `fee_rate_bps` beyond single digits — the overflow
  happens in the multiplication, before the division. The test asserts
  `calculate_fee` fails closed with `ContractError::ArithmeticOverflow`
  (via its existing `checked_mul`) in that case, rather than panicking or
  wrapping to a bogus/negative fee; when the product does fit in `i128`, the
  result must still floor-divide correctly.
- `prop_fee_rounding_max_bps_equals_amount` — at 10_000 bps (100%),
  `fee_amount == amount` exactly, no rounding loss at the boundary rate.
- `prop_fee_rounding_zero_bps_is_always_zero` — at 0 bps, `fee_amount == 0`
  for every amount up to the ceiling.

## Rounding / dust rule (documented, per acceptance criteria)

`withdraw_unused_collateral` does **not** carve the fee out of the requested
`amount` — the user always receives exactly `amount`, and
`amount + fee_amount` is deducted from `total_deposited` on top of it (see
`withdraw.rs`'s existing `#377` module doc). So the relevant invariant isn't
`fee + payout == amount`; it's:

```
fee_amount = floor(amount * fee_rate_bps / 10_000)
amount * fee_rate_bps == fee_amount * 10_000 + dust,   0 <= dust < 10_000
```

Integer division floors, so up to `9_999` stroops of `amount * fee_rate_bps`
can be lost to rounding on every withdrawal. That dust is never collected by
the protocol and never charged to the user beyond the floored `fee_amount`
— it simply disappears below the bps granularity, in the user's favor.
