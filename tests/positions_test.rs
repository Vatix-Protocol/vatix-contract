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
        &None,
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
    assert_event_emitted(&env, "trade_executed");

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
    assert_event_emitted(&env, "trade_executed");
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

    assert_event_emitted(&env, "trade_executed");
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

    /

/* … truncated 3362 chars — edit only what you need near the top … */
