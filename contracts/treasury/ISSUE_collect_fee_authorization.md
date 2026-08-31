# Treasury `collect_fee` authorized-market enforcement

## Issue

> `collect_fee` must only accept registered/authorized market contracts.
> Enforce authorized market registry check. Tests for authorized vs stranger
> caller. Unauthorized `collect_fee` fails; authorized path updates balances.

## Findings

Auditing `contracts/treasury/src/lib.rs` and `contracts/treasury/src/storage.rs`
shows the authorized-market registry gate on `collect_fee` was already fully
implemented (storage doc comment even labels it "v2: Completed the
multi-market `AuthorizedMarkets` registry"):

- `TreasuryContract::collect_fee` (`lib.rs:83-134`) calls
  `storage::is_authorized_market(&env, &caller)` and returns
  `TreasuryError::CallerNotMarket` when the caller isn't in the registry,
  before touching any balance.
- The registry itself (`AuthorizedMarkets`, a `Vec<Address>`) is managed by
  admin-only `add_market` / `remove_market` / `set_market_contract`, and can
  be inspected via `is_authorized_market` / `list_markets`.
- `contracts/market/src/withdraw.rs:104-124` — the only production caller —
  already invokes `collect_fee` passing the market contract's own address as
  `caller`, which self-authorizes via Soroban's contract sub-invocation auth
  (no signature possible for a contract address, so only that exact
  contract can satisfy `caller.require_auth()`).
- Test coverage already existed for both acceptance criteria:
  `collect_fee_rejects_unauthorized_caller` /
  `collect_fee_updates_balance_and_cumulative` /
  `removed_market_cannot_collect_fee` /
  `multiple_markets_can_each_collect_fees` (`contracts/treasury/src/test.rs`).

## What this change adds

No production code changes were needed — the gate and its primary tests
were already correct and in place. This change adds two additional edge-case
regression tests to `contracts/treasury/src/test.rs` to close small gaps in
existing coverage:

- `re_added_market_can_collect_fee_again` — confirms that removing then
  re-adding a market via `remove_market` / `add_market` correctly restores
  `collect_fee` access (no stale registry state left behind by removal).
- `collect_fee_rejects_caller_never_registered` — confirms a caller that was
  *never* registered (as distinct from one that was registered and later
  removed) is rejected identically, and leaves the token balance untouched.
