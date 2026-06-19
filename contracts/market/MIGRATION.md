# Migration: `Position::locked_collateral` is now share-based only (#262)

## What changed

`Position::locked_collateral` is now exclusively a derived, share-based value
maintained by `positions::update_position` (via `calculate_locked_collateral`).
No other code path writes to this field:

- `deposit_collateral` only increments `total_deposited`. It no longer adds
  the deposit amount to `locked_collateral`.
- `withdraw_unused_collateral` reads `Position::locked_collateral` directly
  instead of recomputing a required lock from a hardcoded 50/50 market price.

## Why

Before this change, three code paths computed "locked collateral" three
different ways:

- `deposit_collateral` set `locked_collateral = locked_collateral + amount`,
  so a user who deposited but never bought any shares already showed their
  entire deposit as locked.
- `update_position` correctly set `locked_collateral` from
  `calculate_locked_collateral(yes_shares, no_shares, market_price)`.
- `withdraw_unused_collateral` ignored the stored field entirely and
  recomputed its own lock using a hardcoded `MARKET_PRICE_BPS = 5_000`,
  which diverged from the real trade price whenever shares were bought at
  any price other than 50/50.

## Impact on existing/seeded state and test fixtures

Any `Position` record (on-chain, in a local snapshot, or hand-built in a
test) that was produced by the old `deposit_collateral` and has
`locked_collateral == total_deposited` while `yes_shares == 0 && no_shares
== 0` reflects the old, incorrect accounting. It must be recomputed, not
copied forward:

- The correct value for such a position is `locked_collateral = 0` (no
  shares means nothing is locked).
- Positions that do hold shares should have `locked_collateral` recomputed
  via `calculate_locked_collateral(yes_shares, no_shares, market_price)` at
  whatever price the shares were actually bought at — not assumed to be
  50/50.
- There were no fixture/snapshot files checked into this repo at the time of
  this change (`tests/`, `contracts/market/src` contain only inline test
  data), so no files needed editing. Hand-built `Position { .. }` literals in
  existing tests already set `locked_collateral` to the share-based value
  directly and did not need changes. If you maintain seeded state outside
  this repo (e.g. a local devnet snapshot or fixture data), re-derive
  `locked_collateral` for every position using the rule above before
  diffing against post-fix behavior, or the diff will look like a
  regression when it is actually a correction.
