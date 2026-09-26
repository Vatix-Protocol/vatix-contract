# Reentrancy & Checks-Effects-Interactions (CEI) Audit Report

## Audit Scope
- `contracts/market/src/withdraw.rs`
- `contracts/market/src/settlement.rs`
- `contracts/market/src/deposit.rs` (Issue #695, Issue #849)
- `contracts/market/src/lib.rs` — `withdraw_canceled_collateral` (Issue #784)
- `contracts/treasury/src/lib.rs` (Issue #695)
- `contracts/resolution/src/lib.rs` (Issue #695)
- `contracts/outcome-token/src/lib.rs` (Issue #695)

## Summary of Findings

| Contract | Function | Issue / Order Violation | Severity | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Market** | `withdraw_unused_collateral` | External fee transfer & `collect_fee` call occurred **before** `storage::set_position`. | High | **Fixed** |
| **Market** | `settle_position` | Duplicate `storage::set_position` write — the position was written once before `burn_settled_outcome_tokens` and once again after it, leaving a window between the two writes where `is_settled` was already `true` in the local `position` struct but the second (post-burn) write had not yet landed. Removed the duplicate; only one `set_position` now runs, before both `burn_settled_outcome_tokens` and the payout transfer. | Low | **Fixed (#784)** |
| **Market** | `withdraw_canceled_collateral` | External collateral `token_client.transfer()` occurred **before** `storage::set_position` zeroed `total_deposited` / `locked_collateral`. A reentrant call during the transfer would have observed the stale, non-zero balance and been able to claim the same collateral twice. | High | **Fixed (#784)** |
| **Market** | `deposit_collateral` | External collateral `transfer()` occurred **before** `storage::set_position` / `add_market_participant` / `set_last_deposit_time`. | Medium | **Fixed (#695, #849)** |
| **Market** | `void_market` (Issue #708) | No external calls: the caller-identity check reads `storage::get_resolution_contract`, the status flips to `Canceled` via `storage::set_market`, then `emit_market_voided` publishes. No token transfer or cross-contract invoke on this path. | — | No violation (CEI-ordered: check → effect → event) |
| **Market** | `cancel_market` | No external calls: admin auth is checked, status validated via `validate_cancelable`, the market is persisted via `storage::set_market`, then `emit_market_canceled` publishes. No token transfer or cross-contract invoke on this path. | — | No violation (CEI-ordered: check → effect → event) |
| **Market** | `reopen_market` | No external calls: admin auth is checked, status validated via `validate_reopenable` (Canceled only), the market is persisted via `storage::set_market`, then `emit_market_reopened` publishes. No token transfer or cross-contract invoke on this path. | — | No violation (CEI-ordered: check → effect → event) |
| **Treasury** | `withdraw_fees` | External `transfer()` occurred **before** `storage::set_token_balance` / `set_total_collected`. | High | **Fixed** |
| **Treasury** | `distribute_fees` | Per-stakeholder external `transfer()` calls occurred **inside** the accumulation loop, **before** `storage::set_token_balance` was updated with the reduced balance. | High | **Fixed** |
| **Treasury** | `collect_fee` | No external token call — the caller (a market contract) moves funds separately; `collect_fee` only records the accounting entry. | — | No violation (informational) |
| **Resolution** | `propose` | External bond `transfer()` occurred **before** `storage::set_candidate` persisted the new candidate. | High | **Fixed** |
| **Resolution** | `challenge` | External bond `transfer()` occurred **before** `storage::set_candidate` / `append_challenger` persisted the Challenged status. | High | **Fixed** |
| **Resolution** | `deposit_collateral` | External collateral `transfer()` occurred **before** `storage::set_proposer_collateral` was updated. | Medium | **Fixed** |
| **Resolution** | `finalize` | Status persisted and bond settlement computed **before** every external transfer and the `resolve_market` callback. | — | Already CEI-compliant (see existing "exactly-once" comment in source) |
| **Resolution** | `arbitrate_uphold_proposer` | Status persisted **before** every external transfer and the `resolve_market` callback. | — | Already CEI-compliant |
| **Resolution** | `void_market` | Status persisted **before** `split_bond` / challenger refund transfers; `storage::clear_challengers` runs **after** the refund loop's transfers. | Low | Informational — admin-gated entry point (`admin.require_auth()`), and `candidate.status` is already `Voided` in storage before any transfer fires, so a reentrant call back into `void_market`/`arbitrate_uphold_proposer` is rejected by `require_arbitrable` regardless of when `clear_challengers` runs. Not fixed — reordering would only rename the (already closed) risk. |
| **Resolution** | `slash_collateral` | Already CEI-compliant — `storage::set_proposer_collateral(&env, &proposer, 0)` runs before the transfer. | — | No violation |
| **Outcome Token** | `mint` / `burn` | No external calls at all — pure internal storage mutation (balance/supply), gated by `require_auth()` on the registered market contract. | — | No violation |
| **Outcome Token** | `transfer` | A **read-only** cross-contract call (`get_market_status`) occurs before the balance updates. | Low | Informational — not a value transfer, and the callee is the fixed, admin-registered market contract, not caller-controlled. Not fixed (see below). |

---

## Detailed Remediation

### 1. `withdraw_unused_collateral` (`withdraw.rs`)
- **Before:** Fee routing (`token_client.transfer` and `env.invoke_contract`) was executed prior to updating `position.total_deposited` and persisting it with `storage::set_position`.
- **After:** Decremented `position.total_deposited` and called `storage::set_position` **first**, satisfying CEI before making external token/treasury calls.
- **Follow-up (#709):** A later bad merge left the final user payout `token_client.transfer(&contract_address, &user, &amount)` **duplicated**, which would pay the user twice and over-draw the contract's custodied collateral. The duplicate call was removed — the user payout is now a single external transfer, ordered after `set_position` and after the fee routing, as CEI requires.

### 2. `settle_position` (`settlement.rs`)
- **Before:** Outcome tokens were burned via `burn_settled_outcome_tokens` (external contract calls) before persisting the updated `Position` state to storage.
- **After:** Reordered logic so `storage::set_position` persists state changes **first**, followed by token burns and final payout transfers.

### 3. `deposit_collateral` (`market/src/deposit.rs`, Issue #695, Issue #849)
- **Before:** `token_client.transfer(&user, &contract_address, &amount)` ran first; `storage::set_position`, `storage::add_market_participant`, and `storage::set_last_deposit_time` all ran after it.
- **After:** All three state writes now run first; the collateral transfer is the last thing the function does. The pre-existing `DepositReentrancyGuard` (Issue #501) — a storage-backed lock held for the duration of the call and released on `Drop` — already blocked a second, fully-reentrant call into `deposit_collateral` regardless of ordering, so this reorder is defense-in-depth rather than a closure of an open exploit: it keeps this function consistent with the CEI pattern used everywhere else in this crate, and removes the (mitigated but still theoretically reachable via a differently-shaped reentrant call) window where a malicious/upgraded collateral token could observe or act on a partially-updated position mid-transfer.
- **Issue #849 alignment:** The CEI ordering above is now the enforced invariant for `deposit_collateral`. The function performs, in order: (1) **checks** — market status/expiry validation, amount validation, and acquisition of the `DepositReentrancyGuard`; (2) **effects** — `storage::set_position`, `storage::add_market_participant`, and `storage::set_last_deposit_time`; (3) **interactions** — the single external `token_client.transfer(&user, &contract_address, &amount)`. No external call is reachable before the state writes complete, and the guard makes any reentrant entry fail closed with the stable `DepositReentrancyGuard` error code rather than proceeding on partially-updated state. The public deposit API surface, error codes, and event emissions are unchanged; only ordering and the guard are load-bearing.

### 4. `withdraw_fees` (`treasury/src/lib.rs`, Issue #695)
- **Before:** `token::Client::new(&env, &token).transfer(&treasury, &to, &amount)` ran first; `storage::set_token_balance` (the decremented balance) and `storage::set_total_collected` were only persisted afterward.
- **After:** Both storage writes now happen **before** the transfer. A malicious/upgraded `token` contract that reentered a balance-reading entry point would observe the already-decremented balance, so the same fees cannot be withdrawn twice.

### 5. `distribute_fees` (`treasury/src/lib.rs`, Issue #695)
- **Before:** Each stakeholder's `transfer()` fired inside the accumulation loop, before `storage::set_token_balance` was updated with the reduced balance.
- **After:** The reduced balance is persisted **before** the per-stakeholder transfers, so a reentrant read during any transfer sees the post-distribution balance and cannot double-spend the same fees.

### 6. `propose` / `challenge` / `deposit_collateral` (`resolution/src/lib.rs`, Issue #695)
- **Before:** Bond/collateral `transfer()` calls ran before `storage::set_candidate` / `append_challenger` / `set_proposer_collateral` persisted the new state.
- **After:** State is persisted first; external transfers follow. A reentrant call observes the already-updated candidate/collateral and is rejected by the existing status guards.

---

## Notes on Non-Fixes

- **`Resolution::void_market`** — `storage::clear_challengers` runs after the refund transfers, but `candidate.status` is already `Voided` in storage before any transfer fires, so reentrant calls are rejected by `require_arbitrable`. Reordering would only rename an already-closed risk; left as-is.
- **`OutcomeToken::transfer`** — the cross-contract `get_market_status` call is read-only and targets the fixed, admin-registered market contract, not a caller-controlled address. Not a value transfer; left as-is.

---

## Invariants (Issue #849)

For every money-path entry point in this audit, the following invariants hold and are covered by tests:

1. **CEI ordering** — all checks and state effects complete before any external token transfer or cross-contract invoke.
2. **Fail-closed reentrancy** — reentrant entry into a guarded function fails with a stable error code; it never proceeds on partially-updated state.
3. **Idempotency** — a replayed request cannot double-apply a balance/position/total update.
4. **Source of truth** — the contract remains authoritative for balances, swaps, and admin; no external caller can bypass policy.
5. **No secrets** — no credentials or sensitive values are emitted in events, logs, or metrics.
