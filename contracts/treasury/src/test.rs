#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::{TreasuryContract, TreasuryContractClient, TreasuryError};

// ── helpers ───────────────────────────────────────────────────────────────────

struct Setup {
    env: Env,
    admin: Address,
    market: Address,
    token: Address,
    treasury_id: Address,
    client: TreasuryContractClient<'static>,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let treasury_id = env.register(TreasuryContract, ());
    // SAFETY: client holds a borrow into env; env is owned by Setup and outlives
    // the client for the duration of each test.
    let client: TreasuryContractClient<'static> =
        unsafe { core::mem::transmute(TreasuryContractClient::new(&env, &treasury_id)) };

    client.initialize(&admin, &market);

    Setup { env, admin, market, token, treasury_id, client }
}

/// Fund the treasury with tokens directly (simulates prior fee transfer from market).
fn fund_treasury(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn initialize_stores_admin_and_market() {
    let s = setup();
    assert_eq!(s.client.admin(), s.admin);
    assert_eq!(s.client.market_contract(), s.market);
    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 0);
}

#[test]
fn initialize_can_only_be_called_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    let err = s
        .client
        .try_initialize(&other, &s.market)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::AlreadyInitialized);
}

#[test]
fn admin_panics_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    // Non-initialized treasury: admin() panics internally; try_admin errors.
    assert!(client.try_admin().is_err());
}

// ── collect_fee ───────────────────────────────────────────────────────────────

#[test]
fn collect_fee_updates_balance_and_cumulative() {
    let s = setup();
    s.client.collect_fee(&s.market, &s.token, &1u32, &50_000i128);
    assert_eq!(s.client.token_balance(&s.token), 50_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 50_000);
}

#[test]
fn collect_fee_accumulates_across_calls() {
    let s = setup();
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);
    s.client.collect_fee(&s.market, &s.token, &2u32, &200_000i128);
    assert_eq!(s.client.token_balance(&s.token), 300_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300_000);
}

#[test]
fn collect_fee_accumulates_across_tokens() {
    let s = setup();
    let token2 = {
        let a = Address::generate(&s.env);
        s.env.register_stellar_asset_contract_v2(a).address()
    };
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);
    s.client.collect_fee(&s.market, &token2, &1u32, &200_000i128);
    assert_eq!(s.client.token_balance(&s.token), 100_000);
    assert_eq!(s.client.token_balance(&token2), 200_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 100_000);
    assert_eq!(s.client.get_cumulative_fees(&token2), 200_000);
}

#[test]
fn collect_fee_rejects_unauthorized_caller() {
    let s = setup();
    let rogue = Address::generate(&s.env);
    let err = s
        .client
        .try_collect_fee(&rogue, &s.token, &1u32, &50_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
}

#[test]
fn collect_fee_rejects_zero_amount() {
    let s = setup();
    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &0i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_rejects_negative_amount() {
    let s = setup();
    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &(-1i128))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_errors_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    let market = Address::generate(&env);
    let token = Address::generate(&env);
    let err = client
        .try_collect_fee(&market, &token, &1u32, &1_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── withdraw_fees ─────────────────────────────────────────────────────────────

#[test]
fn withdraw_fees_transfers_to_recipient() {
    let s = setup();
    // Fund treasury and record accounting.
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &200_000i128);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&recipient), 200_000);
    assert_eq!(s.client.token_balance(&s.token), 300_000);
    // Cumulative is unchanged by withdrawal.
    assert_eq!(s.client.get_cumulative_fees(&s.token), 500_000);
}

#[test]
fn withdraw_fees_rejects_non_admin() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let imposter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&imposter, &s.token, &recipient, &100_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn withdraw_fees_rejects_zero_amount() {
    let s = setup();
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &s.admin, &0i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn withdraw_fees_rejects_insufficient_balance() {
    let s = setup();
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &s.admin, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InsufficientBalance);
}

#[test]
fn withdraw_fees_errors_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let err = client
        .try_withdraw_fees(&admin, &token, &admin, &1_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── cumulative stays monotone ─────────────────────────────────────────────────

#[test]
fn cumulative_stays_high_after_withdrawal() {
    let s = setup();
    fund_treasury(&s, 300_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &300_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &300_000i128);

    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300_000);
}

// ── set_market_contract ───────────────────────────────────────────────────────

#[test]
fn set_market_contract_updates_address() {
    let s = setup();
    let new_market = Address::generate(&s.env);
    s.client.set_market_contract(&s.admin, &new_market);
    assert_eq!(s.client.market_contract(), new_market);
}

#[test]
fn set_market_contract_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let new_market = Address::generate(&s.env);
    let err = s
        .client
        .try_set_market_contract(&rando, &new_market)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn old_market_cannot_collect_fee_after_rotation() {
    let s = setup();
    let new_market = Address::generate(&s.env);
    s.client.set_market_contract(&s.admin, &new_market);

    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &100i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);

    s.client.collect_fee(&new_market, &s.token, &1u32, &100i128);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 100);
}
