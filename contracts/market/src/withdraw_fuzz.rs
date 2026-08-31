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
//! 5. **Fee Rounding**: `fee_amount = floor(amount * fee_rate_bps / 10_000)` never
//!    over/underflows and never exceeds `amount` itself, across the full
//!    `fee_rate_bps` range (0–10_000) and edge `amount`s near zero and near the
//!    `validate_amount_reasonable` ceiling (`i128::MAX / 2`) — see the
//!    "Dust rule" note on [`validation::calculate_fee`] fuzz coverage below.

use crate::error::ContractError;
use crate::positions;
use crate::storage;
use crate::types::{Market, MarketStatus, Position};
use crate::validation;
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
    use crate::types::AdapterType;
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
        resolver: None,
        resolved_at: None,
        adapter_type: AdapterType::Ed25519,
        outcome_count: 2,
        closed_to_deposits: false,
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

/// Withdrawal fee rounding invariants.
///
/// `fee_amount = floor(amount * fee_rate_bps / 10_000)` (`validation::calculate_fee`).
/// Withdraw itself never carves the fee out of `amount` — the user always
/// receives exactly `amount`, and `amount + fee_amount` is deducted from
/// `total_deposited` on top (see `withdraw.rs`'s module doc, #377). So the
/// invariant under test isn't "fee + payout == amount"; it's the pair that
/// actually holds here:
///
/// **Dust rule**: integer division floors, so up to `9999` stroops of
/// `amount * fee_rate_bps` can be lost to rounding on every withdrawal. That
/// dust is never collected by the protocol and never charged to the user
/// beyond the floored `fee_amount` — it simply vanishes below the bps
/// granularity. Formally: `amount * fee_rate_bps == fee_amount * 10_000 + dust`
/// with `0 <= dust < 10_000`, and `0 <= fee_amount <= amount`.
mod fee_rounding_invariants {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        /// General case: fee never overflows, never exceeds `amount`, and the
        /// floor-division dust rule holds for the full valid amount/bps range.
        #[test]
        fn prop_fee_rounding_never_overflows_or_exceeds_amount(
            amount in 1i128..=10_000_000_000i128,
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, fee_rate_bps)
                .expect("calculate_fee must not error for valid inputs");

            prop_assert!(fee_amount >= 0, "fee_amount={fee_amount} negative");
            prop_assert!(fee_amount <= amount,
                "fee_amount={fee_amount} exceeds amount={amount} at bps={fee_rate_bps}");

            let product = amount.checked_mul(fee_rate_bps)
                .expect("amount * fee_rate_bps must not overflow in this range");
            let dust = product - fee_amount * 10_000;
            prop_assert!((0..10_000).contains(&dust),
                "dust={dust} out of [0, 10_000) for amount={amount} bps={fee_rate_bps}");

            // The amount actually deducted from a position (amount + fee) must
            // itself be representable without overflow (#377's on-top-of model).
            let total_required = amount.checked_add(fee_amount);
            prop_assert!(total_required.is_some(),
                "amount + fee_amount overflowed for amount={amount} fee={fee_amount}");
        }

        /// Edge case: amounts near the smallest possible positive withdrawal
        /// (1..=1000 stroops) — where floor-division dust is proportionally
        /// largest and `fee_amount` most often rounds down to exactly zero.
        #[test]
        fn prop_fee_rounding_near_zero_amount(
            amount in 1i128..=1_000i128,
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, fee_rate_bps)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert!(fee_amount >= 0);
            prop_assert!(fee_amount <= amount);

            // At 1 bps (the smallest nonzero rate) any amount below 10_000
            // stroops floors the fee to zero entirely — that's the dust rule,
            // not a bug: the protocol simply forgoes fees below its bps
            // granularity rather than rounding up in its own favor.
            if fee_rate_bps > 0 && amount < 10_000 / fee_rate_bps.max(1) {
                prop_assert_eq!(fee_amount, 0,
                    "expected fee to floor to 0 for tiny amount={amount} at bps={fee_rate_bps}");
            }
        }

        /// Edge case: amounts near `validate_amount_reasonable`'s ceiling
        /// (`i128::MAX / 2`, ~8.5e37) crossed with the full bps range. At
        /// this magnitude `amount * fee_rate_bps` itself can exceed
        /// `i128::MAX` for any bps beyond single digits — well before the
        /// division step. `calculate_fee` must fail closed with
        /// `ArithmeticOverflow` in that case (via `checked_mul`), never
        /// panic or silently wrap to a bogus/negative fee; when the product
        /// *does* fit, the result must still floor-divide correctly.
        #[test]
        fn prop_fee_rounding_near_max_amount(
            amount in (i128::MAX / 2 - 10_000_000_000i128)..=(i128::MAX / 2),
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            validation::validate_collateral_amount(amount)
                .expect("amount must be within the validated reasonable range");

            let result = validation::calculate_fee(amount, fee_rate_bps);

            match amount.checked_mul(fee_rate_bps) {
                Some(product) => {
                    let fee_amount = result
                        .expect("calculate_fee must succeed when amount * fee_rate_bps fits in i128");
                    prop_assert!(fee_amount >= 0 && fee_amount <= amount);
                    prop_assert_eq!(fee_amount, product / 10_000);

                    // amount + fee_amount can exceed i128::MAX/2 (fee is
                    // additive, #377), but must stay within i128 itself.
                    let total_required = amount.checked_add(fee_amount);
                    prop_assert!(total_required.is_some(),
                        "amount + fee_amount overflowed near the amount ceiling: amount={amount} fee={fee_amount}");
                }
                None => {
                    prop_assert_eq!(result, Err(ContractError::ArithmeticOverflow),
                        "expected graceful ArithmeticOverflow (not a panic/wrap) for amount={amount} bps={fee_rate_bps}, got {:?}",
                        result);
                }
            }
        }

        /// At the maximum fee rate (10_000 bps = 100%), fee_amount must equal
        /// amount exactly (no rounding loss at the boundary rate).
        #[test]
        fn prop_fee_rounding_max_bps_equals_amount(
            amount in 1i128..=10_000_000_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, 10_000i128)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert_eq!(fee_amount, amount,
                "expected fee_amount == amount at 10_000 bps, got fee={fee_amount} amount={amount}");
        }

        /// At a zero fee rate, fee_amount must always be exactly zero
        /// regardless of amount (including near the amount ceiling).
        #[test]
        fn prop_fee_rounding_zero_bps_is_always_zero(
            amount in 1i128..=(i128::MAX / 2),
        ) {
            let fee_amount = validation::calculate_fee(amount, 0i128)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert_eq!(fee_amount, 0,
                "expected fee_amount == 0 at 0 bps, got fee={fee_amount} for amount={amount}");
        }
    }
}
