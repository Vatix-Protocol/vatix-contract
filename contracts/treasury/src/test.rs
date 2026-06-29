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
    let client: TreasuryContractClient<'static> =
        unsafe { core::mem::transmute(TreasuryContractClient::new(&env, &treasury_id)) };

    client.initialize(&admin, &market);

    Setup { env, admin, market, token, treasury_id, client }
}

fn fund_treasury(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn initialize_stores_admin_and_market() {
    let s = setup();
    assert_eq!(s.client.admin(), s.admin);
    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 0);
    assert!(s.client.is_authorized_market(&s.market));
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
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &200_000i128);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&recipient), 200_000);
    assert_eq!(s.client.token_balance(&s.token), 300_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 500_000);
}

#[test]
fn withdraw_fees_rejects_non_admin() {
    let s = setup();
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
fn cumulative_stays_high_after_withdrawal() {
    let s = setup();
    fund_treasury(&s, 300_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &300_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &300_000i128);

    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300_000);
}

// ── #383: multi-market registry ───────────────────────────────────────────────

#[test]
fn add_market_registers_second_market() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);

    assert!(s.client.is_authorized_market(&s.market));
    assert!(s.client.is_authorized_market(&market2));
    assert_eq!(s.client.list_markets().len(), 2);
}

#[test]
fn add_market_is_idempotent() {
    let s = setup();
    s.client.add_market(&s.admin, &s.market);
    assert_eq!(s.client.list_markets().len(), 1);
}

#[test]
fn add_market_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let market2 = Address::generate(&s.env);
    let err = s
        .client
        .try_add_market(&rando, &market2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn remove_market_deregisters_market() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);
    s.client.remove_market(&s.admin, &market2);

    assert!(!s.client.is_authorized_market(&market2));
    assert_eq!(s.client.list_markets().len(), 1);
}

#[test]
fn remove_market_errors_if_not_registered() {
    let s = setup();
    let unknown = Address::generate(&s.env);
    let err = s
        .client
        .try_remove_market(&s.admin, &unknown)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
}

#[test]
fn removed_market_cannot_collect_fee() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);
    s.client.remove_market(&s.admin, &market2);

    let err = s
        .client
        .try_collect_fee(&market2, &s.token, &1u32, &100i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
}

#[test]
fn multiple_markets_can_each_collect_fees() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);

    s.client.collect_fee(&s.market, &s.token, &1u32, &100i128);
    s.client.collect_fee(&market2, &s.token, &2u32, &200i128);

    assert_eq!(s.client.token_balance(&s.token), 300);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300);
}
