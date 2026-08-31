//! #354: Property tests using proptest to verify the invariant
//! `locked_collateral <= total_deposited` holds after every deposit,
//! update_position, and withdraw_unused_collateral operation.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn setup_market_with_deposit(deposit: i128) -> (Env, Address, u32, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    (env, contract_id, market_id, user)
}

fn check_invariant(env: &Env, contract_id: &Address, market_id: u32, user: &Address) {
    let position = env.as_contract(contract_id, || {
        storage::get_position(env, market_id, user)
            .unwrap()
            .expect("position should exist")
    });
    assert!(
        position.locked_collateral <= position.total_deposited,
        "invariant violated: locked={} > deposited={}",
        position.locked_collateral,
        position.total_deposited
    );
    assert!(
        position.locked_collateral >= 0,
        "invariant violated: locked_collateral is negative: {}",
        position.locked_collateral
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// After a single deposit, locked_collateral is 0 and total_deposited equals
    /// the deposited amount — no trade has been placed yet.
    #[test]
    fn prop_fresh_deposit_has_zero_locked(deposit_usdc in 1u64..=1000u64) {
        let deposit = (deposit_usdc as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market_with_deposit(deposit);
        check_invariant(&env, &contract_id, market_id, &user);

        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position should exist")
        });
        prop_assert_eq!(position.locked_collateral, 0);
        prop_assert_eq!(position.total_deposited, deposit);
    }

    /// After buying YES shares, locked_collateral == yes_shares * price_bps / 10_000
    /// and must not exceed total_deposited.
    #[test]
    fn prop_yes_position_locked_le_deposited(
        deposit_usdc in 1u64..=500u64,
        price_bps in 1i128..=9_999i128,
        share_pct in 0u64..=100u64,
    ) {
        let deposit = (deposit_usdc as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market_with_deposit(deposit);
        let client = MarketContractClient::new(&env, &contract_id);

        let yes_shares = deposit * share_pct as i128 / 100;
        if yes_shares > 0 {
            let _ = client.try_update_position(&user, &market_id, &yes_shares, &0i128, &price_bps);
        }

        check_invariant(&env, &contract_id, market_id, &user);
    }

    /// After buying NO shares, the invariant still holds.
    #[test]
    fn prop_no_position_locked_le_deposited(
        deposit_usdc in 1u64..=500u64,
        price_bps in 1i128..=9_999i128,
        share_pct in 0u64..=100u64,
    ) {
        let deposit = (deposit_usdc as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market_with_deposit(deposit);
        let client = MarketContractClient::new(&env, &contract_id);

        let no_shares = deposit * share_pct as i128 / 100;
        if no_shares > 0 {
            let _ = client.try_update_position(&user, &market_id, &0i128, &no_shares, &price_bps);
        }

        check_invariant(&env, &contract_id, market_id, &user);
    }

    /// After any sequence of deposit → trade → withdraw steps, the invariant holds.
    #[test]
    fn prop_invariant_survives_deposit_trade_withdraw_sequence(
        deposit_usdc in 1u64..=200u64,
        price_bps in 1i128..=9_999i128,
        yes_pct in 0u64..=100u64,
        withdraw_pct in 1u64..=100u64,
    ) {
        let deposit = (deposit_usdc as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market_with_deposit(deposit);
        let client = MarketContractClient::new(&env, &contract_id);

        // Trade
        let yes_shares = deposit * yes_pct as i128 / 100;
        if yes_shares > 0 {
            let _ = client.try_update_position(&user, &market_id, &yes_shares, &0i128, &price_bps);
        }
        check_invariant(&env, &contract_id, market_id, &user);

        // Attempt withdrawal (may fail if locked — that's expected)
        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position should exist")
        });
        let available = (position.total_deposited - position.locked_collateral).max(0);
        if available > 0 {
            let withdraw_amount = (available * withdraw_pct as i128 / 100).max(1);
            let _ = client.try_withdraw_unused_collateral(&user, &market_id, &withdraw_amount);
        }

        check_invariant(&env, &contract_id, market_id, &user);
    }

    /// Invariant holds when both yes and no shares are bought at different prices.
    #[test]
    fn prop_mixed_position_locked_le_deposited(
        deposit_usdc in 2u64..=200u64,
        price_yes in 1i128..=9_999i128,
        price_no in 1i128..=9_999i128,
        yes_pct in 0u64..=50u64,
        no_pct in 0u64..=50u64,
    ) {
        let deposit = (deposit_usdc as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market_with_deposit(deposit);
        let client = MarketContractClient::new(&env, &contract_id);

        let yes_shares = deposit * yes_pct as i128 / 100;
        if yes_shares > 0 {
            let _ = client.try_update_position(&user, &market_id, &yes_shares, &0i128, &price_yes);
        }
        check_invariant(&env, &contract_id, market_id, &user);

        let no_shares = deposit * no_pct as i128 / 100;
        if no_shares > 0 {
            let _ = client.try_update_position(&user, &market_id, &0i128, &no_shares, &price_no);
        }
        check_invariant(&env, &contract_id, market_id, &user);
    }
}
