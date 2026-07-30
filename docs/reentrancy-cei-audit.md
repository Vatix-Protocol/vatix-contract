# Reentrancy & Checks-Effects-Interactions (CEI) Audit Report

## Audit Scope
- `contracts/market/src/withdraw.rs`
- `contracts/market/src/settlement.rs`

## Summary of Findings

| Contract | Function | Issue / Order Violation | Severity | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Market** | `withdraw_unused_collateral` | External fee transfer & `collect_fee` call occurred **before** `storage::set_position`. | High | **Fixed** |
| **Market** | `settle_position` | Outcome token `burn()` external call occurred **before** `storage::set_position`. | Medium | **Fixed** |

---

## Detailed Remediation

### 1. `withdraw_unused_collateral` (`withdraw.rs`)
- **Before:** Fee routing (`token_client.transfer` and `env.invoke_contract`) was executed prior to updating `position.total_deposited` and persisting it with `storage::set_position`.
- **After:** Decremented `position.total_deposited` and called `storage::set_position` **first**, satisfying CEI before making external token/treasury calls.

### 2. `settle_position` (`settlement.rs`)
- **Before:** Outcome tokens were burned via `burn_settled_outcome_tokens` (external contract calls) before persisting the updated `Position` state to storage.
- **After:** Reordered logic so `storage::set_position` persists state changes **first**, followed by token burns and final payout transfers.
