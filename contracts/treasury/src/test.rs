#![cfg(test)]

extern crate std;

use crate::{TreasuryContract, TreasuryContractClient, TreasuryError, ADDRESS_TIMELOCK_SECONDS};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, IntoVal, Map, Symbol, TryIntoVal, Val, Vec,
};

// ── helpers ───────────────────────────────────────────────────────────────────

struct Setup {
    env: Env,
    admin: Address,
    market: Address,
    token: Address,
    treasury_id: Address,
    client: TreasuryContractClient<'static>,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let treasury_id = env.register(TreasuryContract, ());
    let client: TreasuryContractClient<'static> =
        unsafe { core::mem::transmute(TreasuryContractClient::new(&env, &treasury_id)) };

    client.initialize(&admin, &market);

    Setup {
        env,
        admin,
        market,
        token,
        treasury_id,
        client,
    }
}

fn fund_treasury_via_fee(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
    s.client.collect_fee(&s.market, &s.token, &1u32, &amount);
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

// ── Issue #1019: initialize rejects a contract address as admin, and a second call ──

/// `initialize` must reject a deployed contract address as admin with
/// `InvalidAdmin`, before any state is written.
#[test]
fn initialize_rejects_contract_address_as_admin() {
    let env = Env::default();
    env.mock_all_auths();

    // Any deployed contract address works here — register a second,
    // independent treasury instance purely to obtain a contract Address.
    let contract_admin = env.register(TreasuryContract, ());
    let market = Address::generate(&env);

    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);

    let err = client
        .try_initialize(&contract_admin, &market)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAdmin);

    // No admin was stored by the rejected call — the contract is still
    // unversioned/uninitialized.
    assert!(client.try_admin().unwrap().is_err());
}

/// `initialize` with a normal user `Address` succeeds.
#[test]
fn initialize_succeeds_with_user_admin() {
    let s = setup();
    assert_eq!(s.client.admin(), s.admin);
    assert!(s.client.is_authorized_market(&s.market));
}

/// A second `initialize` call — with any admin, including a different one —
/// must be rejected with `AlreadyInitialized`, and must not overwrite the
/// existing admin.
#[test]
fn initialize_rejects_second_call() {
    let s = setup();
    let other_admin = Address::generate(&s.env);
    let other_market = Address::generate(&s.env);

    let err = s
        .client
        .try_initialize(&other_admin, &other_market)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::AlreadyInitialized);

    // Original admin/market registration is untouched.
    assert_eq!(s.client.admin(), s.admin);
    assert!(s.client.is_authorized_market(&s.market));
    assert!(!s.client.is_authorized_market(&other_market));
}

// ── Issue #1022: address timelock blocks execute_market_contract/execute_stakeholders ──

/// `execute_market_contract` called before `effective_at` must return
/// `TimelockNotElapsed`; after the delay it succeeds and the new market
/// becomes authorized.
#[test]
fn execute_market_contract_respects_timelock() {
    let s = setup();
    let new_market = Address::generate(&s.env);

    s.client.propose_market_contract(&s.admin, &new_market);

    let err = s.client.try_execute_market_contract().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::TimelockNotElapsed);
    assert!(!s.client.is_authorized_market(&new_market));

    s.env.ledger().with_mut(|l| {
        l.timestamp += ADDRESS_TIMELOCK_SECONDS + 1;
    });

    let executed = s.client.execute_market_contract();
    assert_eq!(executed, new_market);
    assert!(s.client.is_authorized_market(&new_market));
    // The original market stays registered — execute appends, not replaces.
    assert!(s.client.is_authorized_market(&s.market));
}

/// `execute_stakeholders` called before `effective_at` must return
/// `TimelockNotElapsed`; after the delay it succeeds and the pending list
/// becomes the active one.
#[test]
fn execute_stakeholders_respects_timelock() {
    let s = setup();
    let holder = Address::generate(&s.env);
    let stakeholders: Vec<(Address, u32)> =
        soroban_sdk::vec![&s.env, (holder.clone(), 10_000u32)];

    s.client.propose_stakeholders(&s.admin, &stakeholders);

    let err = s.client.try_execute_stakeholders().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::TimelockNotElapsed);
    // The rejected call did not install the pending list as active.
    assert_eq!(s.client.get_stakeholders().len(), 0);

    s.env.ledger().with_mut(|l| {
        l.timestamp += ADDRESS_TIMELOCK_SECONDS + 1;
    });

    s.client.execute_stakeholders();
    let active = s.client.get_stakeholders();
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap(), (holder, 10_000u32));
}

/// `execute_stakeholders` with nothing pending (fresh treasury) must return
/// `NoPendingStakeholderChange`.
#[test]
fn execute_stakeholders_with_no_pending_change_is_rejected() {
    let s = setup();
    let err = s.client.try_execute_stakeholders().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::NoPendingStakeholderChange);
}

// ── Issue #1018: distribute_fees / stakeholder share validation ──────────────

/// A stakeholder list whose shares sum to 9_999 (below `BPS_DENOMINATOR`) is
/// rejected at `propose_stakeholders` with `InvalidStakeholderWeights`, so it
/// can never become the active list `distribute_fees` would pay out against.
#[test]
fn propose_stakeholders_rejects_sum_below_10000() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let bad: Vec<(Address, u32)> =
        soroban_sdk::vec![&s.env, (a, 4_999u32), (b, 5_000u32)];

    let err = s
        .client
        .try_propose_stakeholders(&s.admin, &bad)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidStakeholderWeights);
    assert!(s.client.get_pending_stakeholders().is_none());
}

/// A stakeholder list whose shares sum to 10_001 (above `BPS_DENOMINATOR`) is
/// rejected at `propose_stakeholders` with `InvalidStakeholderWeights`.
#[test]
fn propose_stakeholders_rejects_sum_above_10000() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let bad: Vec<(Address, u32)> =
        soroban_sdk::vec![&s.env, (a, 5_001u32), (b, 5_000u32)];

    let err = s
        .client
        .try_propose_stakeholders(&s.admin, &bad)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidStakeholderWeights);
    assert!(s.client.get_pending_stakeholders().is_none());
}

/// An empty stakeholder list is rejected at `propose_stakeholders` with
/// `InvalidStakeholderWeights`.
#[test]
fn propose_stakeholders_rejects_empty_list() {
    let s = setup();
    let empty: Vec<(Address, u32)> = Vec::new(&s.env);

    let err = s
        .client
        .try_propose_stakeholders(&s.admin, &empty)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidStakeholderWeights);
}

/// `distribute_fees` on a freshly initialized treasury, before any
/// stakeholder list has ever been configured, returns
/// `NoStakeholdersConfigured`.
#[test]
fn distribute_fees_rejects_when_no_stakeholders_configured() {
    let s = setup();
    fund_treasury_via_fee(&s, 1_000_000i128);

    let err = s
        .client
        .try_distribute_fees(&s.admin, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NoStakeholdersConfigured);
}

/// A stakeholder list summing to exactly 10_000 is accepted, and once
/// executed `distribute_fees` succeeds and pays out proportionally.
#[test]
fn distribute_fees_succeeds_with_shares_summing_to_10000() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let good: Vec<(Address, u32)> =
        soroban_sdk::vec![&s.env, (a.clone(), 4_000u32), (b.clone(), 6_000u32)];

    s.client.propose_stakeholders(&s.admin, &good);
    s.env.ledger().with_mut(|l| {
        l.timestamp += ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_stakeholders();

    fund_treasury_via_fee(&s, 1_000_000i128);
    s.client.distribute_fees(&s.admin, &s.token);

    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&a),
        400_000i128
    );
    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&b),
        600_000i128
    );
    assert_eq!(s.client.token_balance(&s.token), 0);
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
}
