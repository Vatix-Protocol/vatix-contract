# Dust Handling (#932)

"Dust" is any sub-unit residue lost to integer division on a money path.
Reference: [`contracts/market/ISSUE_withdraw_fee_rounding_fuzz.md`](../contracts/market/ISSUE_withdraw_fee_rounding_fuzz.md).

## Rule

```
fee_amount = floor(amount * fee_rate_bps / 10_000)
amount * fee_rate_bps == fee_amount * 10_000 + dust,   0 <= dust < 10_000
```

- **Direction:** floor division, always in the user's favor. The protocol never
  over-collects; dust is neither charged to the user nor credited to the treasury.
- **Tiny amounts:** an `amount` below a rate's bps granularity yields `fee_amount == 0`.
- **Boundaries:** `bps == 0` → fee `0`; `bps == 10_000` → fee `== amount` (no loss).
- **Overflow:** `calculate_fee` uses `checked_mul`; an oversized product fails
  closed with `ContractError::ArithmeticOverflow` — never wraps or panics.
- **Determinism:** the fee is a pure function of `(amount, fee_rate_bps)`, so
  replayed inputs yield identical results.
- **Waivers:** a waived account resolves to `0` bps via
  `fee_waiver::effective_fee_rate_bps`, so no dust arises for it.

## Coverage

`contracts/market/src/withdraw_fuzz.rs` (`fee_rounding_invariants`) asserts the
rule above for near-zero, general, near-max, zero-bps and max-bps inputs.

## Rollback

Documentation only; no on-chain behavior change. Revert the commit to roll back.
