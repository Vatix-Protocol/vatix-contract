//! End-to-end workspace integration test covering the full market lifecycle.

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, MarketParams};

use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, BytesN, Env, String,
};
use vatix_market_contract::{settlement, storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn oracle_keypair_and_signature(
    env: &Env,
    market_id: u32,
    outcome: bool,
) -> (BytesN<32>, BytesN<64>) {
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    let message = vatix_market_contract::oracle::construct_oracle_message(env, market_id, outcome);
    let signature = signing_key.sign(message.to_array().as_slice());

    (
        BytesN::from_array(env, &verifying_key.to_bytes()),
        BytesN::from_array(env, &signature.to_bytes()),
    )
}

#[test]
fn full_lifecycle_init_create_deposit_resolve_settle() {
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

    let outcome = true;
    let (oracle_pubkey, signature) = oracle_keypair_and_signature(&env, 1, outcome);

    let mut params = MarketParams::default_valid(&env);
    params.oracle_pubkey = oracle_pubkey;
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

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);
    assert_event_emitted(&env, "collateral_deposited_event");

    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);
    assert_event_emitted(&env, "trade_executed_event");

    // --- resolve the market (YES wins) ---
    let resolver = Address::generate(&env);
    let market_id_str = String::from_str(&env, "1");
    client.resolve_market(&resolver, &market_id_str, &outcome, &signature);
    assert_event_emitted(&env, "market_resolved_event");

    let payout = env.as_contract(&contract_id, || {
        let market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market should exist");
        let mut position = storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist");
        let payout = settlement::execute_settlement(&env, &mut position, &market)
            .expect("settlement should succeed");
        storage::set_position(&env, market_id, &user, &position).unwrap();
        payout
    });

    assert_eq!(payout, yes_shares);
    assert_event_emitted(&env, "position_settled_event");

    let settled = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert!(settled.is_settled);
}

#[test]
#[should_panic(expected = "MarketNotResolved")]
fn settlement_before_resolution_is_rejected() {
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
    let deposit = 50 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    env.as_contract(&contract_id, || {
        let market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market should exist");
        let mut position = storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist");
        settlement::execute_settlement(&env, &mut position, &market).unwrap();
    });
}
