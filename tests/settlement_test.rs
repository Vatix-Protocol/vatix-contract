//! End-to-end workspace integration tests covering market settlement paths.
//!
//! This file exercises:
//! - The full happy-path lifecycle: init → create → deposit → resolve → settle
//! - Settlement before resolution is rejected (`MarketNotResolved`)
//! - No-winner settlement: `result = None` on a Resolved market refunds full
//!   deposited collateral rather than paying out a winning share balance
//! - Settlement idempotency: a second `settle_position` call on an already-
//!   settled position returns `PositionAlreadySettled` and does not transfer
//!   any additional funds (double-pay guard)
//!
//! All tests that need a resolved market bypass `resolve_market` (whose full
//! signature requires a resolver address, a valid oracle signature, and an
//! `expires_at` timestamp) by directly mutating the market record in storage.
//! This keeps the tests focused on the settlement invariants they are testing
//! without duplicating the oracle-signature machinery already covered in
//! `tests/market_test.rs`.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};
use vatix_market_contract::{
    error::ContractError, storage,
    types::MarketStatus,
    MarketContract, MarketContractClient,
};

const STROOPS_PER_USDC: i128 = 10_000_000;

// ─────────────────────────────────────────────────────────────────────────────
// No-winner settlement
// ─────────────────────────────────────────────────────────────────────────────

/// When a market is resolved with `result = None` (no winning outcome), every
/// user should receive their full deposited collateral back via `settle_position`.
///
/// This integration test confirms the full token flow end-to-end at the
/// workspace level: the storage mutation that represents a no-winner resolution,
/// the `settle_position` call, and the token balance changes all produce the
/// expected result.
#[test]
fn no_winner_settlement_refunds_full_deposited_collateral() {
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
    let token_client = TokenClient::new(&env, &collateral_token);

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

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
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Take a YES position — the user has locked some collateral and holds shares.
    // In a no-winner scenario this position doesn't matter: total_deposited is
    // what gets refunded, not the shares.
    let yes_shares = 50 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);

    // Directly resolve the market with result = None (no winning outcome).
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market must exist");
        market.status = MarketStatus::Resolved;
        market.result = None; // no winner
        storage::set_market(&env, market_id, &market).unwrap();
    });

    // Before settling: user holds 0 (all deposited), contract holds the deposit.
    assert_eq!(token_client.balance(&user), 0);
    assert_eq!(token_client.balance(&contract_id), deposit);

    // settle_position must pay out the full deposited amount (not just the shares).
    let payout = client.settle_position(&user, &market_id);
    assert_eq!(
        payout, deposit,
        "no-winner settlement must refund the full deposited collateral"
    );

    // After settling: user gets the full deposit back, contract is empty.
    assert_eq!(
        token_client.balance(&user),
        deposit,
        "user must receive the full deposit as a no-winner refund"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        0,
        "contract must hold nothing after a no-winner settlement"
    );

    // The position is marked settled.
    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert!(
        position.is_settled,
        "position.is_settled must be true after settlement"
    );
}

/// A no-winner settlement on a market where the user holds **no shares**
/// (only deposited collateral, no update_position call) still refunds their
/// deposited amount correctly.
#[test]
fn no_winner_settlement_refunds_deposit_only_position() {
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
    let token_client = TokenClient::new(&env, &collateral_token);

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 75 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);
    // Intentionally no update_position call — user has no shares.

    // Resolve with no winner.
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market must exist");
        market.status = MarketStatus::Resolved;
        market.result = None;
        storage::set_market(&env, market_id, &market).unwrap();
    });

    let payout = client.settle_position(&user, &market_id);
    assert_eq!(
        payout, deposit,
        "deposit-only position must receive the full deposit as no-winner refund"
    );
    assert_eq!(token_client.balance(&user), deposit);
    assert_eq!(token_client.balance(&contract_id), 0);
}

/// `settle_position` before resolution is rejected with `MarketNotResolved`.
#[test]
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
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 50 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // The market is still Active — settlement must fail.
    let result = client.try_settle_position(&user, &market_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketNotResolved)),
        "settle_position must return MarketNotResolved when the market is Active"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Settlement idempotency / double-pay guard
// ─────────────────────────────────────────────────────────────────────────────

/// A second `settle_position` call for the same position must be rejected with
/// `PositionAlreadySettled` and must NOT transfer any additional collateral.
///
/// This is the workspace-level integration companion to
/// `test_second_settle_position_cannot_double_pay` in
/// `contracts/market/src/settlement.rs`. It verifies the double-pay guard at
/// the cross-crate boundary with an actual SAC token, confirming that:
///
/// 1. The first `settle_position` succeeds and transfers the payout.
/// 2. Every subsequent call returns `Err(PositionAlreadySettled)`.
/// 3. Token balances (user and contract) are identical after each rejected
///    repeat call — no partial or duplicate payout has leaked through.
/// 4. The stored `Position.is_settled` flag remains `true` and the position
///    is otherwise unchanged after the repeat attempts.
#[test]
fn second_settle_cannot_double_pay() {
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
    let token_client = TokenClient::new(&env, &collateral_token);

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

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
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Buy YES shares so the position has a non-zero share payout.
    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);

    // Resolve YES wins by directly writing resolved state to storage.
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market must exist");
        market.status = MarketStatus::Resolved;
        market.result = Some(true); // YES wins
        storage::set_market(&env, market_id, &market).unwrap();
    });

    // ── First settlement: succeeds and transfers payout ──

    let first_payout = client.settle_position(&user, &market_id);
    assert_eq!(
        first_payout, yes_shares,
        "first settlement must pay out the winning YES shares"
    );

    let user_bal_after_first = token_client.balance(&user);
    let contract_bal_after_first = token_client.balance(&contract_id);

    assert_eq!(
        user_bal_after_first, yes_shares,
        "user must hold the payout after first settlement"
    );
    assert_eq!(
        contract_bal_after_first, 0,
        "contract must hold nothing after the winning payout"
    );

    let position_after_first = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert!(
        position_after_first.is_settled,
        "position.is_settled must be true after first settlement"
    );

    // ── Repeat attempts: must error without moving funds ──

    for attempt in 0..3u32 {
        let repeat = client.try_settle_position(&user, &market_id);
        assert_eq!(
            repeat,
            Err(Ok(ContractError::PositionAlreadySettled)),
            "attempt {}: repeat settle must return PositionAlreadySettled",
            attempt + 2
        );

        assert_eq!(
            token_client.balance(&user),
            user_bal_after_first,
            "attempt {}: user balance must not change on repeat settle",
            attempt + 2
        );
        assert_eq!(
            token_client.balance(&contract_id),
            contract_bal_after_first,
            "attempt {}: contract balance must not change on repeat settle",
            attempt + 2
        );
    }

    // ── Stored position must be identical after the rejected repeat attempts ──

    let position_after_repeats = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert_eq!(
        position_after_repeats, position_after_first,
        "stored position must be unchanged after rejected repeat settlements"
    );
}

/// Settlement idempotency also applies when the market had no winner (`result = None`):
/// the refund path must not pay out a second time.
#[test]
fn second_settle_cannot_double_pay_no_winner_path() {
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
    let token_client = TokenClient::new(&env, &collateral_token);

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = collateral_token.clone();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    let user = Address::generate(&env);
    let deposit = 80 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Resolve with no winner.
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market must exist");
        market.status = MarketStatus::Resolved;
        market.result = None;
        storage::set_market(&env, market_id, &market).unwrap();
    });

    // First settlement refunds the full deposit.
    let first_payout = client.settle_position(&user, &market_id);
    assert_eq!(
        first_payout, deposit,
        "first settlement (no-winner path) must refund the full deposit"
    );

    let user_bal_after = token_client.balance(&user);
    let contract_bal_after = token_client.balance(&contract_id);

    // Second attempt must be rejected without moving any funds.
    let repeat = client.try_settle_position(&user, &market_id);
    assert_eq!(
        repeat,
        Err(Ok(ContractError::PositionAlreadySettled)),
        "second no-winner settle must return PositionAlreadySettled"
    );
    assert_eq!(
        token_client.balance(&user),
        user_bal_after,
        "user balance must not change on repeat no-winner settle"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        contract_bal_after,
        "contract balance must not change on repeat no-winner settle"
    );
}
