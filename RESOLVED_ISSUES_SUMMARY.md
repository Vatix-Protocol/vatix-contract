# Four Issues Resolved - Summary & Detailed Breakdown

This repository now contains complete solutions, test coverage, and documentation for four critical protocol issues spanning `outcome-token`, `treasury`, `market`, and position view error reporting.

---

## Issue 1: Outcome-Token Mint/Burn Market-Only Authorization
- **Location**: `contracts/outcome-token/src/test.rs`
- **Documentation**: `contracts/outcome-token/ISSUE_mint_burn_market_only_auth.md`
- **Problem**: `mint` and `burn` in `OutcomeTokenContract` enforce `config.market_contract.require_auth()`, but previous unit test harness called `env.mock_all_auths()`, which bypasses authorization checks entirely.
- **Fix**: Added `setup_unmocked` in `test.rs` using scoped `env.mock_auths` without blanket auth mocking. Added test cases verifying authorized calls succeed and unauthorized calls panic (`should_panic`).

---

## Issue 2: Treasury `collect_fee` Authorized-Market Enforcement
- **Location**: `contracts/treasury/src/test.rs`
- **Documentation**: `contracts/treasury/ISSUE_collect_fee_authorization.md`
- **Problem**: `collect_fee` must enforce that only authorized market contract addresses registered in `AuthorizedMarkets` can deposit fees into the treasury.
- **Fix**: Added tests in `test.rs` for market removal and re-registration round-trips (`re_added_market_can_collect_fee_again`) and never-registered callers (`collect_fee_rejects_caller_never_registered`), confirming rejection with `TreasuryError::CallerNotMarket`.

---

## Issue 3: Withdraw Fee Rounding Fuzz Invariants
- **Location**: `contracts/market/src/withdraw_fuzz.rs`
- **Documentation**: `contracts/market/ISSUE_withdraw_fee_rounding_fuzz.md`
- **Problem**: Fee calculations in `validation::calculate_fee` needed comprehensive property-based fuzz testing across edge amounts near zero, maximum bounds, and BPS range.
- **Fix**: Created `fee_rounding_invariants` proptest suite (2,000 cases per property) testing zero amounts, ceiling amounts (`i128::MAX / 2`), boundary fee rates (0 BPS, 10,000 BPS), and asserting `ContractError::ArithmeticOverflow` on product overflow. Documented floor-division dust handling.

---

## Issue 4: Position Views Explicit Market NotFound Error Validation
- **Location**: `contracts/market/src/lib.rs`, `tests/get_position_unknown_market_test.rs`
- **Documentation**: `docs/issue-market-not-found-views.md`
- **Problem**: `get_position` and `get_net_position` read directly from position storage, returning `Ok(None)` / `Ok(0)` when given an invalid `market_id`, masking non-existent markets as empty user positions.
- **Fix**: Added `storage::has_market(&env, market_id)` check at the start of both views, returning `Err(ContractError::MarketNotFound)` if the market does not exist. Added integration tests in `tests/get_position_unknown_market_test.rs`.
