//! Integration tests for the Treasury fee path through withdraw_unused_collateral.
//!
//! These tests exercise the full cross-contract flow: Market deducts a 50 bps
//! fee from every withdrawal when a Treasury is registered and forwards it via
//! collect_fee. Tests run against live contract instances (no storage mocking).

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};
use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;
const FEE_BPS: i128 = 50;
const BPS_DENOM: i128 = 10_000;

fn fee_for(amount: i128) -> i128 {
    fee_at(amount, FEE_BPS)
}

fn fee_at(amount: i128, bps: i128) -> i128 {
    amount * bps / BPS_DENOM
}

/// Move the ledger past the #496 fee-rate timelock.
fn advance_past_fee_timelock(env: &Env) {
    env.ledger()
        .with_mut(|l| l.timestamp += vatix_market_contract::FEE_RATE_TIMELOCK_SECONDS + 1);
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

    MarketContractClient::new(&env, &market_addr).set_treasury_contract(&admin, &treasury_addr);

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
        &None,
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
    env.ledger().with_mut(|l| l.timestamp += 3601);

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
    env.ledger().with_mut(|l| l.timestamp += 3601);

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
    env.ledger().with_mut(|l| l.timestamp += 3601);

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
    env.ledger().with_mut(|l| l.timestamp += 3601);

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

    // Fee changes are timelocked (#496): proposing does not change the live rate.
    market.set_fee_rate(&admin, &100); // 1%
    assert_eq!(market.get_fee_rate(), FEE_BPS);
    advance_past_fee_timelock(&env);
    market.execute_fee_rate_change();
    assert_eq!(market.get_fee_rate(), 100);

    // Admin can disable fees
    market.set_fee_rate(&admin, &0);
    advance_past_fee_timelock(&env);
    market.execute_fee_rate_change();
    assert_eq!(market.get_fee_rate(), 0);

    // Verify withdrawal with zero fee returns full amount
    let market_id = open_market(&env, &market, &admin, &token);
    let user = Address::generate(&env);
    let deposit = 100 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);
    env.ledger().with_mut(|l| l.timestamp += 3601);
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

// ── Issue #387: Withdraw treasury fee integration test pass ──────────────────
//
// These tests validate the complete withdraw-flow from market → treasury →
// admin recipient, covering happy path, authorization guards, partial
// withdrawals, admin rotation, and invalid-state handling.

/// Full end-to-end: deposit → withdraw (fee collected) → admin withdraws from
/// treasury → recipient holds the tokens and cumulative counter is monotone.
#[test]
fn full_withdraw_treasury_fee_flow() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    let deposit = 200 * STROOPS_PER_USDC;
    StellarAssetClient::new(&env, &token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);
    env.ledger().with_mut(|l| l.timestamp += 3601);

    // Withdraw half — fee is deducted and routed to treasury
    let w1 = 80 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &w1);
    let fee1 = fee_for(w1);

    let before_withdraw = treasury.total_collected();
    let fee_recipient = Address::generate(&env);
    treasury.withdraw_fees(&admin, &token, &fee_recipient, &fee1);

    assert_eq!(
        TokenClient::new(&env, &token).balance(&fee_recipient),
        fee1,
        "fee recipient receives the withdrawn amount"
    );
    assert_eq!(
        treasury.token_balance(&token),
        0,
        "treasury balance drained after full withdrawal"
    );
    // Cumulative counter is monotone: it stays at the total ever collected,
    // not at the current balance.
    assert_eq!(
        treasury.total_collected(),
        before_withdraw,
        "total_collected is monotone — does not change after withdrawal"
    );
}

/// Admin may partially withdraw treasury fees, leaving the remainder
/// custodied for a future withdrawal.
#[test]
fn partial_withdraw_treasury_fees() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(200 * STROOPS_PER_USDC));
    market.deposit_collateral(&user, &market_id, &(200 * STROOPS_PER_USDC));
    env.ledger().with_mut(|l| l.timestamp += 3601);
    market.withdraw_unused_collateral(&user, &market_id, &(100 * STROOPS_PER_USDC));

    let total_fee = fee_for(100 * STROOPS_PER_USDC); // 50 bps
    let partial = total_fee / 2;

    let recipient = Address::generate(&env);
    treasury.withdraw_fees(&admin, &token, &recipient, &partial);

    assert_eq!(
        TokenClient::new(&env, &token).balance(&recipient),
        partial,
        "recipient gets partial amount"
    );
    assert_eq!(
        treasury.token_balance(&token),
        total_fee - partial,
        "treasury retains the remainder"
    );

    // Withdraw the rest
    let remainder = total_fee - partial;
    treasury.withdraw_fees(&admin, &token, &recipient, &remainder);
    assert_eq!(
        treasury.token_balance(&token),
        0,
        "treasury fully drained after second withdrawal"
    );
    assert_eq!(
        TokenClient::new(&env, &token).balance(&recipient),
        total_fee,
        "recipient ends up with the full accumulated fee"
    );
}

/// Admin transfer does not break the treasury's ability to withdraw fees.
#[test]
fn admin_transfer_preserves_withdraw_capability() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let new_admin = Address::generate(&env);
    treasury.transfer_admin(&admin, &new_admin);

    // old admin can no longer withdraw
    let recipient = Address::generate(&env);
    let err = treasury
        .try_withdraw_fees(&admin, &token, &recipient, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        vatix_treasury_contract::TreasuryError::Unauthorized,
        "old admin must be rejected after transfer"
    );

    // new admin can withdraw (treasury has no fees, but auth passes)
    // First, generate some fees — use original market admin for market operations
    let market = MarketContractClient::new(&env, &market_addr);
    let market_id = open_market(&env, &market, &admin, &token);
    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS_PER_USDC));
    market.deposit_collateral(&user, &market_id, &(100 * STROOPS_PER_USDC));
    env.ledger().with_mut(|l| l.timestamp += 3601);
    market.withdraw_unused_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
    let expected_fee = fee_for(50 * STROOPS_PER_USDC);

    treasury.withdraw_fees(&new_admin, &token, &recipient, &expected_fee);
    assert_eq!(
        TokenClient::new(&env, &token).balance(&recipient),
        expected_fee,
        "new admin can withdraw fees after transfer"
    );
}

/// Reject withdrawal of amount exceeding the custodied token balance.
#[test]
fn withdraw_exceeding_balance_rejected() {
    let (env, _market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let recipient = Address::generate(&env);
    let err = treasury
        .try_withdraw_fees(&admin, &token, &recipient, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        vatix_treasury_contract::TreasuryError::InsufficientBalance,
        "withdraw from empty treasury must fail"
    );
}

/// Reject withdrawal with zero or negative amount.
#[test]
fn withdraw_invalid_amount_rejected() {
    let (env, _market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);

    let recipient = Address::generate(&env);
    let err = treasury
        .try_withdraw_fees(&admin, &token, &recipient, &0i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        vatix_treasury_contract::TreasuryError::InvalidAmount,
        "zero amount must be rejected"
    );

    let err = treasury
        .try_withdraw_fees(&admin, &token, &recipient, &(-1i128))
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        vatix_treasury_contract::TreasuryError::InvalidAmount,
        "negative amount must be rejected"
    );
}

// ── Issue #895: withdraw fee → treasury integration ──────────────────────────
//
// Covers WIRE_COLLECT_FEE_CALLBACK_SPEC.md end to end: fee-rate changes go
// through the timelock and only reprice withdrawals once executed; a failing
// `collect_fee` callback reverts the entire withdrawal (no partial payout, no
// orphaned fee transfer); waived users never reach the treasury; and the
// treasury's per-token and cumulative accounting matches every fee charged.

/// Deposit `deposit` for a fresh user and step past the withdraw cooldown.
fn funded_user(
    env: &Env,
    market: &MarketContractClient,
    market_id: u32,
    token: &Address,
    deposit: i128,
) -> Address {
    let user = Address::generate(env);
    StellarAssetClient::new(env, token).mint(&user, &deposit);
    market.deposit_collateral(&user, &market_id, &deposit);
    env.ledger().with_mut(|l| l.timestamp += 3601);
    user
}

/// Snapshot of every balance a withdraw can touch.
fn balances(
    env: &Env,
    token: &Address,
    market: &Address,
    treasury: &Address,
    user: &Address,
) -> [i128; 3] {
    let t = TokenClient::new(env, token);
    [t.balance(user), t.balance(market), t.balance(treasury)]
}

#[test]
fn fee_rate_change_reprices_withdrawals_only_after_timelock() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);
    let user = funded_user(&env, &market, market_id, &token, 1_000 * STROOPS_PER_USDC);
    let w = 100 * STROOPS_PER_USDC;

    // Rate 1: 50 bps.
    market.withdraw_unused_collateral(&user, &market_id, &w);
    let mut expected = fee_at(w, FEE_BPS);
    assert_eq!(treasury.total_collected(), expected);

    // Proposed but not executed: still 50 bps.
    market.set_fee_rate(&admin, &200);
    market.withdraw_unused_collateral(&user, &market_id, &w);
    expected += fee_at(w, FEE_BPS);
    assert_eq!(
        treasury.total_collected(),
        expected,
        "pending rate must not apply yet"
    );

    // Executed: subsequent withdrawals use 200 bps.
    advance_past_fee_timelock(&env);
    market.execute_fee_rate_change();
    market.withdraw_unused_collateral(&user, &market_id, &w);
    expected += fee_at(w, 200);
    assert_eq!(treasury.total_collected(), expected);
    assert_eq!(treasury.token_balance(&token), expected);
    assert_eq!(
        TokenClient::new(&env, &token).balance(&treasury_addr),
        expected
    );

    // Position accounting: every withdrawal deducted amount + fee (#377).
    let position = market.get_position(&market_id, &user).unwrap();
    assert_eq!(
        position.total_deposited,
        1_000 * STROOPS_PER_USDC - 3 * w - expected
    );
}

#[test]
fn canceled_fee_rate_change_never_applies() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);
    let user = funded_user(&env, &market, market_id, &token, 500 * STROOPS_PER_USDC);

    market.set_fee_rate(&admin, &1_000);
    market.cancel_fee_rate_change(&admin);
    advance_past_fee_timelock(&env);
    assert!(
        market.try_execute_fee_rate_change().is_err(),
        "nothing left to execute"
    );

    let w = 100 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&user, &market_id, &w);
    assert_eq!(treasury.total_collected(), fee_at(w, FEE_BPS));
}

#[test]
fn collect_fee_rejection_reverts_entire_withdrawal() {
    // Treasury wired to the market, but the treasury does not recognise this
    // market as an authorized caller → `collect_fee` fails with CallerNotMarket.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let market_addr = env.register(MarketContract, ());
    env.as_contract(&market_addr, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });
    let treasury_addr = env.register(TreasuryContract, ());
    TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &Address::generate(&env));
    let market = MarketContractClient::new(&env, &market_addr);
    market.set_treasury_contract(&admin, &treasury_addr);

    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let market_id = open_market(&env, &market, &admin, &token);
    let user = funded_user(&env, &market, market_id, &token, 100 * STROOPS_PER_USDC);

    let before = balances(&env, &token, &market_addr, &treasury_addr, &user);
    let position_before = market.get_position(&market_id, &user).unwrap();

    let result = market.try_withdraw_unused_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    assert!(
        result.is_err(),
        "a failed collect_fee callback must fail the withdraw"
    );

    // Fail closed: no payout, no fee transfer, no position change.
    assert_eq!(
        balances(&env, &token, &market_addr, &treasury_addr, &user),
        before
    );
    assert_eq!(
        market.get_position(&market_id, &user).unwrap(),
        position_before
    );
    assert_eq!(
        TreasuryContractClient::new(&env, &treasury_addr).total_collected(),
        0
    );
}

#[test]
fn paused_treasury_blocks_fee_withdrawals_until_unpaused() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);
    let user = funded_user(&env, &market, market_id, &token, 100 * STROOPS_PER_USDC);
    let w = 10 * STROOPS_PER_USDC;

    treasury.pause(&admin);
    let before = balances(&env, &token, &market_addr, &treasury_addr, &user);
    let position_before = market.get_position(&market_id, &user).unwrap();
    assert!(market
        .try_withdraw_unused_collateral(&user, &market_id, &w)
        .is_err());
    assert_eq!(
        balances(&env, &token, &market_addr, &treasury_addr, &user),
        before
    );
    assert_eq!(
        market.get_position(&market_id, &user).unwrap(),
        position_before
    );

    treasury.unpause(&admin);
    market.withdraw_unused_collateral(&user, &market_id, &w);
    assert_eq!(treasury.total_collected(), fee_for(w));
}

#[test]
fn waived_user_withdraw_skips_treasury() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);

    let waived = funded_user(&env, &market, market_id, &token, 100 * STROOPS_PER_USDC);
    let payer = funded_user(&env, &market, market_id, &token, 100 * STROOPS_PER_USDC);
    market.add_fee_waiver(&admin, &waived);

    // A waived user can take its whole deposit; no fee, no callback.
    market.withdraw_unused_collateral(&waived, &market_id, &(100 * STROOPS_PER_USDC));
    assert_eq!(
        TokenClient::new(&env, &token).balance(&waived),
        100 * STROOPS_PER_USDC
    );
    assert_eq!(treasury.total_collected(), 0);
    assert_eq!(
        market
            .get_position(&market_id, &waived)
            .unwrap()
            .total_deposited,
        0
    );

    // A non-waived user in the same market still pays.
    let w = 50 * STROOPS_PER_USDC;
    market.withdraw_unused_collateral(&payer, &market_id, &w);
    assert_eq!(treasury.total_collected(), fee_for(w));

    // Removing the waiver restores the fee.
    market.remove_fee_waiver(&admin, &waived);
    StellarAssetClient::new(&env, &token).mint(&waived, &(10 * STROOPS_PER_USDC));
    market.deposit_collateral(&waived, &market_id, &(10 * STROOPS_PER_USDC));
    env.ledger().with_mut(|l| l.timestamp += 3601);
    market.withdraw_unused_collateral(&waived, &market_id, &(5 * STROOPS_PER_USDC));
    assert_eq!(
        treasury.total_collected(),
        fee_for(w) + fee_for(5 * STROOPS_PER_USDC)
    );
}

#[test]
fn fee_rejected_withdraw_does_not_reach_treasury() {
    // amount + fee must fit the available balance; the exact-deposit request
    // fails (fee on top) and nothing is transferred anywhere.
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let market_id = open_market(&env, &market, &admin, &token);
    let deposit = 100 * STROOPS_PER_USDC;
    let user = funded_user(&env, &market, market_id, &token, deposit);

    let before = balances(&env, &token, &market_addr, &treasury_addr, &user);
    let err = market
        .try_withdraw_unused_collateral(&user, &market_id, &deposit)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        vatix_market_contract::error::ContractError::InsufficientCollateral
    );
    assert_eq!(
        balances(&env, &token, &market_addr, &treasury_addr, &user),
        before
    );

    // The largest amount whose fee fits succeeds and leaves only fee dust.
    let max = deposit * BPS_DENOM / (BPS_DENOM + FEE_BPS);
    market.withdraw_unused_collateral(&user, &market_id, &max);
    let left = market
        .get_position(&market_id, &user)
        .unwrap()
        .total_deposited;
    assert_eq!(left, deposit - max - fee_for(max));
    assert!(left >= 0);
}

#[test]
fn fees_from_two_users_accumulate_per_token() {
    let (env, market_addr, treasury_addr, admin, token) = setup_with_treasury();
    let market = MarketContractClient::new(&env, &market_addr);
    let treasury = TreasuryContractClient::new(&env, &treasury_addr);
    let market_id = open_market(&env, &market, &admin, &token);

    let a = funded_user(&env, &market, market_id, &token, 300 * STROOPS_PER_USDC);
    let b = funded_user(&env, &market, market_id, &token, 300 * STROOPS_PER_USDC);
    market.withdraw_unused_collateral(&a, &market_id, &(120 * STROOPS_PER_USDC));
    market.withdraw_unused_collateral(&b, &market_id, &(70 * STROOPS_PER_USDC));

    let total = fee_for(120 * STROOPS_PER_USDC) + fee_for(70 * STROOPS_PER_USDC);
    assert_eq!(treasury.total_collected(), total);
    assert_eq!(treasury.get_cumulative_fees(&token), total);
    assert_eq!(treasury.token_balance(&token), total);
    assert_eq!(
        treasury.list_fee_tokens(),
        soroban_sdk::vec![&env, token.clone()]
    );
}
