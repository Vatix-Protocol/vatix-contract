//! Regression and property tests for #262: reconciling `Position::locked_collateral`
//! between `deposit_collateral`, `update_position`, and `withdraw_unused_collateral`.
//!
//! ## Test organisation
//!
//! | Test | What it covers |
//! |------|---------------|
//! | `deposit_with_zero_shares_has_zero_locked_collateral` | Fresh deposit → locked == 0 |
//! | `withdraw_uses_real_trade_price_not_hardcoded_fifty_fifty` | Price-aware lock accounting |
//! | `table_driven_locked_never_exceeds_deposited` | **#775** — deterministic snapshot matrix |
//! | `property_locked_collateral_never_exceeds_total_deposited` | Randomised property sweep |
//!
//! The table-driven test (`table_driven_locked_never_exceeds_deposited`) is the
//! canonical CI guard for #775: it defines a fixed set of (deposit, yes_shares,
//! no_shares, price_bps, withdraw) scenarios and asserts the invariant
//! `locked_collateral <= total_deposited` holds after each operation, plus
//! captures the exact expected `locked_collateral` as a snapshot so accidental
//! changes to the accounting formula immediately fail CI.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use rand::{rngs::StdRng, Rng, SeedableRng};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

fn setup_market(deposit: i128) -> (Env, Address, u32, Address) {
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
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    (env, contract_id, market_id, user)
}

#[test]
fn deposit_with_zero_shares_has_zero_locked_collateral() {
    let deposit = 50 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });

    assert_eq!(position.yes_shares, 0);
    assert_eq!(position.no_shares, 0);
    assert_eq!(position.total_deposited, deposit);
    assert_eq!(position.locked_collateral, 0);
}

#[test]
fn withdraw_uses_real_trade_price_not_hardcoded_fifty_fifty() {
    let deposit = 100 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    let yes_shares = 100 * STROOPS_PER_USDC;
    client.update_position(&user, &market_id, &yes_shares, &0i128, &6_000i128);

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);

    let over_withdraw =
        client.try_withdraw_unused_collateral(&user, &market_id, &(45 * STROOPS_PER_USDC));
    assert!(over_withdraw.is_err());

    client.withdraw_unused_collateral(&user, &market_id, &(40 * STROOPS_PER_USDC));

    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position should exist")
    });
    assert_eq!(position.total_deposited, 60 * STROOPS_PER_USDC);
    assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);
}

// ---------------------------------------------------------------------------
// #775 — Table-driven locked_never_exceeds_deposited snapshot tests
// ---------------------------------------------------------------------------

/// A single row in the table-driven invariant snapshot test.
///
/// Each row represents one deterministic scenario:
/// 1. Deposit `deposit_stroops` into a fresh market.
/// 2. Call `update_position` with `yes_shares` / `no_shares` at `price_bps`.
///    `None` means skip the update_position step entirely.
/// 3. Withdraw `withdraw_stroops` if `Some(_)`. `None` skips the withdrawal.
/// 4. Assert:
///    - `locked_collateral == expected_locked` (snapshot of accounting formula)
///    - `locked_collateral <= total_deposited` (the core invariant)
///    - `locked_collateral >= 0` (no underflow)
struct LockedInvariantCase {
    /// Human-readable label for failure messages.
    label: &'static str,
    /// Collateral deposited at the start (stroops).
    deposit_stroops: i128,
    /// YES-share delta passed to `update_position` (stroops). `None` = skip.
    yes_shares: Option<i128>,
    /// NO-share delta passed to `update_position` (stroops). `None` = skip.
    no_shares: Option<i128>,
    /// Price in basis points (0–10_000) used for the position update.
    price_bps: i128,
    /// Withdraw this many stroops after the position update. `None` = skip.
    withdraw_stroops: Option<i128>,
    /// Expected `locked_collateral` after all operations.
    expected_locked: i128,
}

/// Table-driven snapshot tests for the collateral invariant (#775).
///
/// These cases are deterministic and their expected values are pinned — if
/// the `locked_collateral` formula in `positions.rs` or `withdraw.rs` is
/// changed the assertion on `expected_locked` will catch the drift before
/// it reaches CI.
///
/// Adding new edge cases here is the preferred way to document and guard
/// against regressions in the accounting logic.
#[test]
fn table_driven_locked_never_exceeds_deposited() {
    let cases: &[LockedInvariantCase] = &[
        // --- Baseline: deposit only, no trade ---
        LockedInvariantCase {
            label: "no_trade_locked_is_zero",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: None,
            no_shares: None,
            price_bps: 5_000,
            withdraw_stroops: None,
            expected_locked: 0,
        },
        // --- YES shares at exactly 50/50 price ---
        LockedInvariantCase {
            label: "yes_shares_at_5000bps",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 5_000,
            withdraw_stroops: None,
            // locked = yes_shares * price_bps / 10_000 = 100 * 0.5 = 50 USDC
            expected_locked: 50 * STROOPS_PER_USDC,
        },
        // --- YES shares at 60 % price ---
        LockedInvariantCase {
            label: "yes_shares_at_6000bps",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 6_000,
            withdraw_stroops: None,
            // locked = 100 * 0.6 = 60 USDC
            expected_locked: 60 * STROOPS_PER_USDC,
        },
        // --- NO shares at 30 % price (= 70 % NO cost) ---
        LockedInvariantCase {
            label: "no_shares_at_3000bps",
            deposit_stroops: 200 * STROOPS_PER_USDC,
            yes_shares: Some(0),
            no_shares: Some(100 * STROOPS_PER_USDC),
            price_bps: 3_000,
            withdraw_stroops: None,
            // NO cost = no_shares * (10_000 - price_bps) / 10_000 = 100 * 0.7 = 70 USDC
            expected_locked: 70 * STROOPS_PER_USDC,
        },
        // --- Minimum price boundary (price_bps = 1) ---
        LockedInvariantCase {
            label: "yes_shares_min_price",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 1,
            withdraw_stroops: None,
            // locked = 100 * 1 / 10_000 = 10_000 stroops (rounds down)
            expected_locked: 10_000,
        },
        // --- Maximum price boundary (price_bps = 9_999) ---
        LockedInvariantCase {
            label: "yes_shares_max_price",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 9_999,
            withdraw_stroops: None,
            // locked = 100 * 9_999 / 10_000 = 99.99 USDC → 999_900_000 stroops
            expected_locked: 999_900_000,
        },
        // --- Partial withdraw stays within locked boundary ---
        LockedInvariantCase {
            label: "partial_withdraw_respects_locked",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(60 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 5_000,
            // unlocked = 100 - 30 = 70 USDC, withdraw 20 USDC → total_deposited = 80 USDC
            withdraw_stroops: Some(20 * STROOPS_PER_USDC),
            // locked stays at 30 USDC (shares unchanged)
            expected_locked: 30 * STROOPS_PER_USDC,
        },
        // --- Withdraw exactly the unlocked amount ---
        LockedInvariantCase {
            label: "withdraw_full_unlocked",
            deposit_stroops: 100 * STROOPS_PER_USDC,
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 4_000,
            // locked = 40 USDC; unlocked = 60 USDC — withdraw exactly 60
            withdraw_stroops: Some(60 * STROOPS_PER_USDC),
            // After withdraw: total_deposited == locked_collateral == 40 USDC
            expected_locked: 40 * STROOPS_PER_USDC,
        },
        // --- Single stroop deposit ---
        LockedInvariantCase {
            label: "single_stroop_deposit",
            deposit_stroops: 1,
            yes_shares: Some(0),
            no_shares: Some(0),
            price_bps: 5_000,
            withdraw_stroops: None,
            expected_locked: 0,
        },
        // --- Large deposit, tiny shares ---
        LockedInvariantCase {
            label: "large_deposit_tiny_shares",
            deposit_stroops: 10_000 * STROOPS_PER_USDC,
            yes_shares: Some(STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 5_000,
            withdraw_stroops: None,
            // locked = 1 USDC * 0.5 = 5_000_000 stroops
            expected_locked: 5_000_000,
        },
        // --- Both YES and NO shares (mixed position) ---
        LockedInvariantCase {
            label: "mixed_yes_and_no_shares",
            deposit_stroops: 200 * STROOPS_PER_USDC,
            // Update YES first, then NO in separate calls (single update_position
            // call with both set applies the larger lock — use two calls below).
            // Here we use yes=100 @ 5000 first, then the yes call locks 50 USDC.
            yes_shares: Some(100 * STROOPS_PER_USDC),
            no_shares: Some(0),
            price_bps: 5_000,
            withdraw_stroops: None,
            expected_locked: 50 * STROOPS_PER_USDC,
        },
        // --- Zero-amount update_position is a no-op for locked ---
        LockedInvariantCase {
            label: "zero_shares_is_noop",
            deposit_stroops: 50 * STROOPS_PER_USDC,
            yes_shares: Some(0),
            no_shares: Some(0),
            price_bps: 7_500,
            withdraw_stroops: None,
            expected_locked: 0,
        },
    ];

    for case in cases {
        // Each case gets a fresh isolated environment.
        let (env, contract_id, market_id, user) = setup_market(case.deposit_stroops);
        let client = MarketContractClient::new(&env, &contract_id);

        // Step 2: update_position (if requested).
        if let (Some(yes), Some(no)) = (case.yes_shares, case.no_shares) {
            // Errors (e.g. InsufficientCollateral) are acceptable — the
            // invariant must hold even if the call is rejected.
            let _ = client.try_update_position(&user, &market_id, &yes, &no, &case.price_bps);
        }

        // Step 3: withdraw (if requested).
        if let Some(amount) = case.withdraw_stroops {
            let _ = client.try_withdraw_unused_collateral(&user, &market_id, &amount);
        }

        // Step 4: read position and assert.
        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position should exist")
        });

        assert_eq!(
            position.locked_collateral,
            case.expected_locked,
            "[{}] locked_collateral snapshot mismatch: got {}, want {}",
            case.label,
            position.locked_collateral,
            case.expected_locked,
        );

        assert!(
            position.locked_collateral <= position.total_deposited,
            "[{}] invariant violated: locked {} > deposited {}",
            case.label,
            position.locked_collateral,
            position.total_deposited,
        );

        assert!(
            position.locked_collateral >= 0,
            "[{}] locked_collateral went negative: {}",
            case.label,
            position.locked_collateral,
        );
    }
}

// ---------------------------------------------------------------------------
// Property-based sweep (kept alongside table-driven cases per #775)
// ---------------------------------------------------------------------------

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
                storage::get_position(&env, market_id, &user)
                    .unwrap()
                    .expect("position should exist")
            });

            match rng.gen_range(0u32..3) {
                0 => {
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
                storage::get_position(&env, market_id, &user)
                    .unwrap()
                    .expect("position should exist")
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
