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

/// Full protocol loop at the workspace level, exercised through the public
/// contract client end to end:
///
///   initialize → create market → deposit → trade → resolve → settle → payout
///
/// Asserts that the SAC collateral token actually moves between the user and
/// the contract at each stage, and that every step publishes its event.
#[test]
fn full_protocol_loop_deposit_trade_resolve_settle() {
    use soroban_sdk::{token::Client as TokenClient, String};
    use vatix_market_contract::types::MarketStatus;

    let env = Env::default();
    env.mock_all_auths();

    // Register the contract with a bootstrapped admin + storage version.
    let (admin, contract_id) = helpers::register_contract(&env);
    let client = MarketContractClient::new(&env, &contract_id);

    // Real SAC collateral token so balances are observable on-chain.
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();
    let sac = StellarAssetClient::new(&env, &collateral_token);
    let token_client = TokenClient::new(&env, &collateral_token);

    // Oracle keypair so the market can later be resolved with a valid signature.
    let outcome = true;
    let (oracle_pubkey, signing_key) = helpers::oracle_keypair(&env);

    // 1. Create the market.
    let question = String::from_str(&env, "Will the full loop settle?");
    let end_time = env.ledger().timestamp() + 86_400;
    let market_id =
        client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);
    assert_eq!(market_id, 1);
    assert_event_emitted(&env, "market_created_event");

    // 2. Deposit collateral: tokens move from the user into the contract.
    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    sac.mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);
    assert_event_emitted(&env, "collateral_deposited_event");
    assert_eq!(token_client.balance(&user), 0);
    assert_eq!(token_client.balance(&contract_id), deposit);

    // 3. Trade: buy YES shares at 50% so the locked collateral is fully covered.
    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);
    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert_eq!(position.yes_shares, yes_shares);

    // 4. Resolve the market (YES wins) with a valid oracle signature.
    let signature = helpers::sign_outcome(&env, &signing_key, market_id, outcome);
    let market_id_str = String::from_str(&env, "1");
    client.resolve_market(&market_id_str, &outcome, &signature);
    assert_event_emitted(&env, "market_resolved_event");
    let resolved = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .unwrap()
            .expect("market should exist")
    });
    assert_eq!(resolved.status, MarketStatus::Resolved);
    assert_eq!(resolved.result, Some(outcome));

    // 5. Settle: the payout equals the winning YES shares and is transferred
    //    from the contract back to the user.
    let payout = client.settle_position(&user, &market_id);
    assert_eq!(payout, yes_shares);
    assert_event_emitted(&env, "position_settled_event");
    assert_eq!(token_client.balance(&user), payout);
    assert_eq!(token_client.balance(&contract_id), deposit - payout);

    let settled = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert!(settled.is_settled);

    // Settling a second time is rejected (position already settled).
    assert!(client.try_settle_position(&user, &market_id).is_err());
}

// --- #373: duplicate market_id creation is rejected ---

/// Each call to `initialize_market` must assign a fresh, unique ID.
/// Manually forcing a duplicate by writing directly to storage and then
/// calling `initialize_market` again must be rejected.
#[test]
fn duplicate_market_id_creation_is_rejected() {
    use vatix_market_contract::{
        error::ContractError,
        types::{Market, MarketStatus},
    };
    use soroban_sdk::{BytesN, String};

    let (env, admin, contract_id) = init_contract();
    let client = MarketContractClient::new(&env, &contract_id);

    // Bootstrap storage version (init_contract doesn't set it).
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();

    let params = MarketParams::default_valid(&env);

    // Create market normally → gets ID 1.
    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &collateral_token,
    );
    assert_eq!(market_id, 1);

    // Manually reset the counter back to 0 so the next increment produces ID 1 again,
    // while market 1 is already in storage — this simulates a counter rollback.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&storage::StorageKey::MarketCounter, &0u32);
    });

    // A second call must be rejected because market ID 1 already exists.
    let result = client.try_initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &collateral_token,
    );
    assert_eq!(result, Err(Ok(ContractError::AlreadyInitialized)));
}
