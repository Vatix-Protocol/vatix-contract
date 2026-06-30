//! #407: Property-based fuzz tests for `withdraw_unused_collateral`.
//!
//! Random `(yes_shares, no_shares, locked_collateral, total_deposited, amount)`
//! combinations are driven through the withdraw logic to assert invariants.
//!
//! ## Invariants Tested
//! 1. **Lock Bound**: `locked_collateral <= total_deposited`
//! 2. **Available Non-Negative**: `available = total_deposited - locked_collateral >= 0`
//! 3. **Withdraw Validation**: Withdraw fails when `amount > available`
//! 4. **Success Preserves Invariant**: Successful withdraw maintains `locked <= total_deposited`

use crate::positions;
use crate::storage;
use crate::types::{Market, MarketStatus, Position};
use crate::withdraw::withdraw_unused_collateral;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

/// Strategy for random position state with valid invariant: locked <= deposited
fn arb_valid_position() -> impl Strategy<Value = (i128, i128, i128, i128)> {
    // deposited in 0..10M, shares in 0..1M, locked derived from shares/price
    (0i128..=10_000_000i128).prop_flat_map(|total_deposited| {
        (
            0i128..=1_000_000i128, // yes_shares
            0i128..=1_000_000i128, // no_shares
            0i128..=10_000i128,    // market_price
            Just(total_deposited),
        )
    })
}

/// Strategy for fuzzing withdraw amount against available collateral
fn arb_withdraw_state() -> impl Strategy<Value = (i128, i128, i128, i128, i128)> {
    (0i128..=10_000_000i128).prop_flat_map(|deposited| {
        (
            0i128..=1_000_000i128, // yes_shares
            0i128..=1_000_000i128, // no_shares
            0i128..=10_000i128,    // price
            Just(deposited),
            1i128..=(deposited + 1), // amount (may exceed available)
        )
    })
}

fn make_market(env: &Env, market_id: u32, collateral_token: &Address) -> Market {
    Market {
        id: market_id,
        question: String::from_str(env, "fuzz?"),
        end_time: 1_000_000,
        oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
        status: MarketStatus::Active,
        result: None,
        creator: Address::generate(env),
        created_at: 0,
        collateral_token: collateral_token.clone(),
        price_bps: 5_000,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    /// Invariant A: withdraw never silently over-withdraws.
    /// If `amount > available`, the call must return an error.
    #[test]
    fn prop_withdraw_never_exceeds_available(
        (yes_shares, no_shares, price, deposited, amount) in arb_withdraw_state()
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = make_market(&env, market_id, &collateral_token);
        // Compute locked from shares/price to ensure valid state
        let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
        // Ensure locked doesn't exceed deposited for valid test cases
        let locked = if locked > deposited { deposited } else { locked };

        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares,
            no_shares,
            locked_collateral: locked,
            total_deposited: deposited,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });

        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
            .mint(&contract_id, &(deposited + amount));

        let available = deposited.saturating_sub(locked);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });

        if amount > available {
            prop_assert!(result.is_err(),
                "expected error: amount={amount} > available={available}");
        }
    }

    /// Invariant B: on success, total_deposited decreases by exactly `amount`
    /// (no fee, no locked shares).
    #[test]
    fn prop_successful_withdraw_decrements_deposited(
        deposited in 1i128..=10_000_000i128,
        amount in 1i128..=10_000_000i128,
    ) {
        prop_assume!(amount <= deposited);

        let env = Env::default();
        env.mock_all_auths();

        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = make_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: deposited,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });

        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
            .mint(&contract_id, &deposited);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });

        prop_assert!(result.is_ok());

        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position exists")
        });
        prop_assert_eq!(updated.total_deposited, deposited - amount);
    }
}

/// Share and collateral invariants - #351
mod share_collateral_invariants {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        /// Invariant: locked_collateral <= total_deposited always holds
        #[test]
        fn prop_locked_never_exceeds_deposited(
            (yes_shares, no_shares, price, deposited) in arb_valid_position()
        ) {
            let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
            // Only test when locked is computed from valid position state
            prop_assert!(locked <= deposited,
                "locked={locked} > deposited={deposited} yes={yes_shares} no={no_shares} price={price}");
        }

        /// Invariant: available = total_deposited - locked is always non-negative
        #[test]
        fn prop_available_non_negative(
            (yes_shares, no_shares, price, deposited) in arb_valid_position()
        ) {
            let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
            let available = deposited.saturating_sub(locked);
            prop_assert!(available >= 0,
                "available={available} negative: deposited={deposited} locked={locked}");
        }

        /// Invariant: after successful deposit, locked <= total_deposited
        #[test]
        fn prop_deposit_preserves_locked_invariant(
            existing_deposited in 0i128..=5_000_000i128,
            new_deposit in 1i128..=5_000_000i128,
        ) {
            let env = Env::default();
            let user = Address::generate(&env);
            let market_id = 1u32;
            let token_admin = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(token_admin);
            let collateral_token = token.address();
            let contract_id = env.register(crate::MarketContract, ());

            let market = make_market(&env, market_id, &collateral_token);

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
                storage::set_market(&env, market_id, &market).unwrap();
            });

            // Create initial position with some shares
            env.as_contract(&contract_id, || {
                let _ = positions::update_position(&env, market_id, &user, 1000, 500, 5000);
            });

            // Get position after shares are set
            let position_before = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).unwrap().unwrap()
            });

            // Deposit additional collateral
            env.mock_all_auths();
            soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
                .mint(&user, &(existing_deposited + new_deposit));

            env.as_contract(&contract_id, || {
                crate::deposit::deposit_collateral(env.clone(), user.clone(), market_id, new_deposit)
            }).unwrap();

            // Verify invariant holds after deposit
            let position_after = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).unwrap().unwrap()
            });

            prop_assert!(position_after.locked_collateral <= position_after.total_deposited,
                "invariant broken: locked={} > total={}",
                position_after.locked_collateral, position_after.total_deposited);
        }

        /// Invariant: withdrawing available collateral preserves locked <= deposited
        #[test]
        fn prop_withdraw_preserves_locked_invariant(
            deposited in 100i128..=1_000_000i128,
            withdraw_amount in 1i128..=100_000i128,
        ) {
            prop_assume!(withdraw_amount <= deposited);

            let env = Env::default();
            env.mock_all_auths();

            let user = Address::generate(&env);
            let market_id = 1u32;
            let token_admin = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(token_admin);
            let collateral_token = token.address();
            let contract_id = env.register(crate::MarketContract, ());

            let market = make_market(&env, market_id, &collateral_token);

            let position = Position {
                market_id,
                user: user.clone(),
                yes_shares: 0,
                no_shares: 0,
                locked_collateral: 0,
                total_deposited: deposited,
                is_settled: false,
            };

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
                storage::set_market(&env, market_id, &market).unwrap();
                storage::set_position(&env, market_id, &user, &position).unwrap();
            });

            soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
                .mint(&contract_id, &deposited);

            let result = env.as_contract(&contract_id, || {
                withdraw_unused_collateral(env.clone(), user.clone(), market_id, withdraw_amount)
            });

            if result.is_ok() {
                let updated = env.as_contract(&contract_id, || {
                    storage::get_position(&env, market_id, &user).unwrap().unwrap()
                });
                prop_assert!(updated.locked_collateral <= updated.total_deposited,
                    "invariant broken after withdraw: locked={} > total={}",
                    updated.locked_collateral, updated.total_deposited);
            }
        }

        /// Invariant: position update recalculates locked from shares
        #[test]
        fn prop_position_update_recalculates_locked(
            initial_yes in 0i128..=10_000i128,
            initial_no in 0i128..=10_000i128,
            yes_delta in -5_000i128..=5_000i128,
            no_delta in -5_000i128..=5_000i128,
            price in 0i128..=10_000i128,
        ) {
            let env = Env::default();
            let user = Address::generate(&env);
            let market_id = 1u32;
            let contract_id = env.register(crate::MarketContract, ());

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
            });

            // Initial position
            let initial_locked = positions::calculate_locked_collateral(initial_yes, initial_no, price);

            // Update position
            let result = env.as_contract(&contract_id, || {
                positions::update_position(&env, market_id, &user, yes_delta, no_delta, price)
            });

            // If update succeeded, verify locked matches computed value
            if let Ok(pos) = result {
                let expected_locked = positions::calculate_locked_collateral(
                    pos.yes_shares, pos.no_shares, price
                );
                prop_assert_eq!(pos.locked_collateral, expected_locked,
                    "locked mismatch: expected={}, got={}", expected_locked, pos.locked_collateral);
            }
        }
    }
}
