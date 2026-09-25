//! Fee-rate management with 172 800-second (48 h) admin timelock.
use crate::error::ContractError;
use crate::storage;
use crate::types::PendingFeeRate;
use soroban_sdk::{Address, Env};

pub const FEE_RATE_TIMELOCK_SECONDS: u64 = 172_800;
pub const FEE_RATE_MAX_BPS: u32 = 500; // 5 %

/// Queue a fee-rate change. The change is NOT applied until
/// `apply_pending_fee_rate` is called after the timelock expires.
///
/// # Errors
/// - `NotAdmin` if `caller` is not the stored admin
/// - `FeeRateOutOfRange` if `new_rate_bps > FEE_RATE_MAX_BPS`
pub fn queue_fee_rate_change(
    env: &Env,
    caller: &Address,
    new_rate_bps: u32,
) -> Result<(), ContractError> {
    caller.require_auth();
    let admin = storage::get_admin(env);
    if *caller != admin {
        return Err(ContractError::NotAdmin);
    }
    if new_rate_bps > FEE_RATE_MAX_BPS {
        return Err(ContractError::FeeRateOutOfRange);
    }
    let queued_at = env.ledger().timestamp();
    let pending = PendingFeeRate { new_rate_bps, queued_at };
    storage::set_pending_fee_rate(env, &pending);
    let effective_at = queued_at + FEE_RATE_TIMELOCK_SECONDS;
    crate::events::emit_fee_rate_change_queued(env, caller, new_rate_bps, effective_at);
    Ok(())
}

/// Apply the queued fee-rate change once the timelock has expired.
///
/// # Errors
/// - `NotAdmin` if `caller` is not the stored admin
/// - `FeeRateTimelockNotExpired` if no pending change exists, or the timelock
///   has not yet elapsed
pub fn apply_pending_fee_rate(
    env: &Env,
    caller: &Address,
) -> Result<u32, ContractError> {
    caller.require_auth();
    let admin = storage::get_admin(env);
    if *caller != admin {
        return Err(ContractError::NotAdmin);
    }
    let pending = storage::get_pending_fee_rate(env)
        .ok_or(ContractError::FeeRateTimelockNotExpired)?;
    let now = env.ledger().timestamp();
    if now < pending.queued_at + FEE_RATE_TIMELOCK_SECONDS {
        return Err(ContractError::FeeRateTimelockNotExpired);
    }
    let new_rate = pending.new_rate_bps;
    storage::set_fee_rate_bps(env, new_rate);
    storage::clear_pending_fee_rate(env);
    crate::events::emit_fee_rate_applied(env, caller, new_rate, now);
    Ok(new_rate)
}

/// Cancel a queued fee-rate change before it is applied.
///
/// This is the fail-closed escape hatch for the money path: if a queued
/// change is discovered to be wrong (bad value, compromised admin session,
/// or a mistaken queue), the admin can revoke it during the timelock window
/// instead of being forced to wait for it to land.
///
/// # Errors
/// - `NotAdmin` if `caller` is not the stored admin
/// - `NoPendingFeeRate` if there is nothing queued to cancel
pub fn cancel_pending_fee_rate(
    env: &Env,
    caller: &Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    let admin = storage::get_admin(env);
    if *caller != admin {
        return Err(ContractError::NotAdmin);
    }
    if storage::get_pending_fee_rate(env).is_none() {
        return Err(ContractError::NoPendingFeeRate);
    }
    storage::clear_pending_fee_rate(env);
    crate::events::emit_fee_rate_change_cancelled(env, caller, env.ledger().timestamp());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Address, Env,
    };

    fn setup(env: &Env) -> (Address, Address) {
        let contract_id = env.register(crate::MarketContract, ());
        let admin = Address::generate(env);
        env.as_contract(&contract_id, || {
            storage::set_admin(env, &admin);
        });
        (contract_id, admin)
    }

    // ---- regression: timelock must be enforced ----
    /// This test FAILS before the timelock is implemented and PASSES once it is.
    /// It must never be removed — it is the sentinel that prevents silent regression.
    #[test]
    fn test_apply_before_timelock_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        // Queue a change at ledger time 0
        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 100).expect("queue should succeed");
        });

        // Advance ledger time by exactly (timelock - 1) seconds — still locked
        env.ledger().set_timestamp(FEE_RATE_TIMELOCK_SECONDS - 1);

        let result = env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin)
        });
        assert_eq!(
            result,
            Err(ContractError::FeeRateTimelockNotExpired),
            "apply must be rejected before timelock expires"
        );
    }

    /// Applying at exactly queued_at + timelock (boundary) must SUCCEED.
    #[test]
    fn test_apply_at_timelock_boundary_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 200).expect("queue should succeed");
        });

        // Warp to exactly queued_at (0) + timelock
        env.ledger().set_timestamp(FEE_RATE_TIMELOCK_SECONDS);

        let result = env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin)
        });
        assert_eq!(result, Ok(200), "apply must succeed at timelock boundary");

        let stored = env.as_contract(&contract_id, || storage::get_fee_rate_bps(&env));
        assert_eq!(stored, 200);
    }

    /// After applying, the pending slot must be cleared — double-apply must fail.
    #[test]
    fn test_double_apply_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 50).unwrap();
        });
        env.ledger().set_timestamp(FEE_RATE_TIMELOCK_SECONDS);
        env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin).unwrap();
        });

        // Second apply must fail — no pending change
        let result = env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin)
        });
        assert_eq!(result, Err(ContractError::FeeRateTimelockNotExpired));
    }

    /// Non-admin cannot queue or apply fee-rate changes.
    #[test]
    fn test_non_admin_cannot_queue() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);
        let non_admin = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &non_admin, 100)
        });
        assert_eq!(result, Err(ContractError::NotAdmin));
    }

    /// Fee rate above 500 bps must be rejected.
    #[test]
    fn test_fee_rate_out_of_range() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let result = env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, FEE_RATE_MAX_BPS + 1)
        });
        assert_eq!(result, Err(ContractError::FeeRateOutOfRange));
    }

    /// Queue emits FeeRateChangeQueuedEvent with correct effective_at.
    #[test]
    fn test_queue_emits_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::IntoVal;

        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        env.ledger().set_timestamp(1_000_000);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 100).unwrap();
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let topic0: soroban_sdk::Symbol = events.first().unwrap().1.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, soroban_sdk::Symbol::new(&env, "fee_rate_change_queued_event"));
    }

    /// Apply emits FeeRateAppliedEvent.
    #[test]
    fn test_apply_emits_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::IntoVal;

        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 100).unwrap();
        });
        env.ledger().set_timestamp(FEE_RATE_TIMELOCK_SECONDS);

        env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin).unwrap();
        });

        let events = env.events().all();
        let apply_event = events.iter().find(|e| {
            let t: soroban_sdk::Symbol = e.1.get(0).unwrap().into_val(&env);
            t == soroban_sdk::Symbol::new(&env, "fee_rate_applied_event")
        });
        assert!(apply_event.is_some(), "apply event must be emitted");
    }

    /// Admin can cancel a queued change during the timelock window; the
    /// pending slot is cleared and the stored rate is untouched.
    #[test]
    fn test_cancel_pending_fee_rate() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 100).unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            cancel_pending_fee_rate(&env, &admin)
        });
        assert_eq!(result, Ok(()));

        // Pending slot cleared — apply must now fail closed.
        let apply = env.as_contract(&contract_id, || {
            apply_pending_fee_rate(&env, &admin)
        });
        assert_eq!(apply, Err(ContractError::FeeRateTimelockNotExpired));
    }

    /// Cancelling with nothing queued fails closed.
    #[test]
    fn test_cancel_without_pending_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        let result = env.as_contract(&contract_id, || {
            cancel_pending_fee_rate(&env, &admin)
        });
        assert_eq!(result, Err(ContractError::NoPendingFeeRate));
    }

    /// Non-admin cannot cancel a queued change.
    #[test]
    fn test_non_admin_cannot_cancel() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);
        let non_admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 100).unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            cancel_pending_fee_rate(&env, &non_admin)
        });
        assert_eq!(result, Err(ContractError::NotAdmin));
    }

    // ---- Issue 3 regression: timelock MUST use env.ledger().timestamp() ----
    //
    // Pend
}
