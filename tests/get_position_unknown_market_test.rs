//! Coverage for `get_position` / `get_net_position` on an unknown `market_id`.
//!
//! Previously both views only ever consulted `storage::get_position`, which
//! is keyed by `(market_id, user)` and returns `Ok(None)` regardless of
//! whether the market itself exists. That made "market doesn't exist" and
//! "market exists but this user never traded" indistinguishable to a caller
//! — both silently returned `Ok(None)`. Neither view actually panicked, but
//! silently returning `None` for a typo'd or never-created `market_id` is
//! its own kind of unclear failure, which is what this fix and its tests
//! address: both views now check market existence first and return
//! `ContractError::MarketNotFound` for a missing market, while continuing to
//! return `Ok(None)` / `Ok(0)` for a real market with no position.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{error::ContractError, storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;
const UNKNOWN_MARKET_ID: u32 = 999_999;

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let user = Address::generate(&env);
    (env, contract_id, user)
}

#[test]
fn get_position_on_unknown_market_returns_market_not_found() {
    let (env, contract_id, user) = setup();
    let client = MarketContractClient::new(&env, &contract_id);

    let result = client.try_get_position(&UNKNOWN_MARKET_ID, &user);
    assert_eq!(result, Err(Ok(ContractError::MarketNotFound)));
}

#[test]
fn get_net_position_on_unknown_market_returns_market_not_found() {
    let (env, contract_id, user) = setup();
    let client = MarketContractClient::new(&env, &contract_id);

    let result = client.try_get_net_position(&UNKNOWN_MARKET_ID, &user);
    assert_eq!(result, Err(Ok(ContractError::MarketNotFound)));
}

/// A real market with no position for this user must still return
/// `Ok(None)` / `Ok(0)` — only a genuinely missing market errors.
#[test]
fn get_position_on_real_market_with_no_position_returns_none() {
    let (env, contract_id, admin) = setup();
    let client = MarketContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token;
    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let never_traded_user = Address::generate(&env);
    let position = client.get_position(&market_id, &never_traded_user);
    assert_eq!(position, None);

    let net = client.get_net_position(&market_id, &never_traded_user);
    assert_eq!(net, 0);
}

/// A real market with an actual position still returns it correctly —
/// the market-existence check must not interfere with the happy path.
#[test]
fn get_position_on_real_market_with_position_returns_it() {
    let (env, contract_id, admin) = setup();
    let client = MarketContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);
    client.update_position(&user, &market_id, &(40 * STROOPS_PER_USDC), &0i128, &6_000i128);

    let position = client
        .get_position(&market_id, &user)
        .expect("position should exist after a trade");
    assert_eq!(position.yes_shares, 40 * STROOPS_PER_USDC);

    let net = client.get_net_position(&market_id, &user);
    assert_eq!(net, 40 * STROOPS_PER_USDC);
}
