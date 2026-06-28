//! Integration tests for Issue #315: Scaffold treasury integration with mock SAC.
//!
//! Uses `env.register_stellar_asset_contract_v2` as the mock SAC to simulate
//! the full fee-collection flow: market withdrawal → token transfer → treasury
//! accounting via `collect_fee`.

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
const FEE_BPS: i128 = 100; // 1%

fn fee_for(amount: i128) -> i128 {
    amount * FEE_BPS / 10_000
}

fn deploy_market_with_fee(env: &Env, admin: &Address) -> Address {
    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(env);
        storage::set_admin(env, admin);
        storage::set_fee_rate_bps(env, FEE_BPS);
    });
    market_addr
}

/// Issue #315: treasury receives fees denominated in the mock SAC token.
#[test]
fn mock_sac_fee_flows_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &market_addr);

    let market = MarketContractClient::new(&env, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    // Mock SAC: use Stellar Asset Contract as the collateral token.
    let token_admin = Address::generate(&env);
    let mock_sac = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = mock_sac.clone();
    let market_id = market.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let user = Address::generate(&env);
    let deposit = 500 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &mock_sac).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 200 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    let expected_fee = fee_for(withdraw_amount);

    assert_eq!(
        TokenClient::new(&env, &mock_sac).balance(&user),
        withdraw_amount,
        "user gets exactly the withdrawal amount"
    );
    assert_eq!(
        treasury.token_balance(&mock_sac),
        expected_fee,
        "treasury accounting reflects mock SAC fee"
    );
    assert_eq!(
        treasury.total_collected(),
        expected_fee,
        "total_collected tracks cumulative fees"
    );
}

/// Issue #315: multiple withdrawals accumulate SAC fees in treasury.
#[test]
fn mock_sac_multiple_withdrawals_accumulate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &market_addr);

    let market = MarketContractClient::new(&env, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let token_admin = Address::generate(&env);
    let mock_sac = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = mock_sac.clone();
    let market_id = market.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let user = Address::generate(&env);
    let deposit = 1000 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &mock_sac).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let w1 = 100 * STROOPS_PER_USDC;
    let w2 = 300 * STROOPS_PER_USDC;
    let w3 = 200 * STROOPS_PER_USDC;

    market.withdraw_unused_collateral(&user, &market_id, &w1);
    market.withdraw_unused_collateral(&user, &market_id, &w2);
    market.withdraw_unused_collateral(&user, &market_id, &w3);

    let total_fee = fee_for(w1) + fee_for(w2) + fee_for(w3);
    assert_eq!(treasury.token_balance(&mock_sac), total_fee);
    assert_eq!(treasury.total_collected(), total_fee);
}

/// Issue #315: admin can withdraw accumulated SAC fees from treasury.
#[test]
fn admin_withdraws_mock_sac_fees() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = deploy_market_with_fee(&env, &admin);
    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &market_addr);

    let market = MarketContractClient::new(&env, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let token_admin = Address::generate(&env);
    let mock_sac = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = mock_sac.clone();
    let market_id = market.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let user = Address::generate(&env);
    let deposit = 200 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &mock_sac).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 100 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);
    let collected = fee_for(withdraw_amount);

    let fee_recipient = Address::generate(&env);
    treasury.withdraw_fees(&admin, &mock_sac, &fee_recipient, &collected);

    assert_eq!(treasury.token_balance(&mock_sac), 0);
    assert_eq!(
        TokenClient::new(&env, &mock_sac).balance(&fee_recipient),
        collected
    );
    assert_eq!(treasury.total_collected(), collected);
}

/// Issue #315: no-fee path — user gets full amount when fee_rate is zero.
#[test]
fn zero_fee_rate_no_sac_fee_deducted() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
        // fee_rate_bps defaults to 0
    });

    let market = MarketContractClient::new(&env, &market_addr);

    let token_admin = Address::generate(&env);
    let mock_sac = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = mock_sac.clone();
    let market_id = market.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &mock_sac).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);

    let withdraw_amount = 80 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

    assert_eq!(
        TokenClient::new(&env, &mock_sac).balance(&user),
        withdraw_amount,
        "no fee deducted when fee_rate_bps is 0"
    );
}
