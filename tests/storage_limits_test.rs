//! Simulates mainnet storage/resource limits (#505).
//!
//! Soroban test environments run with an *unlimited* budget by default, which
//! hides resource-exhaustion regressions that would only surface on mainnet,
//! where CPU instructions and memory are capped per the network's published
//! limits. This test resets the budget to those mainnet-equivalent defaults
//! (`Budget::reset_default`) before exercising a storage-heavy workload —
//! creating a market and opening many independent user positions — so a path
//! that grows too storage/compute heavy as usage scales fails here instead of
//! in production.

#[allow(dead_code)]
mod helpers;

use helpers::assert_event_emitted;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};

const STROOPS_PER_USDC: i128 = 10_000_000;

/// Number of distinct user positions opened against a single market. Large
/// enough to meaningfully grow contract storage (one `Position` ledger entry
/// per user) while keeping the test fast.
const POSITION_COUNT: u32 = 50;

#[test]
fn market_with_many_positions_stays_within_mainnet_budget() {
    let env = Env::default();
    env.mock_all_auths();

    // From here on, every host operation is metered against the same
    // CPU/memory limits a real mainnet transaction would face, instead of
    // the unlimited budget the test harness uses by default.
    env.cost_estimate().budget().reset_default();

    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_admin(&env, &admin);
    });

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let collateral_token = token.address();

    let question = soroban_sdk::String::from_str(&env, "Will BTC reach $100k?");
    let end_time = env.ledger().timestamp() + 86_400;
    let oracle_pubkey = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

    let market_id = client.initialize_market(
        &admin,
        &question,
        &end_time,
        &oracle_pubkey,
        &collateral_token,
    );

    let deposit = 100 * STROOPS_PER_USDC;
    for _ in 0..POSITION_COUNT {
        let user = Address::generate(&env);
        StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);
        client.buy_yes(&user, &market_id, &(10 * STROOPS_PER_USDC), &5_000i128);
    }

    assert_event_emitted(&env, "trade_executed_event");

    // Reaching this point means the whole flow completed without exceeding
    // the mainnet-equivalent CPU/memory budget above. Sanity-check the
    // tracker actually recorded consumption, so a future SDK change that
    // silently makes `reset_default` a no-op doesn't make this test vacuous.
    let budget = env.cost_estimate().budget();
    assert!(
        budget.cpu_instruction_cost() > 0,
        "expected the mainnet budget tracker to record consumed CPU instructions"
    );
    assert!(
        budget.memory_bytes_cost() > 0,
        "expected the mainnet budget tracker to record consumed memory"
    );
}
