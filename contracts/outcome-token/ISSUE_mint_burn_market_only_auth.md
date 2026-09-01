# Outcome-token mint/burn market-only authorization

## Issue

> outcome-token must enforce market-only auth on mint/burn.
> require_auth / invoker checks on mint and burn. Tests. External EOAs
> cannot mint. Market contract path succeeds in test harness.

## Findings

`contracts/outcome-token/src/lib.rs` already gates both `mint` (line ~118)
and `burn` (line ~148) with `config.market_contract.require_auth()` before
touching any balance — only the registered market contract address can
satisfy that check, so the production authorization logic already matched
the issue's request.

The gap was in test coverage, not production code: every existing test in
`contracts/outcome-token/src/test.rs` runs through the shared `setup()`
helper, which calls `env.mock_all_auths()`. That switches the whole test
environment into "recording" mode where *every* `require_auth()` call
succeeds unconditionally, regardless of who's actually calling. That means
none of the existing `mint`/`burn` tests actually exercised the
`market_contract.require_auth()` gate — they would keep passing even if that
line were deleted.

## What this change adds

No production code changes were needed. `contracts/outcome-token/src/test.rs`
gains:

- `setup_unmocked` — a second test harness that does *not* call
  `mock_all_auths()`, instead using `env.mock_auths(&[...])` scoped to just
  the `initialize` call so the contract can still be bootstrapped.
- `mint_succeeds_when_authorized_by_market_contract` /
  `burn_succeeds_when_authorized_by_market_contract` — mock a `MockAuth`
  entry for exactly the registered `market_contract` address and the exact
  `mint`/`burn` invocation, and assert the call succeeds and balances update.
  This is the "market contract path succeeds" acceptance criterion.
- `mint_fails_without_market_contract_authorization` /
  `burn_fails_without_market_contract_authorization` — call `mint`/`burn`
  with *no* mocked auth at all and assert (`#[should_panic]`) that the call
  panics, since nothing can satisfy `market_contract.require_auth()` without
  holding that contract's authorization. This is the "external EOAs cannot
  mint" acceptance criterion — a bare `Address::generate` caller has no way
  to produce a valid authorization for the market contract's address.
