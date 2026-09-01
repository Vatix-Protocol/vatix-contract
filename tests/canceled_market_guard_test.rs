//! Coverage for the "canceled markets must reject trades" guard.
//!
//! `MarketContract::update_position`, `deposit_collateral`, and
//! `withdraw_unused_collateral` all gate on `market.status != Active`, which
//! means a `Canceled` market is rejected through the exact same
//! `ContractError::MarketNotActive` path as a `Resolved` one. That existing
//! guard was previously only exercised against `Resolved` markets in the
//! test suite (see `test_withdraw_validates_market_not_active_resolved` in
//! `contracts/market/src/withdraw.rs`) — this file adds the missing
//! `Canceled` coverage explicitly, plus a before/after regression check that
//! trades succeed right up until the market is canceled and fail
//! immediately after.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{error::ContractError, storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn setup_canceled_market() -> (Env, Address, u32, Address, Address) {
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
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    client.cancel_market(&admin, &market_id);

    (env, contract_id, market_id, user, collateral_token)
}

/// Buying shares (positive delta) on a canceled market must be rejected.
#[test]
fn update_position_buy_rejected_on_canceled_market() {
    let (_env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&_env, &contract_id);

    let result = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Selling shares (negative delta) on a canceled market must also be
/// rejected — the market-status guard runs before the share-balance check.
#[test]
fn update_position_sell_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    let result = client.try_update_position(
        &user,
        &market_id,
        &(-1i128),
        &0i128,
        &6_000i128,
    );
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Depositing new collateral into a canceled market must be rejected.
#[test]
fn deposit_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    StellarAssetClient::new(&env, &token).mint(&user, &(10 * STROOPS_PER_USDC));
    let result = client.try_deposit_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Withdrawing from a canceled market must be rejected — cancellation does
/// not open an alternate withdrawal path in this contract version; funds
/// remain accounted for in the position until the market's lifecycle
/// dictates otherwise.
#[test]
fn withdraw_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    let result =
        client.try_withdraw_unused_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert_eq!(result, Err(Ok(ContractError::MarketNotActive)));
}

/// Regression: the exact same trade that succeeds while the market is
/// Active must fail immediately once the market transitions to Canceled,
/// proving the guard reacts to the live status rather than a cached value.
#[test]
fn same_trade_succeeds_before_cancel_and_fails_after() {
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
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Trade succeeds while Active.
    let before = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert!(before.is_ok(), "trade should succeed on an Active market");

    client.cancel_market(&admin, &market_id);

    // The identical trade shape now fails.
    let after = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert_eq!(after, Err(Ok(ContractError::MarketNotActive)));
}

// ── #588: reopen_market guard — Canceled → Active requires explicit admin call ──

/// Attempting to call `reopen_market` on an Active market must be rejected.
/// Active markets are not in a state that needs reopening; surfacing this as
/// an error prevents accidental double-calls.
#[test]
fn reopen_active_market_is_rejected() {
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

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.address();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    // Market is Active — reopen must be rejected.
    let result = client.try_reopen_market(&admin, &market_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketNotActive)),
        "reopen_market on an Active market must return MarketNotActive"
    );
}

/// `reopen_market` on a non-existent market must return `MarketNotFound`.
#[test]
fn reopen_nonexistent_market_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let result = client.try_reopen_market(&admin, &999u32);
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketNotFound)),
        "reopen_market on a nonexistent market must return MarketNotFound"
    );
}

/// A non-admin cannot call `reopen_market`.
#[test]
fn reopen_market_rejected_for_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);

    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.address();

    let market_id = client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
        &None,
    );

    client.cancel_market(&admin, &market_id);

    let result = client.try_reopen_market(&non_admin, &market_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::NotAdmin)),
        "reopen_market by a non-admin must return NotAdmin"
    );
}

/// Full lifecycle: Active → Canceled → reopen (Active) → trading resumes.
///
/// This is the only sanctioned path for `Canceled → Active`. Verifies that
/// trades succeed again after a successful `reopen_market` and that the
/// `market_reopened` event is emitted.
#[test]
fn reopen_canceled_market_restores_active_and_trading() {
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::IntoVal;

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
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Cancel the market.
    client.cancel_market(&admin, &market_id);

    // Trading is blocked while Canceled.
    let blocked = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert_eq!(blocked, Err(Ok(ContractError::MarketNotActive)));

    // Reopen via the explicit admin flow.
    let reopen_result = client.try_reopen_market(&admin, &market_id);
    assert!(reopen_result.is_ok(), "reopen_market must succeed on a Canceled market");

    // Verify the market_reopened event was emitted.
    let events = env.events().all();
    let has_reopen_event = events.iter().any(|(_, topics, _)| {
        let t0: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&env);
        t0 == soroban_sdk::Symbol::new(&env, "market_reopened")
    });
    assert!(has_reopen_event, "market_reopened event must be emitted on successful reopen");

    // Trading is allowed again after reopen.
    let after_reopen = client.try_update_position(
        &user,
        &market_id,
        &(10 * STROOPS_PER_USDC),
        &0i128,
        &6_000i128,
    );
    assert!(after_reopen.is_ok(), "trading must resume on a reopened market");
}

// ── #588 gap: withdraw_canceled_collateral is the one permitted withdrawal path ──

/// `withdraw_unused_collateral` must still be rejected on a Canceled market
/// (task 1 regression, reproduced here to be adjacent to the positive case).
///
/// `withdraw_canceled_collateral` is the **only** sanctioned withdrawal path
/// for a canceled market. The regular `withdraw_unused_collateral` guard
/// checks `status == Active` and must continue to reject `Canceled` markets.
#[test]
fn withdraw_unused_still_rejected_on_canceled_market() {
    let (env, contract_id, market_id, user, _token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);

    let result =
        client.try_withdraw_unused_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketNotActive)),
        "withdraw_unused_collateral must reject on a Canceled market"
    );
}

/// `withdraw_canceled_collateral` must **succeed** on a Canceled market and
/// return the user's full deposited balance.
///
/// This is the positive-path counterpart to the rejection tests above —
/// it proves the special refund path works, is distinct from the trading
/// withdrawal path, and actually delivers the collateral back to the user.
#[test]
fn withdraw_canceled_collateral_succeeds_on_canceled_market() {
    use soroban_sdk::token::Client as TokenClient;

    let (env, contract_id, market_id, user, collateral_token) = setup_canceled_market();
    let client = MarketContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &collateral_token);

    // The user deposited 100 USDC in setup_canceled_market.
    let deposit = 100 * STROOPS_PER_USDC;

    // Before reclaim: user balance is 0 (all deposited), contract holds it.
    assert_eq!(
        token_client.balance(&user),
        0,
        "user should hold nothing before reclaim"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        deposit,
        "contract should hold the deposit before reclaim"
    );

    // Reclaim succeeds and returns the full deposited amount.
    let refund = client.withdraw_canceled_collateral(&user, &market_id);
    assert_eq!(
        refund,
        deposit,
        "refund must equal the full deposited collateral"
    );

    // After reclaim: collateral has moved from contract back to user.
    assert_eq!(
        token_client.balance(&user),
        deposit,
        "user must receive back the full deposit"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        0,
        "contract must hold nothing after reclaim"
    );
}

/// `withdraw_canceled_collateral` must be rejected on an Active market —
/// the refund path is exclusive to Canceled markets.
#[test]
fn withdraw_canceled_collateral_rejected_on_active_market() {
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
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    // Market is still Active — the canceled-collateral refund path must reject.
    let result = client.try_withdraw_canceled_collateral(&user, &market_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketNotActive)),
        "withdraw_canceled_collateral must reject on an Active market"
    );
}

// ── #588 gap: reopen_market on a Resolved market must return MarketAlreadyResolved ──

/// `reopen_market` on a Resolved market must return `MarketAlreadyResolved`.
///
/// Resolved markets are terminal — their outcome is final, collateral has been
/// or may be claimed, and there is no path back to Active. This test confirms
/// the `validate_reopenable` guard correctly distinguishes the Resolved state
/// from the Canceled state and rejects the operation with the right error.
#[test]
fn reopen_resolved_market_returns_market_already_resolved() {
    use vatix_market_contract::types::MarketStatus;

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

    // Force the market into Resolved state by directly mutating storage,
    // sidestepping oracle-signature verification (the oracle path is tested
    // separately; we only need the status to be Resolved here).
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .unwrap()
            .expect("market should exist");
        market.status = MarketStatus::Resolved;
        market.result = Some(true);
        storage::set_market(&env, market_id, &market).unwrap();
    });

    // Attempting to reopen a Resolved market must return MarketAlreadyResolved.
    let result = client.try_reopen_market(&admin, &market_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::MarketAlreadyResolved)),
        "reopen_market on a Resolved market must return MarketAlreadyResolved"
    );
}
