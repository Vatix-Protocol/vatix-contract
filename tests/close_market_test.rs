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
use vatix_market_contract::{storage, ContractError, MarketContract, MarketContractClient};

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

// ── Issue #703: `update_position` lock-increase block ────────────────────────
//
// Once a market is closed to deposits, `update_position` must reject any
// trade that would *increase* a user's locked collateral (the loophole #703
// closes), while still allowing trades that reduce or hold the lock flat.

#[test]
fn update_position_increasing_lock_fails_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Close the market to new deposits before any position is opened.
    client.close_market_to_deposits(&admin, &market_id);

    // Opening a position here would increase locked collateral from zero —
    // exactly the loophole #703 closes. It must be rejected with the
    // dedicated error discriminant.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.update_position(&user, &market_id, &true, &(10 * STROOPS_PER_USDC));
    }));

    assert!(
        result.is_err(),
        "update_position increasing locked collateral must fail when market is closed to deposits"
    );

    // Assert the specific ContractError discriminant, not just any panic.
    let err = result.unwrap_err();
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains(&format!("{:?}", ContractError::MarketClosedToDeposits)),
        "expected ContractError::MarketClosedToDeposits, got: {msg}"
    );
}

#[test]
fn update_position_reducing_lock_succeeds_when_market_closed_to_deposits() {
    let (env, admin, contract_id, user, market_id, _token) = setup_market_with_collateral();
    let client = MarketContractClient::new(&env, &contract_id);

    // Open a position while the market is still open to deposits.
    client.update_position(&user, &market_id, &true, &(20 * STROOPS_PER_USDC));

    let position_before = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .expect("version check failed")
            .expect("position should exist")
    });
    let locked_before = position_before.locked_collateral;
    assert!(locked_before > 0, "position should have locked collateral");

    // Now close the market to deposits.
    client.close_market_to_deposits(&admin, &market_id);

    // Reducing the lock (closing out risk) must still be allowed.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.update_position(&user, &market_id, &false, &(5 * STROOPS_PER_USDC));
    }));
    assert!(
        result.is_ok(),
        "update_position reducing locked collateral must succeed when market is closed to deposits"
    );

    // The stored position's locked collateral must have decreased.
    let position_after = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .expect("version check failed")
            .expect("position should exist")
    });
    assert!(
        position_after.locked_collateral < locked_before,
        "locked_collateral should decrease after a reducing update_position"
    );

    // The trade event must have been emitted.
    assert_event_emitted(&env, "position_updated");
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
    let sig = signing_key.sign(msg.

/* … truncated 5715 chars — edit only what you need near the top … */
