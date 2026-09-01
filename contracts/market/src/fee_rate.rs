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

    // ---- Issue 3 regression: timelock MUST use env.ledger().timestamp() ----
    //
    // PendingFeeRate.effective_at is computed as `queued_at + FEE_RATE_TIMELOCK_SECONDS`
    // where `queued_at = env.ledger().timestamp()` at the time of queuing.
    // apply_pending_fee_rate checks `now >= queued_at + FEE_RATE_TIMELOCK_SECONDS`
    // where `now = env.ledger().timestamp()` at the time of applying.
    //
    // This test proves that ONLY the ledger clock matters — arbitrary wall-clock
    // offsets (simulated here by NOT advancing the ledger) are irrelevant, and
    // that advancing the ledger by exactly FEE_RATE_TIMELOCK_SECONDS is sufficient.
    //
    // If apply_pending_fee_rate were changed to use a wall-clock source this test
    // would still pass in CI (where wall time advances), so the companion test
    // test_apply_before_timelock_rejected provides the blocking counterpart.

    /// Confirm that apply_pending_fee_rate uses env.ledger().timestamp().
    ///
    /// Warps the ledger to `queued_at + FEE_RATE_TIMELOCK_SECONDS - 1` and
    /// asserts rejection, then warps to exactly `queued_at + FEE_RATE_TIMELOCK_SECONDS`
    /// and asserts acceptance.  Both checks must pass for the invariant to hold.
    ///
    /// This is the Issue-3 sentinel test — **do not remove**.
    #[test]
    fn regression_fee_rate_timelock_uses_ledger_timestamp() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin) = setup(&env);

        // Queue at ledger time 1_000_000 (arbitrary non-zero starting point).
        let queue_time: u64 = 1_000_000;
        env.ledger().set_timestamp(queue_time);

        env.as_contract(&contract_id, || {
            queue_fee_rate_change(&env, &admin, 300).expect("queue should succeed at queue_time");
        });

        // Verify the pending record captures the ledger timestamp, not wall time.
        let pending = env.as_contract(&contract_id, || {
            storage::get_pending_fee_rate(&env)
                .expect("pending fee rate must be set after queue")
        });
        assert_eq!(
            pending.queued_at, queue_time,
            "PendingFeeRate.queued_at must equal env.ledger().timestamp() at queue time — \
             if it equals wall-clock time this is the regression"
        );

        // One second before the boundary — must still be locked.
        env.ledger().set_timestamp(queue_time + FEE_RATE_TIMELOCK_SECONDS - 1);
        let early = env.as_contract(&contract_id, || apply_pending_fee_rate(&env, &admin));
        assert_eq!(
            early,
            Err(ContractError::FeeRateTimelockNotExpired),
            "apply must be rejected one second before ledger boundary — \
             if it succeeded here the timelock is not using env.ledger().timestamp()"
        );

        // Exactly at the boundary — must succeed.
        env.ledger().set_timestamp(queue_time + FEE_RATE_TIMELOCK_SECONDS);
        let on_time = env.as_contract(&contract_id, || apply_pending_fee_rate(&env, &admin));
        assert_eq!(
            on_time,
            Ok(300),
            "apply must succeed at exactly queued_at + FEE_RATE_TIMELOCK_SECONDS — \
             the ledger timestamp is the only clock that counts on-chain"
        );

        // Verify the new rate is persisted.
        let rate = env.as_contract(&contract_id, || storage::get_fee_rate_bps(&env));
        assert_eq!(rate, 300);
    }
}
