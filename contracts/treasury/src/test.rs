#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::{TreasuryContract, TreasuryContractClient, TreasuryError};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, TreasuryContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let treasury_id = env.register(TreasuryContract, ());
    // SAFETY: client holds a reference into env; env is returned and outlives
    // all subsequent borrows in the same test frame.
    let client: TreasuryContractClient<'static> =
        unsafe { core::mem::transmute(TreasuryContractClient::new(&env, &treasury_id)) };

    (env, treasury_id, client, admin, market, token_id)
}

fn mint(env: &Env, token_id: &Address, recipient: &Address, amount: i128) {
    StellarAssetClient::new(env, token_id).mint(recipient, &amount);
}

// ── initialize ───────────────────────────────────────────────────────────────

#[test]
fn initialize_stores_admin_and_market() {
    let (_, _, client, admin, market, _) = setup();
    client.initialize(&admin, &market).unwrap();
    assert_eq!(client.get_admin().unwrap(), admin);
    assert_eq!(client.get_authorized_market().unwrap(), market);
}

#[test]
fn initialize_can_only_be_called_once() {
    let (env, _, client, admin, market, _) = setup();
    client.initialize(&admin, &market).unwrap();
    let other = Address::generate(&env);
    let err = client.try_initialize(&other, &market).unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::AlreadyInitialized);
}

#[test]
fn get_admin_errors_before_initialize() {
    let (_, _, client, _, _, _) = setup();
    let err = client.try_get_admin().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── collect_fee ──────────────────────────────────────────────────────────────

#[test]
fn collect_fee_transfers_tokens_and_updates_cumulative() {
    let (env, treasury_id, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    mint(&env, &token_id, &market, 1_000_000);

    let cumulative = client.collect_fee(&market, &token_id, &50_000).unwrap();
    assert_eq!(cumulative, 50_000);

    assert_eq!(client.get_balance(&token_id), 50_000);
    assert_eq!(TokenClient::new(&env, &token_id).balance(&market), 950_000);
}

#[test]
fn collect_fee_accumulates_across_calls() {
    let (env, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    mint(&env, &token_id, &market, 3_000_000);

    client.collect_fee(&market, &token_id, &100_000).unwrap();
    let cumulative = client.collect_fee(&market, &token_id, &200_000).unwrap();

    assert_eq!(cumulative, 300_000);
    assert_eq!(client.get_cumulative_fees(&token_id), 300_000);
    assert_eq!(client.get_balance(&token_id), 300_000);
}

#[test]
fn collect_fee_rejects_unauthorized_caller() {
    let (env, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    let rogue = Address::generate(&env);
    mint(&env, &token_id, &rogue, 1_000_000);
    let err = client
        .try_collect_fee(&rogue, &token_id, &50_000)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn collect_fee_rejects_zero_amount() {
    let (_, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    let err = client
        .try_collect_fee(&market, &token_id, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_rejects_negative_amount() {
    let (_, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    let err = client
        .try_collect_fee(&market, &token_id, &-1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_errors_when_not_initialized() {
    let (_, _, client, _, market, token_id) = setup();
    let err = client
        .try_collect_fee(&market, &token_id, &1_000)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── withdraw_fees ─────────────────────────────────────────────────────────────

#[test]
fn withdraw_fees_transfers_to_recipient() {
    let (env, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    mint(&env, &token_id, &market, 1_000_000);
    client.collect_fee(&market, &token_id, &500_000).unwrap();

    let recipient = Address::generate(&env);
    client
        .withdraw_fees(&admin, &token_id, &recipient, &200_000)
        .unwrap();

    assert_eq!(TokenClient::new(&env, &token_id).balance(&recipient), 200_000);
    assert_eq!(client.get_balance(&token_id), 300_000);
}

#[test]
fn withdraw_fees_rejects_non_admin() {
    let (env, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    mint(&env, &token_id, &market, 500_000);
    client.collect_fee(&market, &token_id, &500_000).unwrap();

    let imposter = Address::generate(&env);
    let recipient = Address::generate(&env);
    let err = client
        .try_withdraw_fees(&imposter, &token_id, &recipient, &100_000)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotAdmin);
}

#[test]
fn withdraw_fees_rejects_zero_amount() {
    let (_, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    let err = client
        .try_withdraw_fees(&admin, &token_id, &admin, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn withdraw_fees_errors_when_not_initialized() {
    let (_, _, client, admin, _, token_id) = setup();
    let err = client
        .try_withdraw_fees(&admin, &token_id, &admin, &1_000)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── get_balance / get_cumulative_fees ─────────────────────────────────────────

#[test]
fn get_balance_returns_zero_before_any_fee() {
    let (_, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    assert_eq!(client.get_balance(&token_id), 0);
}

#[test]
fn cumulative_stays_high_after_withdrawal() {
    let (env, _, client, admin, market, token_id) = setup();
    client.initialize(&admin, &market).unwrap();
    mint(&env, &token_id, &market, 300_000);
    client.collect_fee(&market, &token_id, &300_000).unwrap();

    let recipient = Address::generate(&env);
    client
        .withdraw_fees(&admin, &token_id, &recipient, &300_000)
        .unwrap();

    // live balance drained; cumulative counter unchanged
    assert_eq!(client.get_balance(&token_id), 0);
    assert_eq!(client.get_cumulative_fees(&token_id), 300_000);
}
