//! #407: Property-based fuzz tests for `withdraw_unused_collateral`.
//!
//! Random `(yes_shares, no_shares, locked_collateral, total_deposited, amount)`
//! combinations are driven through the withdraw logic to assert invariants.

use crate::storage;
use crate::types::{Market, MarketStatus, Position};
use crate::withdraw::withdraw_unused_collateral;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn arb_state() -> impl Strategy<Value = (i128, i128, i128, i128, i128)> {
    // deposited in 0..10M, locked in 0..=deposited, amount in 1..=deposited+1
    (0i128..=10_000_000i128).prop_flat_map(|deposited| {
        (
            0i128..=1_000_000i128, // yes_shares
            0i128..=1_000_000i128, // no_shares
            0i128..=deposited,     // locked_collateral
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
    /// Invariant A: withdraw never silently over-withdraws.
    /// If `amount > available`, the call must return an error.
    #[test]
    fn prop_withdraw_never_exceeds_available(
        (yes_shares, no_shares, locked, deposited, amount) in arb_state()
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

        let available = if deposited > locked { deposited - locked } else { 0 };

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
