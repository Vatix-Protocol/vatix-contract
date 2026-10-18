extern crate std;

use crate::{TreasuryContract, TreasuryContractClient, TreasuryError};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, IntoVal, Map, Symbol, TryIntoVal, Val,
};

/// Boot a treasury with `admin` as the protocol admin and `market` as the
/// sole initially-registered market contract authorized to call
/// `collect_fee`. Returns the treasury contract's own address too, so tests
/// can seed it with a real token balance for `withdraw_fees`.
fn setup(env: &Env) -> (TreasuryContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let market = Address::generate(env);
    client.initialize(&admin, &market);
    (client, admin, market, contract_id)
}

/// Deploy a Stellar Asset Contract test token, mirroring the convention used
/// in `tests/treasury_integration_test.rs`.
fn create_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

fn mint(env: &Env, token: &Address, who: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(who, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

/// Find the most recent event tagged with topic `name` among all events
/// emitted so far, returning its `(topics, data)` pair. Mirrors
/// `find_fee_collected_event` in `tests/treasury_integration_test.rs`.
fn find_event(env: &Env, name: &str) -> Option<(std::vec::Vec<Val>, Val)> {
    let target = Symbol::new(env, name);
    let all: std::vec::Vec<(Address, soroban_sdk::Vec<Val>, Val)> =
        env.events().all().iter().collect();
    for (_contract, topics, data) in all.into_iter().rev() {
        let topics: std::vec::Vec<Val> = topics.iter().collect();
        let first: Symbol = topics[0].clone().into_val(env);
        if first == target {
            return Some((topics, data));
        }
    }
    None
}

fn data_map(env: &Env, data: Val) -> Map<Symbol, Val> {
    data.try_into_val(env).expect("data must be a Map<Symbol,Val>")
}

fn data_i128(env: &Env, m: &Map<Symbol, Val>, key: &str) -> i128 {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

// ── #1021: add_market / remove_market idempotency + admin-only guard ────────

#[test]
fn add_market_and_remove_market_reject_non_admin() {
    let env = Env::default();
    let (client, _admin, _market, _treasury) = setup(&env);
    let attacker = Address::generate(&env);
    let new_market = Address::generate(&env);

    let add_result = client.try_add_market(&attacker, &new_market);
    assert_eq!(add_result, Err(Ok(TreasuryError::Unauthorized)));

    let remove_result = client.try_remove_market(&attacker, &new_market);
    assert_eq!(remove_result, Err(Ok(TreasuryError::Unauthorized)));
}

#[test]
fn add_market_twice_is_idempotent() {
    let env = Env::default();
    let (client, admin, market, _treasury) = setup(&env);
    let new_market = Address::generate(&env);

    client.add_market(&admin, &new_market);
    client.add_market(&admin, &new_market);

    let markets = client.list_markets();
    let occurrences = markets.iter().filter(|m| m == &new_market).count();
    assert_eq!(occurrences, 1);
    // The market registered at `initialize` is untouched.
    assert!(markets.contains(&market));
    assert_eq!(markets.len(), 2);
}

#[test]
fn remove_market_for_absent_address_is_a_noop() {
    let env = Env::default();
    let (client, admin, market, _treasury) = setup(&env);
    let absent = Address::generate(&env);

    let before = client.list_markets();
    client.remove_market(&admin, &absent);
    let after = client.list_markets();

    assert_eq!(before, after);
    assert!(after.contains(&market));
}

// ── #1020: TotalCollected / CumulativeFees monotonicity ──────────────────────

#[test]
fn cumulative_counters_stay_monotone_across_collect_and_withdraw() {
    let env = Env::default();
    let (client, admin, market, treasury) = setup(&env);
    let token = create_token(&env);

    let first_fee = 1_000i128;
    let second_fee = 2_500i128;
    let withdraw_amount = 1_200i128;
    let total = first_fee + second_fee;

    // `collect_fee` only does bookkeeping (the market is assumed to have
    // already moved the real tokens in); seed the treasury's own token
    // balance so the later real `token::Client::transfer` in
    // `withdraw_fees` has something to send.
    mint(&env, &token, &treasury, total);

    client.collect_fee(&market, &token, &1u32, &first_fee);
    client.collect_fee(&market, &token, &1u32, &second_fee);

    assert_eq!(client.get_cumulative_fees(&token), total);
    assert_eq!(client.total_collected(), total);
    assert_eq!(client.token_balance(&token), total);

    let recipient = Address::generate(&env);
    client.withdraw_fees(&admin, &token, &recipient, &withdraw_amount);

    // CumulativeFees and TotalCollected are historical, monotone counters:
    // they must be unchanged by the withdrawal.
    assert_eq!(client.get_cumulative_fees(&token), total);
    assert_eq!(client.total_collected(), total);

    // TokenBalance is the decreasable custodied balance.
    let expected_remaining = total - withdraw_amount;
    assert_eq!(client.token_balance(&token), expected_remaining);
    assert_eq!(balance(&env, &token, &recipient), withdraw_amount);

    // The emitted event payloads match the stored counters.
    let (_fee_topics, fee_data) =
        find_event(&env, "fee_collected").expect("fee_collected event must be emitted");
    let fee_data = data_map(&env, fee_data);
    assert_eq!(data_i128(&env, &fee_data, "fee_amount"), second_fee);
    assert_eq!(data_i128(&env, &fee_data, "new_token_balance"), total);
    assert_eq!(data_i128(&env, &fee_data, "new_cumulative_fees"), total);

    let (_withdrawn_topics, withdrawn_data) =
        find_event(&env, "fees_withdrawn").expect("fees_withdrawn event must be emitted");
    let withdrawn_data = data_map(&env, withdrawn_data);
    assert_eq!(data_i128(&env, &withdrawn_data, "amount"), withdraw_amount);
    assert_eq!(
        data_i128(&env, &withdrawn_data, "remaining_token_balance"),
        expected_remaining
    );
}
