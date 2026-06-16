//! Workspace integration tests for market creation and collateral deposit
//! through the public `MarketContract` client.

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, MarketParams};

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

/// Register the contract and configure its admin, returning the env, the admin
/// address, and the contract id.
fn init_contract() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_admin(&env, &admin);
    });

    (env, admin, contract_id)
}

#[test]
fn create_market_then_deposit_collateral() {
    let (env, admin, contract_id) = init_contract();
    let client = MarketContractClient::new(&env, &contract_id);

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
    assert_eq!(market_id, 1);
    assert_event_emitted(&env, "market_created_event");

    // The created market is persisted with the expected parameters.
    let market = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id).expect("market should exist")
    });
    assert_eq!(market.collateral_token, collateral_token);

    // Deposit collateral and confirm the position total is tracked.
    let user = Address::generate(&env);
    let deposit = 25 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);
    assert_event_emitted(&env, "collateral_deposited_event");

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).expect("position should exist")
    });
    assert_eq!(position.total_deposited, deposit);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn non_admin_cannot_create_market() {
    let (env, _admin, contract_id) = init_contract();
    let client = MarketContractClient::new(&env, &contract_id);

    let non_admin = Address::generate(&env);
    let params = MarketParams::default_valid(&env);

    // A caller that is not the stored admin must be rejected with NotAdmin (#41).
    client.initialize_market(
        &non_admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );
}
