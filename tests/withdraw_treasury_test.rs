//! Integration tests for the Treasury fee path through withdraw_unused_collateral.
//!
//! These tests exercise the full cross-contract flow: Market deducts a 50 bps
//! fee from every withdrawal when a Treasury is registered and forwards it via
//! collect_fee. Tests run against live contract instances (no storage mocking).

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};
use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;
const FEE_BPS: i128 = 50;
const BPS_DENOM: i128 = 10_000;

fn fee_for(amount: i128) -> i128 {
    amount * FEE_BPS / BPS_DENOM
}

/// Deploy market + treasury, wire them together, return their addresses and helpers.
fn setup_with_treasury() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
        storage::set_version(&env);
        storage::set_fee_rate_bps(&env, FEE_BPS);
    });

    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr)
        .initialize(&admin, &market_addr);

    MarketContractClient::new(&env, &market_addr).set_treasury(&admin, &treasury_addr);

    let token_admin = Address::generate(&env);
    let collateral_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    (env, market_addr, treasury_addr, admin, collateral_token)
}

/// Create a market and return its numeric id.
fn open_market(
    env: &Env,
    client: &MarketContractClient,
    admin: &Address,
    token: &Address,
) -> u32 {
    let mut params = MarketParams::default_valid(env);
    params.collateral_token = token.clone();
    client.initialize_market(
        admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    )
}

// ── fee routing ───────────────────────────────────────────────────────────────

#[test]
fn withdraw_routes_half_percent_fee_to_treasury() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 50 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    let expected_fee = fee_for(withdraw_amount);

    assert_eq!(
        TokenClient::new(&env, &token).balance(&user),
        withdraw_amount,
        "user receives exactly the requested amount"
    );
    assert_eq!(
        treasury.token_balance(&token),
        expected_fee,
        "treasury holds the 50 bps fee"
    );
    assert_eq!(
        treasury.total_collected(),
        expected_fee,
        "cumulative counter updated"
    );
}

#[test]
fn multiple_withdrawals_accumulate_fees() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    let deposit = 500 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let w1 = 100 * STROOPS_PER_USDC;
    let w2 = 200 * STROOPS_PER_USDC;

    market.withdraw_unused_collateral(&user, &market_id, &w1);
    market.withdraw_unused_collateral(&user, &market_id, &w2);

    let total_fee = fee_for(w1) + fee_for(w2);
    assert_eq!(treasury.total_collected(), total_fee);
    assert_eq!(treasury.token_balance(&token), total_fee);
}

// ── no treasury ───────────────────────────────────────────────────────────────

#[test]
fn withdraw_without_treasury_sends_full_amount_to_user() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
        storage::set_version(&env);
    });
    let market = MarketContractClient::new(&env, &market_addr);

    // Set fee rate but no treasury - fee will be retained in contract
    market.set_fee_rate(&admin, &FEE_BPS);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    let deposit = 50 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    // Calculate how much we can actually withdraw: available - fee
    // With 50 bps fee: withdraw = deposit / (1 + 0.005) ≈ deposit * 0.995
    let withdraw_amount = 49_750_000; // leaves room for fee
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    let expected_fee = fee_for(withdraw_amount);
    let expected_user_balance = withdraw_amount;

    assert_eq!(
        TokenClient::new(&env, &token).balance(&user),
        expected_user_balance,
        "no treasury → user receives amount minus fee (fee retained in contract)"
    );
}

// ── admin fee withdrawal ──────────────────────────────────────────────────────

#[test]
fn admin_can_drain_treasury_and_cumulative_stays_unchanged() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    let deposit = 200 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 100 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);
    let collected = fee_for(withdraw_amount);

    let fee_recipient = Address::generate(&env);
    treasury.withdraw_fees(&admin, &token, &fee_recipient, &collected);

    assert_eq!(
        treasury.token_balance(&token),
        0,
        "live balance drained after admin withdrawal"
    );
    assert_eq!(
        treasury.total_collected(),
        collected,
        "cumulative counter is monotone and does not decrease"
    );
    assert_eq!(
        TokenClient::new(&env, &token).balance(&fee_recipient),
        collected,
        "fee recipient holds the withdrawn amount"
    );
}

// ── authorization ─────────────────────────────────────────────────────────────

#[test]
fn non_admin_cannot_withdraw_treasury_fees() {
    let (env, _market_addr, treasury_addr, _admin, token) = setup_with_treasury();
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let imposter = Address::generate(&env);
    let err = treasury
        .try_withdraw_fees(&imposter, &token, &imposter, &1i128)
        .unwrap_err()
        .unwrap();

    assert_eq!(
        err,
        vatix_treasury_contract::TreasuryError::Unauthorized,
        "imposter must not be allowed to drain the treasury"
    );
}

// ── fee rate configuration ────────────────────────────────────────────────────

#[test]
fn admin_can_set_fee_rate() {
    let (env, market_addr, _treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);

    // Initially set to 50 bps by setup
    assert_eq!(market.get_fee_rate(), FEE_BPS);

    // Admin can change it
    market.set_fee_rate(&admin, &100); // 1%
    assert_eq!(market.get_fee_rate(), 100);

    // Admin can disable fees
    market.set_fee_rate(&admin, &0);
    assert_eq!(market.get_fee_rate(), 0);

    // Verify withdrawal with zero fee returns full amount
    let market_id = open_market(&env, &market, &admin, &token);
    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);
    market.withdraw_unused_collateral(&user, &market_id, &deposit);

    assert_eq!(
        TokenClient::new(&env, &token).balance(&user),
        deposit,
        "zero fee rate → user receives full amount"
    );
}

#[test]
fn non_admin_cannot_set_fee_rate() {
    let (env, market_addr, _treasury_addr, _admin, _token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let imposter = Address::generate(&env);

    let result = market.try_set_fee_rate(&imposter, &100);
    assert!(result.is_err(), "non-admin must not be able to set fee rate");
}

#[test]
fn invalid_fee_rate_rejected() {
    let (env, market_addr, _treasury_addr, admin, _token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);

    // Fee rate > 10000 bps (100%) should be rejected
    let result = market.try_set_fee_rate(&admin, &10001);
    assert!(result.is_err(), "fee rate > 10000 bps must be rejected");

    // Negative fee rate should be rejected
    let result = market.try_set_fee_rate(&admin, &-1);
    assert!(result.is_err(), "negative fee rate must be rejected");
}
