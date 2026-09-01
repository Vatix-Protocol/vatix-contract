# Fix: `get_position` / `get_net_position` fail clearly on unknown `market_id`

## Problem

`MarketContract::get_position` and `MarketContract::get_net_position` read
directly from `storage::get_position(market_id, user)`, which is keyed by
`(market_id, user)`. That storage lookup returns `Ok(None)` in two very
different situations:

1. The market exists, but this particular user has never traded or
   deposited in it.
2. `market_id` does not correspond to any market at all (typo, wrong
   network, market never created).

Both cases were indistinguishable to a caller — a bad `market_id` silently
looked identical to "no position yet" instead of failing clearly.

## Fix

`contracts/market/src/lib.rs`:

- `get_position` now calls `storage::has_market(&env, market_id)` first and
  returns `Err(ContractError::MarketNotFound)` when the market doesn't
  exist, before ever touching position storage.
- `get_net_position` gets the same check, for the same reason.
- Doc comments on both functions spell out the two distinct `Ok`/`Err`
  outcomes.

The happy paths are unchanged: a real market with no position for the
queried user still returns `Ok(None)` (`get_position`) / `Ok(0)`
(`get_net_position`).

## Scope note

`get_market_status` and `get_collateral_token` also take a `market_id` and
currently `.expect("market not found")` (i.e. panic) on a missing market.
They were left untouched in this change because they're plain (non-`Result`)
cross-contract view functions consumed by the outcome-token and resolution
contracts — changing their signature to `Result` is a breaking
cross-contract API change and out of scope for this focused fix, which is
about the position-read views named in the issue.

## Tests

`tests/get_position_unknown_market_test.rs`:

- `get_position` / `get_net_position` on an unknown `market_id` →
  `ContractError::MarketNotFound`.
- A real market with no position for the user → `Ok(None)` / `Ok(0)`
  (regression guard on the happy path).
- A real market with an actual position → the position/net value is
  returned correctly.
