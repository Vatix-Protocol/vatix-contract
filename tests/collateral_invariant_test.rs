//! Regression and property tests for #262: reconciling `Position::locked_collateral`
//! between `deposit_collateral`, `update_position`, and `withdraw_unused_collateral`.
//!
//! Before the fix:
//! - `deposit_collateral` incremented `locked_collateral` by the deposit amount,
//!   so a deposit with zero shares already looked "locked".
//! - `withdraw_unused_collateral` ignored the stored `locked_collateral` and
//!   recomputed its own lock from a hardcoded 50/50 price, diverging from
//!   whatever price `update_position` actually used.
//!
//! These tests assert the core invariant `locked_collateral <= total_deposited`
//! holds across deposit -> update_position -> withdraw sequences, and pin down
//! the two concrete regressions described above.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use rand::{rngs::StdRng, Rng, SeedableRng};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

/// Register the contract, create a market backed by a real asset, and fund +
/// deposit `deposit` stroops for a fresh user.
///
/// Returns the env, contract id, market id, and user.
fn setup_market(deposit: i128) -> (Env, Address, u32, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
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
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    (env, contract_id, market_id, user)
}

/// Regression test: a deposit with zero shares held must show zero locked
/// collateral, not the deposited amount.
#[test]
fn deposit_with_zero_shares_has_zero_locked_collateral() {
    let deposit = 50 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).expect("position should exist")
    });

    assert_eq!(position.yes_shares, 0);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.total_deposited, deposit);
    assert_eq!(position.locked_collateral, 0);
}

/// Regression test: withdraw must use the real trade price recorded by
/// `update_position`, not a hardcoded 50/50 split.
#[test]
fn withdraw_uses_real_trade_price_not_hardcoded_fifty_fifty() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buy 100 YES shares at 60% -> locks 60 USDC (not the 50 USDC a
    // hardcoded 50/50 calculation would produce).
    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &6_000i128);

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).expect("position should exist")
    });
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);

    // Under the old hardcoded-50% logic, available would be 100 - 50 = 50,
    // so withdrawing 45 would have succeeded. The real lock is 60, leaving
    // only 40 available, so this must now fail.
    let over_withdraw =
        client.try_withdraw_unused_collateral(&user, &market_id, &(45 * STROOPS_PER_USDC));
    assert!(over_withdraw.is_err());

    // Exactly the truly available 40 USDC can be withdrawn.
    client.withdraw_unused_collateral(&user, &market_id, &(40 * STROOPS_PER_USDC));

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).expect("position should exist")
    });
    assert_eq!(position.total_deposited, 60 * STROOPS_PER_USDC);
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);
}

/// Property test: across many randomized deposit -> buy -> withdraw
/// sequences (at random, non-50/50 prices), `locked_collateral` must never
/// exceed `total_deposited`, and must never go negative.
#[test]
fn property_locked_collateral_never_exceeds_total_deposited() {
    let mut rng = StdRng::seed_from_u64(0x262);

    for trial in 0u32..40 {
        let deposit_fraction = rng.gen_range(1u64..=500);
        let deposit_amount = (deposit_fraction as i128) * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market(deposit_amount);
        let client = MarketContractClient::new(&env, &contract_id);

        let steps = rng.gen_range(1u32..=10);
        for _ in 0..steps {
            let price = rng.gen_range(1u64..=9_999) as i128;

            let position = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).expect("position should exist")
            });

            match rng.gen_range(0u32..3) {
                0 => {
                    // Buy a random amount of YES shares, bounded by total
                    // deposited so most attempts succeed without overflow.
                    let pct = rng.gen_range(0u64..=100);
                    let yes_delta = position.total_deposited * pct as i128 / 100;
                    let _ =
                        client.try_update_position(&user, &market_id, &yes_delta, &0i128, &price);
                }
                1 => {
                    let pct = rng.gen_range(0u64..=100);
                    let no_delta = position.total_deposited * pct as i128 / 100;
                    let _ =
                        client.try_update_position(&user, &market_id, &0i128, &no_delta, &price);
                }
                _ => {
                    let pct = rng.gen_range(1u64..=100);
                    let amount = (position.total_deposited * pct as i128 / 100).max(1);
                    let _ = client.try_withdraw_unused_collateral(&user, &market_id, &amount);
                }
            }

            let position = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).expect("position should exist")
            });

            assert!(
                position.locked_collateral <= position.total_deposited,
                "trial {trial}: invariant violated, locked {} > deposited {}",
                position.locked_collateral,
                position.total_deposited
            );
            assert!(
                position.locked_collateral >= 0,
                "trial {trial}: locked_collateral went negative: {}",
                position.locked_collateral
            );
        }
    }
}
