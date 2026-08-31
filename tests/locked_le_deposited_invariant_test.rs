//! Table-driven invariant coverage: `locked_collateral` must never exceed
//! `total_deposited` for any `Position`.
//!
//! This complements the existing randomized coverage in
//! `tests/collateral_invariant_test.rs`, `tests/proptest_locked_invariant.rs`
//! and `contracts/market/src/positions.rs::proptest_tests` with a small set
//! of explicit, deterministic, named scenarios (deposit-only, net-YES,
//! net-NO, hedged, boundary-exact, and the documented failure/rejection
//! case) so the invariant's edge cases are easy to read and reason about
//! without relying on fuzzing to hit them.
//!
//! ## The invariant
//! `position.locked_collateral <= position.total_deposited` must hold after
//! every `deposit_collateral`, `update_position`, and
//! `withdraw_unused_collateral` call.
//!
//! ## Documented failure case
//! A trade that would *increase* the lock beyond the currently deposited
//! collateral is rejected outright with `ContractError::InsufficientCollateral`
//! (see `MarketContract::update_position`, step 4, in
//! `contracts/market/src/lib.rs`). The position is left completely
//! unmodified on that path — the invariant is preserved by refusing the
//! state transition, not by clamping or silently truncating it.

#[allow(dead_code)]
mod helpers;

use helpers::MarketParams;

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{error::ContractError, storage, MarketContract, MarketContractClient};

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
        &None,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
    client.deposit_collateral(&user, &market_id, &deposit);

    (env, contract_id, market_id, user)
}

fn assert_invariant(env: &Env, contract_id: &Address, market_id: u32, user: &Address) {
    let position = env
        .as_contract(contract_id, || storage::get_position(env, market_id, user).unwrap())
        .expect("position should exist");
    assert!(
        position.locked_collateral <= position.total_deposited,
        "invariant violated: locked={} > deposited={}",
        position.locked_collateral,
        position.total_deposited
    );
    assert!(position.locked_collateral >= 0);
}

/// Table of (yes_shares_pct_of_deposit, no_shares_pct_of_deposit, price_bps,
/// case_name) scenarios, all of which must leave the invariant intact.
struct Case {
    name: &'static str,
    deposit_usdc: i128,
    yes_shares: i128,
    no_shares: i128,
    price_bps: i128,
}

#[test]
fn table_driven_locked_never_exceeds_deposited() {
    let cases = [
        Case { name: "deposit_only_no_trade", deposit_usdc: 100, yes_shares: 0, no_shares: 0, price_bps: 5_000 },
        Case { name: "net_yes_half_price", deposit_usdc: 100, yes_shares: 100 * STROOPS_PER_USDC, no_shares: 0, price_bps: 5_000 },
        Case { name: "net_yes_boundary_exact_deposit", deposit_usdc: 100, yes_shares: 100 * STROOPS_PER_USDC, no_shares: 0, price_bps: 10_000 },
        Case { name: "net_no_low_price", deposit_usdc: 100, yes_shares: 0, no_shares: 100 * STROOPS_PER_USDC, price_bps: 1_000 },
        Case { name: "hedged_equal_shares_zero_lock", deposit_usdc: 100, yes_shares: 50 * STROOPS_PER_USDC, no_shares: 50 * STROOPS_PER_USDC, price_bps: 7_500 },
        Case { name: "small_deposit_small_trade", deposit_usdc: 1, yes_shares: STROOPS_PER_USDC / 2, no_shares: 0, price_bps: 6_000 },
    ];

    for case in cases {
        let deposit = case.deposit_usdc * STROOPS_PER_USDC;
        let (env, contract_id, market_id, user) = setup_market(deposit);
        let client = MarketContractClient::new(&env, &contract_id);

        let result =
            client.try_update_position(&user, &market_id, &case.yes_shares, &case.no_shares, &case.price_bps);
        assert!(result.is_ok(), "case '{}' expected success: {:?}", case.name, result);

        assert_invariant(&env, &contract_id, market_id, &user);
    }
}

/// Documented failure case: a trade whose required lock exceeds the
/// deposited collateral must be rejected with `InsufficientCollateral`, and
/// the position must be left completely unchanged (invariant preserved by
/// refusal, not by clamping).
#[test]
fn over_leveraged_trade_is_rejected_and_position_unchanged() {
    let deposit_usdc = 10i128;
    let deposit = deposit_usdc * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Buying 100 USDC worth of YES shares at 100% price requires 100 USDC
    // locked against only 10 USDC deposited.
    let yes_shares = 100 * STROOPS_PER_USDC;
    let price_bps = 10_000i128;

    let position_before = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).unwrap()
    });

    let result = client.try_update_position(&user, &market_id, &yes_shares, &0i128, &price_bps);
    assert_eq!(
        result,
        Err(Ok(ContractError::InsufficientCollateral)),
        "over-leveraged trade must be rejected with InsufficientCollateral"
    );

    let position_after = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).unwrap()
    });
    assert_eq!(
        position_before, position_after,
        "a rejected trade must not mutate the stored position at all"
    );

    assert_invariant(&env, &contract_id, market_id, &user);
}

/// Sequence of trades and a partial withdraw must keep the invariant true
/// after every single step, not just at the end.
#[test]
fn invariant_holds_after_every_step_in_a_trade_sequence() {
    let deposit = 200 * STROOPS_PER_USDC;
    let (env, contract_id, market_id, user) = setup_market(deposit);
    let client = MarketContractClient::new(&env, &contract_id);

    // Step 1: buy YES.
    client.update_position(&user, &market_id, &(80 * STROOPS_PER_USDC), &0i128, &6_000i128);
    assert_invariant(&env, &contract_id, market_id, &user);

    // Step 2: partially hedge with NO.
    client.update_position(&user, &market_id, &0i128, &(30 * STROOPS_PER_USDC), &6_000i128);
    assert_invariant(&env, &contract_id, market_id, &user);

    // Step 3: withdraw whatever remains available.
    let position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).unwrap().unwrap()
    });
    let available = position.total_deposited - position.locked_collateral;
    if available > 0 {
        client.withdraw_unused_collateral(&user, &market_id, &available);
        assert_invariant(&env, &contract_id, market_id, &user);
    }

    // Step 4: sell back down to fully hedged.
    client.update_position(&user, &market_id, &(-80 * STROOPS_PER_USDC), &(-30 * STROOPS_PER_USDC), &6_000i128);
    assert_invariant(&env, &contract_id, market_id, &user);

    let final_position = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user).unwrap().unwrap()
    });
    assert_eq!(final_position.yes_shares, 0);
    assert_eq!(final_position.no_shares, 0);
    assert_eq!(final_position.locked_collateral, 0);
}
