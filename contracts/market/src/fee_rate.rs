//! Stored fee rate (basis points) for the market contract.
//!
//! Minimal restoration for the re-wired fee-waiver module (#1060): the
//! stored rate and its cap, plus the setter used by tests. Queued/timelocked
//! rate changes from the pre-#1003 module stay out of scope per #1138.

use crate::error::ContractError;
use soroban_sdk::{Address, Env, Symbol};

/// Maximum fee rate in basis points (100% = 10_000).
pub const MAX_FEE_RATE_BPS: u32 = 10_000;

/// Read the stored fee rate in basis points (`0` when never set).
pub fn get_fee_rate_bps(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&(
            Symbol::new(env, "fee_rate_bps"),
            soroban_sdk::symbol_short!("v1"),
        ))
        .unwrap_or(0)
}

/// Set the stored fee rate in basis points.
///
/// Admin-only: `caller` must equal the stored admin. Validated to be within
/// `0..=MAX_FEE_RATE_BPS`.
pub fn set_fee_rate_bps(env: &Env, caller: &Address, rate_bps: u32) -> Result<(), ContractError> {
    caller.require_auth();
    if *caller != crate::storage::get_admin(env) {
        return Err(ContractError::NotAdmin);
    }
    if rate_bps > MAX_FEE_RATE_BPS {
        return Err(ContractError::InvalidFeeRate);
    }
    env.storage().persistent().set(
        &(
            Symbol::new(env, "fee_rate_bps"),
            soroban_sdk::symbol_short!("v1"),
        ),
        &rate_bps,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as TestAddress;

    /// Issue #1278: only the stored admin may change the rate, and values
    /// outside `0..=MAX_FEE_RATE_BPS` are rejected without touching storage.
    #[test]
    fn test_set_fee_rate_bps_is_admin_only_and_bounded() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let other = <Address as TestAddress>::generate(&env);
        env.as_contract(&contract_id, || {
            crate::storage::set_version(&env);
            crate::storage::set_admin(&env, &admin);
        });

        env.as_contract(&contract_id, || {
            assert_eq!(get_fee_rate_bps(&env), 0);

            set_fee_rate_bps(&env, &admin, 250).unwrap();
            assert_eq!(get_fee_rate_bps(&env), 250);
            set_fee_rate_bps(&env, &admin, 0).unwrap();
            assert_eq!(get_fee_rate_bps(&env), 0);
            set_fee_rate_bps(&env, &admin, MAX_FEE_RATE_BPS).unwrap();
            assert_eq!(get_fee_rate_bps(&env), MAX_FEE_RATE_BPS);

            assert_eq!(
                set_fee_rate_bps(&env, &admin, MAX_FEE_RATE_BPS + 1),
                Err(ContractError::InvalidFeeRate)
            );
            assert_eq!(
                set_fee_rate_bps(&env, &other, 500),
                Err(ContractError::NotAdmin)
            );
            assert_eq!(
                get_fee_rate_bps(&env),
                MAX_FEE_RATE_BPS,
                "rejected setters must not mutate stored rate"
            );
        });
    }
}
