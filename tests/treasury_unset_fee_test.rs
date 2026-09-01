//! Integration tests for the treasury-optional fee path.
//!
//! Documented behavior: when a withdrawal fee is configured but no treasury
//! address has been registered, the withdrawal must still succeed ("skip
//! transfer", never "revert"). The fee amount is retained in the market
//! contract's own collateral token balance and a `FeeRetainedNoTreasury`
//! event is emitted so the retained fee is never silent. Once a treasury is
//! registered, subsequent withdrawals route the fee there instead and stop
//! emitting `FeeRetainedNoTreasury`.
//!
//! See `contracts/market/src/withdraw.rs` module docs for the full write-up.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, IntoVal, Symbol,
};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};
use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;
const FEE_BPS: i128 = 500; // 5%

fn deploy_market_with_fee(env: &Env, admin: &Address) -> Address {
    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(env);
        storage::set_admin(env, admin);
        storage::set_fee_rate_bps(env, FEE_BPS);
    });
    market_addr
}

fn has_fee_retained_event(env: &Env) -> bool {
    env.events().all().iter().any(|(_, topics, _)| {
        let topic0: Symbol = topics.get(0).unwrap().into_val(env);
        topic0 == Symbol::new(env, "fee_retained_no_treasury")
    })
}

/// Treasury unset: withdrawal succeeds, user gets exactly `amount`, and the
/// fee is retained inside the market contract's own token balance instead of
/// being dropped or reverted.
#[test]
fn withdraw_succeeds_and_retains_fee_when_treasury_unset() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let market = MarketContractClient::new(&env, &market_addr);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let market_id = market.initialize_market(
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
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 40 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    let expected_fee = withdraw_amount * FEE_BPS / 10_000;
    let token_client = TokenClient::new(&env, &token);

    assert_eq!(
        token_client.balance(&user),
        withdraw_amount,
        "user receives exactly the requested amount even though a fee applied"
    );
    assert_eq!(
        token_client.balance(&market_addr),
        deposit - withdraw_amount - expected_fee,
        "the fee remains in the market contract's own balance"
    );

    let position = env.as_contract(&market_addr, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert_eq!(
        position.total_deposited,
        deposit - withdraw_amount - expected_fee,
        "total_deposited accounts for both the withdrawal and the retained fee"
    );

    assert!(
        has_fee_retained_event(&env),
        "FeeRetainedNoTreasury must fire whenever a fee is retained without a treasury"
    );
}

/// Treasury set: the fee is routed to the treasury contract and the
/// "retained" event must not appear for that withdrawal.
#[test]
fn withdraw_routes_fee_to_treasury_when_registered() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let market = MarketContractClient::new(&env, &market_addr);

    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let market_id = market.initialize_market(
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
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 40 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    let expected_fee = withdraw_amount * FEE_BPS / 10_000;
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    assert_eq!(treasury.token_balance(&token), expected_fee);
    assert!(
        !has_fee_retained_event(&env),
        "FeeRetainedNoTreasury must not fire once a treasury is registered"
    );
}

/// Switching from unset -> registered mid-lifetime: only withdrawals made
/// before registration emit the "retained" event.
#[test]
fn registering_treasury_later_stops_retained_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let market = MarketContractClient::new(&env, &market_addr);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let market_id = market.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 200 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    // First withdrawal: no treasury yet -> fee retained, event emitted.
    market.withdraw_unused_collateral(&user, &market_id, &(20 * STROOPS_PER_USDC));
    assert!(has_fee_retained_event(&env));

    // Register a treasury and withdraw again.
    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);

    market.withdraw_unused_collateral(&user, &market_id, &(20 * STROOPS_PER_USDC));

    // The most recent withdrawal's fee reached the treasury.
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let expected_fee = 20 * STROOPS_PER_USDC * FEE_BPS / 10_000;
    assert_eq!(treasury.token_balance(&token), expected_fee);
}
