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
//!
//! ## Spoofing (#926)
//!
//! A waiver can only be granted by the stored admin. Passing the admin's
//! address as `caller` is not enough: `caller.require_auth()` runs before the
//! equality check, so a transaction lacking the admin's signature aborts.
//! Waiver status is read from contract storage only — no client-supplied
//! flag can make [`effective_fee_rate_bps`] return `0`.

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
/// Emits [`crate::events::FeeWaiverAdded`] with the waived `account` and the
/// acting `admin` (plus `added_at`). The on-chain schema does not carry the
/// updated list length; indexers derive it from the ordered event stream.
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
    storage::set_fee_waivers(env, &waivers);

    crate::events::emit_fee_waiver_added(env, waiver_address, caller);
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

    // No-op fast path (#1062): removing an absent address requires no Vec
    // allocation, no storage rewrite, and no `FeeWaiverRemoved` event — return
    // early before touching the list. (The add-side cap check in
    // `add_fee_waiver` is unaffected; only removal short-circuits here.)
    if !has_fee_waiver(env, waiver_address) {
        return Ok(());
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
        storage::set_fee_waivers(env, &new_waivers);
        crate::events::emit_fee_waiver_removed(env, waiver_address, caller);
    }

    Ok(())
}

/// Return `true` if `address` holds a fee waiver.
///
/// Read-only: performs no auth. Exposed externally as the `get_fee_waiver`
/// contract entrypoint (#1060), so the `#[allow(dead_code)]` that hid this
/// helper from the ABI is removed.
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
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env,
    };

    pub(super) fn setup(env: &Env) -> (Address, Address) {
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
                add_fee_waiver(&env, &admin, &addr).expect("add must succeed while under cap");
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
        let count = env.as_contract(&contract_id, || storage::get_fee_waivers(&env).len());
        assert_eq!(count, storage::MAX_FEE_WAIVERS);

        // Issue #1066 (3): re-adding an address that is already present at
        // capacity is still an idempotent no-op — Ok(()) with no growth.
        // The duplicate check runs before the cap check, so the guard pins
        // the ordering: presence first, cap second.
        let mut first_at_capacity: Option<Address> = None;
        env.as_contract(&contract_id, || {
            let waivers = storage::get_fee_waivers(&env);
            first_at_capacity = Some(waivers.get(0).expect("list must be full"));
        });
        let duplicate = first_at_capacity.expect("list must be full");
        let result = env.as_contract(&contract_id, || add_fee_waiver(&env, &admin, &duplicate));
        assert_eq!(
            result,
            Ok(()),
            "re-adding a present address at capacity must stay a no-op"
        );
        let count = env.as_contract(&contract_id, || storage::get_fee_waivers(&env).len());
        assert_eq!(
            count,
            storage::MAX_FEE_WAIVERS,
            "idempotent re-add at capacity must not grow the list"
        );
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

        let result = env.as_contract(&contract_id, || add_fee_waiver(&env, &non_admin, &waiver));
        assert_eq!(result, Err(ContractError::NotAdmin));

        // Deny-by-default: the unauthorized add must not have mutated state.
        let present = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!present, "unauthorized add must not grant a waiver");
    }

    /// #926: naming the admin as `caller` without the admin's signature must
    /// fail auth before any state is written.
    #[test]
    #[should_panic]
    fn test_spoofed_admin_caller_without_auth_is_rejected() {
        let env = Env::default();
        let (contract_id, admin) = setup(&env);
        let attacker = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let _ = add_fee_waiver(&env, &admin, &attacker);
        });
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

        // Separate harness-contract frames: each `require_auth` address is
        // authorized for exactly one frame under `mock_all_auths`, so the
        // fee-rate write and the waiver add must run in distinct frames.
        env.as_contract(&contract_id, || {
            crate::fee_rate::set_fee_rate_bps(&env, &admin, 250).expect("admin set should succeed");
        });
        env.as_contract(&contract_id, || {
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
            crate::fee_rate::set_fee_rate_bps(&env, &admin, 250).expect("admin set should succeed");
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

    /// #1062 (1): removing an absent address is a no-op — it emits no
    /// `FeeWaiverRemoved` event and leaves the list (and storage) unchanged.
    #[test]
    fn remove_absent_fee_waiver_is_a_noop() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let kept = Address::generate(&env);
        let absent = Address::generate(&env);
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &kept).expect("setup add should succeed");
        });
        // Drain the setup `fee_waiver_added` event before the removal.
        assert_eq!(env.events().all().len(), 1, "setup add must emit one event");

        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &absent).expect("absent remove must not error");
        });

        // No event is emitted for the no-op path.
        assert_eq!(
            env.events().all().len(),
            0,
            "removing an absent address must emit no event"
        );

        // The waiver list is unchanged (no storage rewrite).
        let waivers = env.as_contract(&contract_id, || storage::get_fee_waivers(&env));
        assert_eq!(waivers.len(), 1, "absent remove must not rewrite storage");
        assert_eq!(waivers.get(0).unwrap(), kept);
    }

    /// #1062 (2): removing a present address removes exactly one entry and
    /// emits `FeeWaiverRemoved`.
    #[test]
    fn remove_present_fee_waiver_emits_removed_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let waiver = Address::generate(&env);
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waiver).expect("setup add should succeed");
        });
        assert_eq!(env.events().all().len(), 1, "setup add must emit one event");

        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waiver).expect("present remove must succeed");
        });

        // Exactly one `FeeWaiverRemoved` event is emitted.
        assert_eq!(
            env.events().all().len(),
            1,
            "removing a present address must emit an event"
        );

        // The entry is removed from the list.
        let waivers = env.as_contract(&contract_id, || storage::get_fee_waivers(&env));
        assert_eq!(waivers.len(), 0, "present remove must empty the list");
        let is_waiver = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!is_waiver);
    }

    /// #1062 (3): removing the same address twice is a no-op the second time.
    #[test]
    fn remove_fee_waiver_twice_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let waiver = Address::generate(&env);
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waiver).expect("setup add should succeed");
        });
        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waiver).expect("first remove must succeed");
        });
        assert_eq!(env.events().all().len(), 1, "first remove must emit one event");

        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waiver).expect("second remove must not error");
        });

        // The second removal of an absent address emits no event.
        assert_eq!(
            env.events().all().len(),
            0,
            "the second removal must emit no event"
        );
        let is_waiver = env.as_contract(&contract_id, || has_fee_waiver(&env, &waiver));
        assert!(!is_waiver);
    }

}

#[cfg(test)]
mod fee_waiver_view_tests {
    use super::tests::setup;
    use super::*;
    use crate::MarketContractClient;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    /// The read-only view must reflect the waiver list: present after an
    /// admin add (#1060).
    #[test]
    fn test_get_fee_waiver_returns_true_for_present_address() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let waived = Address::generate(&env);

        // Admin add runs in its own harness frame, so its `require_auth`
        // footprint is consumed exactly once.
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waived).expect("admin add should succeed");
        });

        let client = MarketContractClient::new(&env, &contract_id);
        assert!(
            client.get_fee_waiver(&waived),
            "view must return true for a waived address"
        );
    }

    /// The view must return `false` after the waiver is removed (#1060).
    #[test]
    fn test_get_fee_waiver_returns_false_after_removal() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let waived = Address::generate(&env);

        // Add and remove run in separate harness frames so their
        // `require_auth` footprints are consumed once each.
        env.as_contract(&contract_id, || {
            add_fee_waiver(&env, &admin, &waived).expect("admin add should succeed");
        });
        env.as_contract(&contract_id, || {
            remove_fee_waiver(&env, &admin, &waived).expect("admin remove should succeed");
        });

        let client = MarketContractClient::new(&env, &contract_id);
        assert!(
            !client.get_fee_waiver(&waived),
            "view must return false after the waiver is removed"
        );
    }

    /// The view must return `false` for an address that was never added
    /// (#1060).
    #[test]
    fn test_get_fee_waiver_returns_false_for_never_added_address() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let stranger = Address::generate(&env);

        client.initialize(&admin);

        assert!(
            !client.get_fee_waiver(&stranger),
            "view must return false for a never-added address"
        );
    }

    /// The read-only view performs no auth, so it succeeds without any auth
    /// setup (#1060: negative auth test).
    #[test]
    fn test_get_fee_waiver_requires_no_auth() {
        let env = Env::default();
        // Deliberately no `mock_all_auths()`: the view must not call
        // `require_auth`, so this succeeds even with auth disabled.
        let contract_id = env.register(crate::MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let stranger = Address::generate(&env);

        client.initialize(&admin);

        assert!(!client.get_fee_waiver(&stranger));
    }
}
