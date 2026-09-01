# Reentrancy & Checks-Effects-Interactions (CEI) Audit Report

## Audit Scope
- `contracts/market/src/withdraw.rs`
- `contracts/market/src/settlement.rs`
- `contracts/market/src/deposit.rs` (Issue #695)
- `contracts/treasury/src/lib.rs` (Issue #695)
- `contracts/resolution/src/lib.rs` (Issue #695)
- `contracts/outcome-token/src/lib.rs` (Issue #695)

## Summary of Findings

| Contract | Function | Issue / Order Violation | Severity | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Market** | `withdraw_unused_collateral` | External fee transfer & `collect_fee` call occurred **before** `storage::set_position`. | High | **Fixed** |
| **Market** | `settle_position` | Outcome token `burn()` external call occurred **before** `storage::set_position`. | Medium | **Fixed** |
| **Market** | `deposit_collateral` | External collateral `transfer()` occurred **before** `storage::set_position` / `add_market_participant` / `set_last_deposit_time`. | Medium | **Fixed** |
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

### 3. `deposit_collateral` (`market/src/deposit.rs`, Issue #695)
- **Before:** `token_client.transfer(&user, &contract_address, &amount)` ran first; `storage::set_position`, `storage::add_market_participant`, and `storage::set_last_deposit_time` all ran after it.
- **After:** All three state writes now run first; the collateral transfer is the last thing the function does. The pre-existing `DepositReentrancyGuard` (Issue #501) — a storage-backed lock held for the duration of the call and released on `Drop` — already blocked a second, fully-reentrant call into `deposit_collateral` regardless of ordering, so this reorder is defense-in-depth rather than a closure of an open exploit: it keeps this function consistent with the CEI pattern used everywhere else in this crate, and removes the (mitigated but still theoretically reachable via a differently-shaped reentrant call) window where a malicious/upgraded collateral token could observe or act on a partially-updated position mid-transfer.

### 4. `withdraw_fees` (`treasury/src/lib.rs`, Issue #695)
- **Before:** `token::Client::new(&env, &token).transfer(&treasury, &to, &amount)` ran first; `storage::set_token_balance` (the decremented balance) and `storage::set_total_collected` were only persisted afterward.
- **After:** Both storage writes now happen **before** the transfer. A malicious/upgraded `token` contract that reentered a balance-reading entry point (e.g. a second `withdraw_fees` call, if it could somehow re-authorize) from inside its own `transfer` implementation would previously have observed the stale, not-yet-decremented balance and could have doubly withdrawn against it.

### 5. `distribute_fees` (`treasury/src/lib.rs`, Issue #695)
- **Before:** Each stakeholder's `token_client.transfer(&treasury, &stakeholder, &amount)` fired **inside** the same loop that accumulated `distributed`; `storage::set_token_balance` with the reduced remainder was only written once the loop (and every transfer in it) had completed.
- **After:** The function now runs in two passes — first it computes every stakeholder's payout amount and the resulting `remaining` balance and persists that via `storage::set_token_balance`, and only then iterates a second time to fire the actual transfers. This closes the window where a reentrant call mid-distribution would have read the pre-distribution balance instead of the post-distribution one.

### 6. `propose` (`resolution/src/lib.rs`, Issue #695)
- **Before:** `token_client.transfer(&proposer, &env.current_contract_address(), &bond_amount)` (locking the proposer's bond) ran before `storage::set_candidate` persisted the new `ResolutionCandidate`. The early `CandidateAlreadyExists` guard only consults storage, so it cannot see a proposal that hasn't been persisted yet.
- **After:** The candidate is built and persisted via `storage::set_candidate` (and its `CandidateProposed` event emitted) **before** the bond transfer. A reentrant call into `propose` for the same `market_id` from inside a malicious collateral token's `transfer` can no longer slip past `CandidateAlreadyExists`, because the first call's candidate is now recorded before the transfer that could trigger reentrancy even executes.

### 7. `challenge` (`resolution/src/lib.rs`, Issue #695)
- **Before:** `TokenClient::new(&env, &collateral_token).transfer(&challenger, &this, &bond_amount)` ran before `candidate.status` was updated to `Challenged` and persisted via `storage::set_candidate`/`storage::append_challenger`.
- **After:** The status transition and challenger record are persisted **first**; the bond transfer runs last. A reentrant call into `challenge` for the same `candidate_id` can no longer observe the pre-challenge status and post a second, inconsistent challenge before the first one's state has landed.

### 8. `deposit_collateral` (`resolution/src/lib.rs`, Issue #695)
- **Before:** `TokenClient::new(&env, &collateral_token).transfer(&proposer, &env.current_contract_address(), &amount)` ran before `storage::set_proposer_collateral` persisted the increased balance; `prev` was read once, before the transfer.
- **After:** `storage::set_proposer_collateral(&env, &proposer, prev + amount)` now runs **before** the transfer, so a reentrant call can no longer read the same stale `prev` and overwrite (rather than accumulate) one of two concurrent deposits.

---

## Notes on findings left unfixed

### `void_market` (`resolution/src/lib.rs`) — Low risk, not fixed
`candidate.status` is set to `Voided` and persisted via `storage::set_candidate` **before** `split_bond` (which transfers/burns the proposer's forfeited bond) and the challenger-refund loop run. That ordering is already CEI-correct for the state that actually gates re-entry (`require_arbitrable` checks `candidate.status == Challenged`, which is no longer true once voided). The one remaining out-of-order step is `storage::clear_challengers`, which runs *after* the refund loop's transfers rather than before — but nothing reads the challengers list again within this call or is gated by its absence, and `void_market` is admin-only (`admin.require_auth()`), which substantially narrows the realistic threat model compared to the user-facing entry points above. Left as-is to avoid touching a terminal, already-guarded admin path without a concrete exploit to close.

### `transfer` (`outcome-token/src/lib.rs`) — Low risk, not fixed
`transfer` calls `env.invoke_contract(&config.market_contract, "get_market_status", ...)` before updating either party's balance. This is a **read-only view call**, not a value-moving external call, and `config.market_contract` is a fixed address the outcome-token admin registers — not something the caller of `transfer` controls — so the realistic reentrancy surface here is materially different from the value-transfer cases fixed above. Reordering would not change anything material (the call carries no state that needs to land before it executes), so it is documented rather than restructured.

### `collect_fee` (`treasury/src/lib.rs`) — No violation
`collect_fee` never makes an external token call itself; the actual token movement happens on the caller's side (a market contract's fee-routing code, already covered by the `withdraw_unused_collateral` entry above) before it invokes `collect_fee` to record the accounting entry. There is nothing to reorder within this function.

---

## Issues #752–#755 — Resolution audit additions (no new CEI concerns)

The following changes introduced by Issues #752–#755 carry no new reentrancy or
CEI implications:

- **#752 — `get_factory` / `get_market_contract` / `get_admin` getters**: Pure
  storage reads; no external calls, no state writes, no CEI ordering concern.

- **#753 — Bond constant visibility (`MIN_BOND_AMOUNT`, `MIN_CHALLENGE_BOND_AMOUNT`)**: The
  constants were changed from `const` to `pub const`. No CEI impact. The bond
  transfer in `propose` / `challenge` was already ordered after all state writes
  (see §6 and §7 above); making the floor constants visible for testing does not
  alter the ordering.

- **#754 — `finalize` open-caller documentation**: No code change to `finalize`
  itself. The existing implementation already uses the keeper model and is
  already CEI-compliant (see table row above: "Already CEI-compliant — status
  persisted and bond settlement computed before every external transfer").

- **#755 — `market_id_to_string` ABI bridge documentation**: No code change.
  The `market_id_to_string` helper is a pure string-formatting function with no
  external calls or state writes. The cross-contract call to `resolve_market`
  that consumes its output already runs after all state writes in `finalize`
  and `arbitrate_uphold_proposer` (see CEI table above).
