//! Integration tests for the "close market to deposits" feature

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, MarketParams};

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn init_contract() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    (env, admin, contract_id)
}

fn setup_market_with_collateral(
) -> (Env, Address, Address, Address, u32, Address) {
    let (env, admin, contract_id) = init_contract();
    let client = MarketContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let collateral_token = token.address();

    let user = Address::generate(&env);
    let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
    stellar_asset_client.mint(&user, &(1_000 * STROOPS_PER_USDC));

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    // User deposits initial collateral
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS_PER_USDC));

    (env, admin, contract_id, user, market_id, collateral_token)
}

#[test]
fn close_market_to_deposits_succeeds() {
    let (env, admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Verify market is initially open to deposits
    let market_before = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market_before.closed_to_deposits, false);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // Verify market is now closed to deposits
    let market_after = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market_after.closed_to_deposits, true);

    // Verify the event was emitted
    assert_event_emitted(&env, "market_closed_to_deposits_event");
}

#[test]
fn deposit_fails_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, collateral_token) =
        setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // Mint more collateral for the user
    let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
    stellar_asset_client.mint(&user, &(500 * STROOPS_PER_USDC));

    // Attempt to deposit - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.deposit_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
    }));

    assert!(
        result.is_err(),
        "Deposit should fail when market is closed to deposits"
    );
}

#[test]
fn withdrawal_succeeds_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // User should still be able to withdraw their collateral
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_unused_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
    }));

    assert!(
        result.is_ok(),
        "Withdrawal should still work when market is closed to deposits"
    );
}

#[test]
fn close_market_to_deposits_idempotent() {
    let (env, admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market twice
    client.close_market_to_deposits(&admin, &market_id);
    client.close_market_to_deposits(&admin, &market_id);

    // Market should still be closed
    let market = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market.closed_to_deposits, true);
}

#[test]
fn unauthorized_close_market_to_deposits_fails() {
    let (env, _admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Non-admin tries to close market
    let attacker = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.close_market_to_deposits(&attacker, &market_id);
    }));

    assert!(
        result.is_err(),
        "Non-admin should not be able to close market to deposits"
    );
}

#[test]
fn close_nonexistent_market_to_deposits_fails() {
    let (env, admin, contract_id, _user, _market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Try to close a market that doesn't exist
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.close_market_to_deposits(&admin, &999u32);
    }));

    assert!(
        result.is_err(),
        "Closing a non-existent market should fail"
    );
}
