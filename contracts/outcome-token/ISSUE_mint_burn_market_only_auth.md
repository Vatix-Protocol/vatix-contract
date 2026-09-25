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

## Invariants

- `mint` and `burn` are callable only by the address stored in
  `config.market_contract`; every other caller is denied by default.
- Authorization is checked *before* any balance mutation, so a denied call
  cannot partially apply state changes (fail-closed).
- The market contract address is fixed at `initialize` time and cannot be
  changed by an unprivileged caller.

## Edge cases & failure modes

- **Unauthorized caller (EOA or other contract):** `require_auth()` on the
  market contract address cannot be satisfied, so the call panics and no
  balance is written.
- **Replayed / concurrent requests:** `mint`/`burn` are pure state
  transitions keyed on the caller's authorization; a replayed invocation
  still requires a fresh valid authorization for the market contract, so it
  cannot be replayed by an untrusted party. Idempotency at the market layer
  is the market contract's responsibility.
- **Auth expiry / wrong role:** any address other than the registered
  market contract fails the `require_auth()` gate.
- **Testnet vs mainnet / address drift:** the market contract address is
  supplied at `initialize`; a mismatched address simply fails the gate.

## Security considerations

- The contract remains the source of truth for balances; mint/burn cannot
  be driven by untrusted clients.
- Deny-by-default: no privileged surface is reachable without the market
  contract's authorization.
- No secrets are read or logged on this path.

## Test plan

- Unit tests for the market-only invariant and auth negatives (see above).
- CI stays green; the new tests run under the existing `cargo test` job.

## Acceptance criteria

- [x] Behavior for 'outcome mint burn auth' matches cited docs.
- [x] Authz/fail-closed covered by tests.
- [x] Docs updated; mainnet safety respected.
- [x] Rollback: no production code change, so reverting the test file is
  sufficient.

## Out of scope

Unrelated refactors; irreversible mainnet without readiness checklist.
