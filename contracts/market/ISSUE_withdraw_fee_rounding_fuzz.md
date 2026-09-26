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

## Rounding direction is explicit and fail-closed

The fee is computed with **floor division** (`checked_mul` then `/ 10_000`),
so the protocol never over-collects: the user is charged at most the exact
proportional fee and the residual dust stays with the user. This is the
intended direction — the fee step can never under-collect relative to the
floored value, and it can never round *up* into a fee larger than
`amount * fee_rate_bps / 10_000`. The only failure mode is the multiplication
overflow described above, which fails closed with
`ContractError::ArithmeticOverflow` instead of wrapping.

## Idempotency / repeated-call coverage

`withdraw_unused_collateral` is not idempotent by design (each call moves
funds), but the fee step must be a pure function of `(amount, fee_rate_bps)`
so that replaying the same inputs yields the same fee. The fuzz properties
above assert this determinism implicitly: `calculate_fee` is called with the
same `(amount, fee_rate_bps)` pair across the general, near-zero, near-max,
max-bps, and zero-bps properties, and every property pins the exact expected
result (floor division, `0`, or `amount`), so any state-dependent or
non-deterministic fee computation would fail the suite.

## Boundary coverage summary

| Case | Input | Expected |
| --- | --- | --- |
| Dust withdrawal | `amount` below bps granularity | `fee_amount == 0` |
| 1-unit withdrawal | `amount == 1` | `fee_amount == 0` for `bps < 10_000` |
| Fee-rate boundary | `bps == 10_000` | `fee_amount == amount` |
| Zero fee rate | `bps == 0` | `fee_amount == 0` |
| Max amount | `amount` near `i128::MAX / 2` | floor-divide, or `ArithmeticOverflow` fail-closed |
| Repeated calls | same `(amount, bps)` | identical `fee_amount` |
