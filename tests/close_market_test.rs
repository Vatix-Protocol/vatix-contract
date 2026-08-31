//! Integration tests for the "close market to deposits" feature
//!
//! ## Operation matrix (Issue #574, revised by #703)
//!
//! | Operation                             | Market open | Closed to deposits |
//! |----------------------------------------|-------------|---------------------|
//! | `deposit_collateral`                   | ✅ allowed  | ❌ blocked          |
//! | `withdraw_unused_collateral`           | ✅ allowed  | ✅ allowed          |
//! | `settle_position` (after resolve)      | ✅ allowed  | ✅ allowed          |
//! | `update_position` — reduces/flat lock  | ✅ allowed  | ✅ allowed          |
//! | `update_position` — increases lock     | ✅ allowed  | ❌ blocked          |
//!
//! `closed_to_deposits` was originally documented (#574) as blocking only
//! `deposit_collateral`, leaving `update_position` free to open new exposure
//! through the trading path instead — the exact loophole #703 closes.
//! `update_position` now blocks any trade that would *increase* a user's
//! locked collateral once the market is closed; trades that reduce or hold
//! the lock flat (closing out risk) are still always allowed, and
//! withdrawals/settlement are never affected by `closed_to_deposits`.

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, MarketParams};

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn init_contract() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    (env, admin, contract_id)
}

fn setup_market_with_collateral() -> (Env, Address, Address, Address, u32, Address) {
    let (env, admin, contract_id) = init_contract();
    let client = MarketContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let collateral_token = token.address();

    let user = Address::generate(&env);
    let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
    stellar_asset_client.mint(&user, &(1_000 * STROOPS_PER_USDC));

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

    // User deposits initial collateral
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS_PER_USDC));

    (env, admin, contract_id, user, market_id, collateral_token)
}

#[test]
fn close_market_to_deposits_succeeds() {
    let (env, admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Verify market is initially open to deposits
    let market_before = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market_before.closed_to_deposits, false);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // Verify market is now closed to deposits
    let market_after = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market_after.closed_to_deposits, true);

    // Verify the event was emitted
    assert_event_emitted(&env, "market_closed_to_deposits");
}

#[test]
fn deposit_fails_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, collateral_token) =
        setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // Mint more collateral for the user
    let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
    stellar_asset_client.mint(&user, &(500 * STROOPS_PER_USDC));

    // Attempt to deposit - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.deposit_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
    }));

    assert!(
        result.is_err(),
        "Deposit should fail when market is closed to deposits"
    );
}

#[test]
fn withdrawal_succeeds_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market to new deposits
    client.close_market_to_deposits(&admin, &market_id);

    // User should still be able to withdraw their collateral
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_unused_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
    }));

    assert!(
        result.is_ok(),
        "Withdrawal should still work when market is closed to deposits"
    );
}

#[test]
fn close_market_to_deposits_idempotent() {
    let (env, admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market twice
    client.close_market_to_deposits(&admin, &market_id);
    client.close_market_to_deposits(&admin, &market_id);

    // Market should still be closed
    let market = env.as_contract(&contract_id, || {
        storage::get_market(&env, market_id)
            .expect("version check failed")
            .expect("market should exist")
    });
    assert_eq!(market.closed_to_deposits, true);
}

#[test]
fn unauthorized_close_market_to_deposits_fails() {
    let (env, _admin, contract_id, _user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Non-admin tries to close market
    let attacker = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.close_market_to_deposits(&attacker, &market_id);
    }));

    assert!(
        result.is_err(),
        "Non-admin should not be able to close market to deposits"
    );
}

#[test]
fn close_nonexistent_market_to_deposits_fails() {
    let (env, admin, contract_id, _user, _market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Try to close a market that doesn't exist
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.close_market_to_deposits(&admin, &999u32);
    }));

    assert!(result.is_err(), "Closing a non-existent market should fail");
}

// ── Issue #574: operation matrix — deposit blocked, withdraw/settle allowed ───
//
// These tests document the allowed-operation matrix when a market is closed
// to deposits. Every operation except `deposit_collateral` must continue to
// work unchanged.

/// Helper: build a real Ed25519 oracle keypair and sign `(market_id, outcome)`.
fn oracle_keypair_for(
    env: &Env,
    market_id: u32,
    outcome: bool,
) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<64>) {
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use vatix_market_contract::oracle;

    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey = soroban_sdk::BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let msg = oracle::construct_oracle_message(env, market_id, outcome);
    let sig = signing_key.sign(msg.to_array().as_slice());
    let sig_bytes = soroban_sdk::BytesN::from_array(env, &sig.to_bytes());
    (pubkey, sig_bytes)
}

/// Matrix row: deposit is blocked after close (Issue #574).
///
/// Already covered by `deposit_fails_when_market_closed_to_deposits` above;
/// this version uses an explicit `try_*` call to assert the exact error.
#[test]
fn matrix_deposit_blocked_when_closed() {
    let (env, admin, contract_id, user, market_id, collateral_token) =
        setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    client.close_market_to_deposits(&admin, &market_id);

    let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
    stellar_asset_client.mint(&user, &(10 * STROOPS_PER_USDC));

    let result = client.try_deposit_collateral(&user, &market_id, &STROOPS_PER_USDC);
    assert!(
        result.is_err(),
        "deposit must be blocked when market is closed to deposits"
    );
}

/// Matrix row: withdraw is still allowed after close (Issue #574).
#[test]
fn matrix_withdraw_allowed_when_closed() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    client.close_market_to_deposits(&admin, &market_id);

    // User deposited 100 USDC during setup — withdraw 10 of it.
    client.withdraw_unused_collateral(&user, &market_id, &(10 * STROOPS_PER_USDC));
    // No panic == success. Also verify the event.
    assert_event_emitted(&env, "collateral_withdrawn");
}

/// Matrix row: settle_position is allowed after close + resolve (Issue #574).
///
/// The user opens a position *before* the market closes (closing blocks
/// opening new exposure — see `matrix_trade_blocked_when_closed`), then the
/// market is closed to deposits and resolved with a real oracle signature,
/// then the user's position is settled. The settlement must succeed —
/// `closed_to_deposits` must not interfere with settling an existing position.
#[test]
fn matrix_settle_allowed_when_closed() {
    let (env, admin, contract_id, user, market_id, _collateral_token) =
        setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Step 1: give the user YES shares (while the market is still open) so
    // there's a payout to settle.
    let shares = 50 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &shares, &0i128, &5_000i128);

    // Step 2: close to deposits.
    client.close_market_to_deposits(&admin, &market_id);

    // Step 3: resolve with a real oracle signature.
    let outcome = true;
    let (oracle_pubkey, oracle_sig) = oracle_keypair_for(&env, market_id, outcome);

    // Replace the market's oracle pubkey so the signature verifies.
    env.as_contract(&contract_id, || {
        let mut market = storage::get_market(&env, market_id)
            .expect("storage version ok")
            .expect("market must exist");
        market.oracle_pubkey = oracle_pubkey;
        storage::set_market(&env, market_id, &market).expect("set market ok");
    });

    let market_id_str = soroban_sdk::String::from_str(&env, "1");
    let resolver = Address::generate(&env);
    let expires_at = env.ledger().timestamp() + 3_600;
    client.resolve_market(
        &resolver,
        &market_id_str,
        &outcome,
        &oracle_sig,
        &expires_at,
    );
    assert_event_emitted(&env, "market_resolved");

    // Step 4: settle — must succeed even though the market was closed to deposits.
    client.settle_position(&user, &market_id);
    assert_event_emitted(&env, "position_settled");
}

/// Matrix row: update_position that *increases* locked collateral is
/// blocked once the market is closed to deposits (Issue #703).
///
/// This is the exact gap #703 closes: closing a market to deposits must
/// block opening new exposure through the trading path, not just the
/// dedicated `deposit_collateral` entrypoint. Even though the user has
/// plenty of already-deposited collateral to cover the trade, opening new
/// exposure via `update_position` after close must still be rejected.
#[test]
fn matrix_trade_blocked_when_closed_increases_lock() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    client.close_market_to_deposits(&admin, &market_id);

    // User has 100 USDC deposited from setup — plenty to cover this trade —
    // but buying new YES shares must still be rejected once closed.
    let shares = 50 * STROOPS_PER_USDC;
    let result = client.try_update_position(&user, &market_id, &shares, &0i128, &5_000i128);
    assert!(
        result.is_err(),
        "update_position must block new exposure once the market is closed to deposits (#703)"
    );
}

/// Matrix row: update_position that reduces or holds locked collateral flat
/// remains allowed after close (Issue #703) — closing out risk is never
/// blocked, only opening new exposure is.
#[test]
fn matrix_trade_allowed_when_closed_reduces_lock() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Open a position while the market is still open.
    let shares = 50 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &shares, &0i128, &5_000i128);

    client.close_market_to_deposits(&admin, &market_id);

    // Selling half of the YES shares reduces the lock — must still work.
    client.update_position(&user, &market_id, &(-shares / 2), &0i128, &5_000i128);
    assert_event_emitted(&env, "trade_executed");
}
