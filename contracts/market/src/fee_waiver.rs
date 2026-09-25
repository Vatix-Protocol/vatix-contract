//! Fee-waiver list management.
//!
//! The waiver list is stored as a bounded `Vec<Address>` under
//! [`StorageKey::FeeWaivers`]. The size is hard-capped at
//! [`storage::MAX_FEE_WAIVERS`] to prevent unbounded-Vec griefing attacks
//! where a malicious (or compromised) admin can grow the list without limit
//! and stall transactions by exhausting the per-transaction ledger-entry
//! read budget.
//!
//! ## Griefing vector (closed by this module)
//!
//! Without a cap, an admin calling `add_fee_waiver` in a loop could write a
//! Vec with tens-of-thousands of entries.  Every subsequent call that reads
//! or iterates the list would pay a super-linear cost.  At ~64 KiB per
//! persistent ledger entry and the Soroban read-byte budget, this can make
//! the contract permanently unresponsive.
//!
//! ## Invariant (checked by regression test)
//!
//! `add_fee_waiver` MUST return `FeeWaiverCapReached` when the list already
//! contains `MAX_FEE_WAIVERS` addresses.  Removing this check re-opens the
//! griefing vector.

use crate::error::ContractError;
use crate::storage;
use soroban_sdk::{Address, Env};

/// Add `waiver_address` to the fee-waiver list.
///
/// # Access control
/// Requires `caller` to be the stored admin (checked via `require_auth` +
/// admin equality).
///
/// # Cap
/// Returns [`ContractError::FeeWaiverCapReached`] when the list already
/// contains [`storage::MAX_FEE_WAIVERS`] entries.  This is the sentinel that
/// prevents unbounded-Vec griefing.
///
/// # Idempotent
/// Adding an address that is already on the list is a no-op (does not
/// duplicate the entry, does not error).
///
/// # Events
/// Emits [`FeeWaiverAddedEvent`] with the caller, the new address, and the
/// updated list length.
///
/// # Errors
/// - [`ContractError::NotAdmin`] – `caller` is not the admin
/// - [`ContractError::FeeWaiverCapReached`] – list is at capacity
pub fn add_fee_waiver(
    env: &Env,
    caller: &Address,
    waiver_address: &Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    let admin = storage::get_admin(env);
    if *caller != admin {
        return Err(ContractError::NotAdmin);
    }

    let mut waivers = storage::get_fee_waivers(env);

    // Idempotent: already present → skip.
    for i in 0..waivers.len() {
        if waivers.get(i).unwrap() == *waiver_address {
            return Ok(());
        }
    }

    // Cap enforcement — this is the griefing guard.
    if waivers.len() >= storage::MAX_FEE_WAIVERS {
        return Err(ContractError::FeeWaiverCapReached);
    }

    waivers.push_back(waiver_address.clone());
    let count = waivers.len();
    storage::set_fee_waivers(env, &waivers);

    crate::events::emit_fee_waiver_added(env, caller, waiver_address, count);
    Ok(())
}

/// Remove `waiver_address` from the fee-waiver list.
///
/// # Access control
/// Requires `caller` to be the stored admin.
///
/// # Idempotent
/// Removing an address that is not on the list is a no-op (no error).
///
/// # Events
/// Emits [`FeeWaiverRemovedEvent`] only when an entry was actually removed.
///
/// # Errors
/// - [`ContractError::NotAdmin`] – `caller` is not the admin
pub fn remove_fee_waiver(
    env: &Env,
    caller: &Address,
    waiver_address: &Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    let admin = storage::get_admin(env);
    if *caller != admin {
        return Err(ContractError::NotAdmin);
    }

    let waivers = storage::get_fee_waivers(env);
    let mut new_waivers = soroban_sdk::Vec::new(env);
    let mut found = false;

    for i in 0..waivers.len() {
        let addr = waivers.get(i).unwrap();
        if addr == *waiver_address {
            found = true;
        } else {
            new_waivers.push_back(addr);
        }
    }

    if found {
        let count = new_waivers.len();
        storage::set_fee_waivers(env, &new_waivers);
        crate::events::emit_fee_waiver_removed(env, caller, waiver_address, count);
    }

    Ok(())
}

/// Return `true` if `address` holds a fee waiver.
#[allow(dead_code)]
pub fn has_fee_waiver(env: &Env, address: &Address) -> bool {
    let waivers = storage::get_fee_waivers(env);
    for i in 0..waivers.len() {
        if waivers.get(i).unwrap() == *address {
            return true;
        }
    }
    false
}

/// Resolve the effective fee rate (in basis points) for `account`.
///
/// This is the single source of truth for fee-rate waiver application and is
/// the entrypoint money paths MUST call instead of reading the raw fee rate
/// directly.  It composes the two rules owned by this module:
///
/// 1. **Waiver rule** — if `account` holds a fee waiver (see
///    [`has_fee_waiver`]) the effective rate is `0`.
/// 2. **Fee-rate rule** — otherwise the stored fee rate is returned, already
///    validated to be within [`crate::fee_rate::MAX_FEE_RATE_BPS`] by the
///    setter.
///
/// # Deny-by-default
/// A missing or unset fee rate resolves to `0` (no fee) rather than an
/// implicit non-zero default, so an unconfigured market can never silently
/// charge an unexpected rate.  Waivers are only ever granted by the admin via
/// [`add_fee_waiver`], so an untrusted client cannot self-grant a waiver.
///
/// # Fail-closed
/// This is a pure read of contract storage; it performs no external calls and
/// therefore cannot be made to fail open by an RPC/DB outage.  Callers that
/// need to charge a fee MUST treat a non-zero result as authoritative.
#[allow(dead_code)]
pub fn effective_fee_rate_bps(env: &Env, account: &Address) -> u32 {
    if has_fee_waiver(env, account) {
        return 0;
    }
    crate::fee_rate::get_fee_rate_bps(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup(env: &Env) -> (Address, Address) {
        let contract_id = env.register(crate::MarketContract, ());
        let admin = Address::generate(env);
        env.as_contract(&contract_id, || {
            storage::set_admin(env, &admin);
        });
        (contract_id, admin)
    }

    // ---- regression: cap must prevent unbounded Vec griefing ----
    /// This test MUST remain.  Removing it re-opens the griefing vector.
    #[test]
    fn regression_fee_waiver_cap_prevents_unbounded_growth() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        // Fill the list to exactly MAX_FEE_WAIVERS entries.
        // Each add_fee_waiver call is a separate as_contract invocation to avoid
        // "frame is already authorized" from mock_all_auths.
        for _ in 0..storage::MAX_FEE_WAIVERS {
            let addr = Address::generate(&env);
            env.as_contract(&contract_id, || {
                add_fee_waiver(&env, &admin, &addr)
                    .expect("add must succeed while under cap");
            });
        }

        // One more add must be rejected — this is the cap sentinel.
        let overflow_addr = Address::generate(&env);
        let result = env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &overflow_addr)
        });
        assert_eq!(
            result,
            Err(ContractError::FeeWaiverCapReached),
            "FeeWaiverCapReached must be returned at capacity — \
             removing this check re-opens unbounded-Vec griefing"
        );

        // List length must still be exactly MAX_FEE_WAIVERS.
        let count = env.as_contract(&contract_id, || {
            storage::get_fee_waivers(&env).len()
        });
        assert_eq!(count, storage::MAX_FEE_WAIVERS);
    }

    #[test]
    fn test_add_remove_fee_waiver_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let waiver = Address::generate(&env);

        // Not present before add.
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!present);

        // Add.
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waiver).expect("add should succeed");
        });
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(present);

        // Idempotent add — list length must not grow.
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waiver).expect("idempotent add should not error");
        });
        let count = env.as_contract(&contract_id, || storage::get_fee_waivers(&env).len());
        assert_eq!(count, 1, "idempotent add must not duplicate the entry");

        // Remove.
        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waiver).expect("remove should succeed");
        });
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!present);

        // Idempotent remove — not an error.
        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waiver).expect("idempotent remove should not error");
        });
    }

    #[test]
    fn test_non_admin_cannot_add_fee_waiver() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);
        let non_admin = Address::generate(&env);
        let waiver = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &non_admin, &waiver)
        });
        assert_eq!(result, Err(ContractError::NotAdmin));

        // Deny-by-default: the unauthorized add must not have mutated state.
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!present, "unauthorized add must not grant a waiver");
    }

    #[test]
    fn test_non_admin_cannot_remove_fee_waiver() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let non_admin = Address::generate(&env);
        let waiver = Address::generate(&env);

        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waiver).expect("admin add should succeed");
        });

        let result = env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &non_admin, &waiver)
        });
        assert_eq!(result, Err(ContractError::NotAdmin));

        // The waiver must still be present after the rejected removal.
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(present, "unauthorized remove must not revoke a waiver");
    }

    #[test]
    fn test_effective_fee_rate_waived_account_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let waived = Address::generate(&env);

        env.as_contract(&contract_id, || {
            crate::fee_rate::set_fee_rate_bps(&env, &admin, 250)
                .expect("admin set should succeed");
            add_fee_waiver(&env, &admin, &waived).expect("add should succeed");
        });

        let rate = env.as_contract(&contract_id, || effective_fee_rate_bps(&env, &waived));
        assert_eq!(rate, 0, "waived account must pay zero fee");
    }

    #[test]
    fn test_effective_fee_rate_non_waived_uses_stored_rate() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let other = Address::generate(&env);

        env.as_contract(&contract_id, || {
            crate::fee_rate::set_fee_rate_bps(&env, &admin, 250)
                .expect("admin set should succeed");
        });

        let rate = env.as_contract(&contract_id, || effective_fee_rate_bps(&env, &other));
        assert_eq!(rate, 250, "non-waived account must pay the stored rate");
    }

    #[test]
    fn test_effective_fee_rate_deny_by_default_when_unset() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);
        let account = Address::generate(&env);

        // No fee rate configured and no waiver: must resolve to zero, never an
        // implicit non-zero default.
        let rate = env.as_contract(&contract_id, || effective_fee_rate_bps(&env, &account));
        assert_eq!(rate, 0, "unset fee rate must deny-by-default to zero");
    }
}
