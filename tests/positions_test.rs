//! Workspace integration tests for share trading via `update_position`.

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, MarketParams};

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

/// Register the contract, configure the admin, create a market backed by a real
/// asset, and fund + deposit `deposit` stroops for a fresh user.
///
/// Returns the env, contract id, market id, user, and the user's deposit.
fn market_with_funded_user(deposit: i128) -> (Env, Address, u32, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
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

#[test]
fn buying_shares_updates_position_and_locks_collateral() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buy 100 YES shares at a 50% price -> 50 USDC locked.
    let yes_shares = 100 * STROOPS_PER_USDC;
    let position = client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);
    // The last emitted event should be trade_executed
    assert_event_emitted(&env, "trade_executed_event");

    assert_eq!(position.yes_shares, yes_shares);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.locked_collateral, 50 * STROOPS_PER_USDC);

    // The update is persisted.
    let stored = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .expect("version check ok")
            .expect("position should exist")
    });
    assert_eq!(stored.yes_shares, yes_shares);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn buying_beyond_deposited_collateral_is_rejected() {
    // Only 10 USDC deposited, but 100 YES at 50% needs 50 USDC locked.
    let deposit = 10 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);
}

// ========== Tests for Trading Convenience Functions ==========

#[test]
fn buy_yes_convenience_function_works() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buy 100 YES shares at 60% price using convenience function
    let amount = 100 * STROOPS_PER_USDC;
    let position = client.buy_yes(&user, &market_id, &amount, &6_000i128);

    assert_eq!(position.yes_shares, amount);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);
    
    // Verify event was emitted
    assert_event_emitted(&env, "trade_executed_event");
}

#[test]
fn buy_no_convenience_function_works() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buy 100 NO shares at 60% YES price (40% NO cost)
    let amount = 100 * STROOPS_PER_USDC;
    let position = client.buy_no(&user, &market_id, &amount, &6_000i128);

    assert_eq!(position.yes_shares, 0);
    assert_eq!(position.no_shares, amount);
    assert_eq!(position.locked_collateral, 40 * STROOPS_PER_USDC);
    
    assert_event_emitted(&env, "trade_executed_event");
}

#[test]
fn sell_yes_convenience_function_works() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // First buy 100 YES shares
    let amount = 100 * STROOPS_PER_USDC;
    client.buy_yes(&user, &market_id, &amount, &6_000i128);

    // Then sell 50 YES shares using convenience function
    let sell_amount = 50 * STROOPS_PER_USDC;
    let position = client.sell_yes(&user, &market_id, &sell_amount, &6_000i128);

    assert_eq!(position.yes_shares, 50 * STROOPS_PER_USDC);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.locked_collateral, 30 * STROOPS_PER_USDC);
}

#[test]
fn sell_no_convenience_function_works() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // First buy 100 NO shares
    let amount = 100 * STROOPS_PER_USDC;
    client.buy_no(&user, &market_id, &amount, &6_000i128);

    // Then sell 50 NO shares using convenience function
    let sell_amount = 50 * STROOPS_PER_USDC;
    let position = client.sell_no(&user, &market_id, &sell_amount, &6_000i128);

    assert_eq!(position.yes_shares, 0);
    assert_eq!(position.no_shares, 50 * STROOPS_PER_USDC);
    assert_eq!(position.locked_collateral, 20 * STROOPS_PER_USDC);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn buy_yes_rejects_zero_amount() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (_env, _contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&_env, &_contract_id);

    // Attempt to buy 0 YES shares should fail
    client.buy_yes(&user, &market_id, &0i128, &6_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn buy_no_rejects_negative_amount() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (_env, _contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&_env, &_contract_id);

    // Attempt to buy negative NO shares should fail
    client.buy_no(&user, &market_id, &(-10 * STROOPS_PER_USDC), &6_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn sell_yes_fails_when_user_has_no_shares() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (_env, _contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&_env, &_contract_id);

    // Attempt to sell YES shares without owning any
    client.sell_yes(&user, &market_id, &(50 * STROOPS_PER_USDC), &6_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn sell_no_fails_when_user_has_no_shares() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (_env, _contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&_env, &_contract_id);

    // Attempt to sell NO shares without owning any
    client.sell_no(&user, &market_id, &(50 * STROOPS_PER_USDC), &6_000i128);
}

#[test]
fn get_position_returns_correct_data() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buy some shares
    let amount = 100 * STROOPS_PER_USDC;
    client.buy_yes(&user, &market_id, &amount, &6_000i128);

    // Query position using get_position
    let position = client.get_position(&market_id, &user)
        .expect("storage check ok")
        .expect("position should exist");

    assert_eq!(position.yes_shares, amount);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);
    assert_eq!(position.total_deposited, deposit);
    assert_eq!(position.is_settled, false);
}

#[test]
fn get_position_returns_none_for_nonexistent_position() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, _user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Query position for a user who never traded
    let other_user = Address::generate(&env);
    let position = client.get_position(&market_id, &other_user)
        .expect("storage check ok");

    assert!(position.is_none());
}

#[test]
fn get_market_returns_correct_data() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, _user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Query market details
    let market = client.get_market(&market_id)
        .expect("storage check ok")
        .expect("market should exist");

    assert_eq!(market.id, market_id);
    assert_eq!(market.status, vatix_market_contract::types::MarketStatus::Active);
    assert_eq!(market.price_bps, 5_000); // Default initial price
    assert!(market.result.is_none()); // Not resolved yet
}

#[test]
fn comprehensive_trading_flow_with_convenience_functions() {
    let deposit = 200 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = market_with_funded_user(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // 1. Buy 100 YES at 60%
    client.buy_yes(&user, &market_id, &(100 * STROOPS_PER_USDC), &6_000i128);
    let pos1 = client.get_position(&market_id, &user).unwrap().unwrap();
    assert_eq!(pos1.yes_shares, 100 * STROOPS_PER_USDC);
    assert_eq!(pos1.locked_collateral, 60 * STROOPS_PER_USDC);

    // 2. Buy 50 NO at 70% YES (30% NO cost)
    client.buy_no(&user, &market_id, &(50 * STROOPS_PER_USDC), &7_000i128);
    let pos2 = client.get_position(&market_id, &user).unwrap().unwrap();
    assert_eq!(pos2.yes_shares, 100 * STROOPS_PER_USDC);
    assert_eq!(pos2.no_shares, 50 * STROOPS_PER_USDC);
    // Net: 50 YES at 70% = 35 USDC locked
    assert_eq!(pos2.locked_collateral, 35 * STROOPS_PER_USDC);

    // 3. Sell 25 YES at 65%
    client.sell_yes(&user, &market_id, &(25 * STROOPS_PER_USDC), &6_500i128);
    let pos3 = client.get_position(&market_id, &user).unwrap().unwrap();
    assert_eq!(pos3.yes_shares, 75 * STROOPS_PER_USDC);
    assert_eq!(pos3.no_shares, 50 * STROOPS_PER_USDC);
    // Net: 25 YES at 65% = 16.25 USDC locked
    assert_eq!(pos3.locked_collateral, 1625 * STROOPS_PER_USDC / 100);

    // 4. Sell all NO shares
    client.sell_no(&user, &market_id, &(50 * STROOPS_PER_USDC), &6_500i128);
    let pos4 = client.get_position(&market_id, &user).unwrap().unwrap();
    assert_eq!(pos4.yes_shares, 75 * STROOPS_PER_USDC);
    assert_eq!(pos4.no_shares, 0);
    // Net: 75 YES at 65% = 48.75 USDC locked
    assert_eq!(pos4.locked_collateral, 4875 * STROOPS_PER_USDC / 100);
}
