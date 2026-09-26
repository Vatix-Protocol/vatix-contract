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

/// Compute the treasury fee (in stroops) for a given trade notional.
///
/// This is the single source of truth for the fee amount that the withdraw
/// path wires into `treasury::collect_fee`. It is pure and fail-closed:
///
/// * `notional` is the gross amount being settled (must be > 0).
/// * The rate is read from storage (the timelocked value), never from the
///   caller, so an untrusted client cannot influence the fee.
/// * Rounding is explicit and always in favor of the protocol: the fee is
///   computed with ceiling division so truncation can never under-collect.
///   For any `notional > 0` and `rate_bps > 0` the result is `>= 1` stroop,
///   which keeps dust withdrawals from silently bypassing the fee.
/// * The result is capped at `notional` so a misconfigured rate can never
///   make the treasury collect more than the trade itself.
///
/// # Errors
/// - `InvalidAmount` if `notional` is zero or negative
pub fn compute_treasury_fee(env: &Env, notional: i128) -> Result<i128, ContractError> {
    if notional <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let rate_bps = storage::get_fee_rate_bps(env) as i128;
    // rate_bps is bounded by FEE_RATE_MAX_BPS at queue time, but clamp again
    // here so a corrupted/legacy storage value can never over-collect.
    let rate_bps = if rate_bps > FEE_RATE_MAX_BPS as i128 {
        FEE_RATE_MAX_BPS as i128
    } else {
        rate_bps
    };
    // Ceiling division: (notional * rate_bps + 9_999) / 10_000. This rounds
    // the fee up so truncation never under-collects on the withdraw path.
    let numerator = notional
        .checked_mul(rate_bps)
        .ok_or(ContractError::InvalidAmount)?
        .checked_add(9_999)
        .ok_or(ContractError::InvalidAmount)?;
    let fee = numerator / 10_000;
    // Fail-closed: never collect more than the notional itself.
    Ok(if fee > notional { notional } else { fee })
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

    // ---- withdraw fee rounding fuzz (issue #851) ----

    /// Set the stored fee rate directly (bypassing the timelock) so the
    /// rounding invariants can be exercised across the full legal range.
    fn set_rate(env: &Env, contract_id: &Address, rate_bps: u32) {
        env.as_contract(contract_id, || {
            storage::set_fee_rate_bps(env, rate_bps);
        });
    }

    fn fee(env: &Env, contract_id: &Address, notional: i128) -> i128 {
        env.as_contract(contract_id, || compute_treasury_fee(env, notional))
            .expect("fee computation must succeed for positive notional")
    }

    /// Dust and 1-unit withdrawals must still collect at least 1 stroop when
    /// the rate is non-zero — truncation must never round the fee to zero.
    #[test]
    fn test_dust_and_unit_withdrawals_round_up() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);

        for rate in [1u32, 2, 7, 50, 100, 250, FEE_RATE_MAX_BPS] {
            set_rate(&env, &contract_id, rate);
            for notional in [1i128, 2, 3, 9, 10, 99, 100, 101, 9_999, 10_000, 10_001] {
                let f = fee(&env, &contract_id, notional);
                assert!(
                    f >= 1,
                    "fee must be >= 1 stroop for notional={} rate={} (got {})",
                    notional,
                    rate,
                    f
                );
                assert!(
                    f <= notional,
                    "fee must never exceed notional={} rate={} (got {})",
                    notional,
                    rate,
                    f
                );
            }
        }
    }

    /// The fee must equal the exact ceiling of notional * rate / 10_000 for
    /// every amount at and around the fee-rate boundaries.
    #[test]
    fn test_fee_matches_ceiling_division() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);

        for rate in [1u32, 3, 17, 99, 100, 333, FEE_RATE_MAX_BPS] {
            set_rate(&env, &contract_id, rate);
            for notional in [1i128, 2, 3, 7, 10, 100, 1_000, 10_000, 123_456, 1_000_000] {
                let expected = (notional * rate as i128 + 9_999) / 10_000;
                let expected = if expected > notional { notional } else { expected };
                assert_eq!(
                    fee(&env, &contract_id, notional),
                    expected,
                    "ceiling mismatch notional={} rate={}",
                    notional,
                    rate
                );
            }
        }
    }

    /// A zero rate must collect nothing, and a max rate must never over-collect
    /// beyond the notional (fail-closed cap).
    #[test]
    fn test_zero_and_max_rate_bounds() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);

        set_rate(&env, &contract_id, 0);
        for notional in [1i128, 10, 10_000, 1_000_000] {
            assert_eq!(fee(&env, &contract_id, notional), 0);
        }

        set_rate(&env, &contract_id, FEE_RATE_MAX_BPS);
        for notional in [1i128, 10, 10_000, 1_000_000] {
            let f = fee(&env, &contract_id, notional);
            assert!(f >= 1 && f <= notional);
        }
    }

    /// A corrupted/legacy storage rate above the max must be clamped so the
    /// treasury can never over-collect.
    #[test]
    fn test_corrupted_rate_is_clamped() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);

        set_rate(&env, &contract_id, 10_000); // 100 % — above FEE_RATE_MAX_BPS
        let f = fee(&env, &contract_id, 1_000_000);
        let expected = (1_000_000i128 * FEE_RATE_MAX_BPS as i128 + 9_999) / 10_000;
        assert_eq!(f, expected, "rate must be clamped to FEE_RATE_MAX_BPS");
    }

    /// Repeated calls with identical inputs must be deterministic (idempotent
    /// pure computation) — no hidden state drift between withdraw attempts.
    #[test]
    fn test_repeated_calls_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);

        set_rate(&env, &contract_id, 137);
        let first = fee(&env, &contract_id, 987_654);
        for _ in 0..32 {
            assert_eq!(fee(&env, &contract_id, 987_654), first);
        }
    }

    /// Non-positive notionals must fail closed rather than silently returning 0.
    #[test]
    fn test_non_positive_notional_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, _admin) = setup(&env);
        set_rate(&env, &contract_id, 100);

        for notional in [0i128, -1, -10_000] {
            let result = env.as_contract(&contract_id, || compute_treasury_fee(&env, notional));
            assert_eq!(result, Err(ContractError::InvalidAmount));
        }
    }
}
