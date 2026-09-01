//! #352: Snapshot tests for event JSON payloads.
//!
//! Each test triggers exactly one event and asserts the complete shape of the
//! emitted event: topic count, topic values, and every data field. This acts
//! as a breaking-change guard for the event catalog documented in README.md.

#[allow(dead_code)]
mod helpers;

use helpers::{oracle_keypair, register_contract, sign_outcome, MarketParams};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    Address, BytesN, Env, IntoVal, Map, String, Symbol, TryIntoVal, Val,
};
use vatix_market_contract::{MarketContract, MarketContractClient};
use vatix_resolution_contract::{ResolutionContract, ResolutionContractClient};
use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

// ── helpers ───────────────────────────────────────────────────────────────────

fn last_event(env: &Env) -> (std::vec::Vec<Val>, Val) {
    let all = env.events().all();
    assert!(!all.is_empty(), "no events emitted");
    let ev = all.last().unwrap();
    (ev.1.iter().collect(), ev.2)
}

fn topic_sym(env: &Env, topics: &[Val], idx: usize) -> Symbol {
    topics[idx].clone().into_val(env)
}

fn data_map(env: &Env, data: Val) -> Map<Symbol, Val> {
    data.try_into_val(env).expect("data must be a Map<Symbol,Val>")
}

fn data_u64(env: &Env, m: &Map<Symbol, Val>, key: &str) -> u64 {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

fn data_i128(env: &Env, m: &Map<Symbol, Val>, key: &str) -> i128 {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

fn data_bool(env: &Env, m: &Map<Symbol, Val>, key: &str) -> bool {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

fn data_addr(env: &Env, m: &Map<Symbol, Val>, key: &str) -> Address {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

fn data_u32(env: &Env, m: &Map<Symbol, Val>, key: &str) -> u32 {
    m.get(Symbol::new(env, key)).unwrap().into_val(env)
}

// ── Market: contract_initialized_event ───────────────────────────────────────

#[test]
fn event_contract_initialized_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &cid);
    let admin = Address::generate(&env);

    client.initialize(&admin).unwrap();

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2, "contract_initialized_event has 2 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "contract_initialized_event"));
    let topic_admin: Address = topics[1].clone().into_val(&env);
    assert_eq!(topic_admin, admin);

    let m = data_map(&env, data);
    let _ts: u64 = data_u64(&env, &m, "initialized_at");
}

// ── Market: market_created_event ──────────────────────────────────────────────

#[test]
fn event_market_created_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let token = Address::generate(&env);
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token;

    client.initialize_market(
        &admin,
        &params.question,
        &params.end_time,
        &params.oracle_pubkey,
        &params.collateral_token,
    );

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2, "market_created_event has 2 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "market_created_event"));
    let topic_id: u32 = topics[1].clone().into_val(&env);
    assert_eq!(topic_id, 1u32);

    let m = data_map(&env, data);
    let creator: Address = data_addr(&env, &m, "creator");
    assert_eq!(creator, admin);
    let _q: String = m.get(Symbol::new(&env, "question")).unwrap().into_val(&env);
    let _et: u64 = data_u64(&env, &m, "end_time");
}

// ── Market: collateral_deposited_event ────────────────────────────────────────

#[test]
fn event_collateral_deposited_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let mid = client.initialize_market(
        &admin, &params.question, &params.end_time, &params.oracle_pubkey, &params.collateral_token,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &1_000i128);
    client.deposit_collateral(&user, &mid, &1_000i128);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "collateral_deposited_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "collateral_deposited_event"));
    let topic_user: Address = topics[1].clone().into_val(&env);
    assert_eq!(topic_user, user);
    let topic_mid: u32 = topics[2].clone().into_val(&env);
    assert_eq!(topic_mid, mid);

    let m = data_map(&env, data);
    assert_eq!(data_i128(&env, &m, "amount"), 1_000);
    assert_eq!(data_i128(&env, &m, "new_total"), 1_000);
}

// ── Market: collateral_withdrawn_event ────────────────────────────────────────

#[test]
fn event_collateral_withdrawn_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let mid = client.initialize_market(
        &admin, &params.question, &params.end_time, &params.oracle_pubkey, &params.collateral_token,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &2_000i128);
    client.deposit_collateral(&user, &mid, &2_000i128);
    client.withdraw_unused_collateral(&user, &mid, &500i128);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "collateral_withdrawn_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "collateral_withdrawn_event"));

    let m = data_map(&env, data);
    assert_eq!(data_i128(&env, &m, "amount"), 500);
    assert_eq!(data_i128(&env, &m, "new_total"), 1_500);
}

// ── Market: position_updated_event ───────────────────────────────────────────

#[test]
fn event_position_updated_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    let mut params = MarketParams::default_valid(&env);
    params.collateral_token = token.clone();
    let mid = client.initialize_market(
        &admin, &params.question, &params.end_time, &params.oracle_pubkey, &params.collateral_token,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &10_000i128);
    client.deposit_collateral(&user, &mid, &10_000i128);
    client.update_position(&user, &mid, &5_000i128, &0i128, &5_000i128);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "position_updated_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "position_updated_event"));

    let m = data_map(&env, data);
    assert_eq!(data_i128(&env, &m, "yes_shares"), 5_000);
    assert_eq!(data_i128(&env, &m, "no_shares"), 0);
    // locked = yes_shares * price_bps / 10_000 = 5_000 * 5_000 / 10_000 = 2_500
    assert_eq!(data_i128(&env, &m, "locked_collateral"), 2_500);
}

// ── Market: market_resolved_event ────────────────────────────────────────────

#[test]
fn event_market_resolved_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let (oracle_pubkey, signing_key) = oracle_keypair(&env);
    let token = Address::generate(&env);

    let mid = client.initialize_market(
        &admin,
        &String::from_str(&env, "resolved?"),
        &(env.ledger().timestamp() + 86_400),
        &oracle_pubkey,
        &token,
    );
    let sig = sign_outcome(&env, &signing_key, mid, true);
    client.resolve_market(&String::from_str(&env, "1"), &true, &sig);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2, "market_resolved_event has 2 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "market_resolved_event"));
    let topic_mid: u32 = topics[1].clone().into_val(&env);
    assert_eq!(topic_mid, mid);

    let m = data_map(&env, data);
    assert_eq!(data_bool(&env, &m, "outcome"), true);
    let _resolved_at: u64 = data_u64(&env, &m, "resolved_at");
}

// ── Market: position_settled_event ────────────────────────────────────────────

#[test]
fn event_position_settled_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, cid) = register_contract(&env);
    let client = MarketContractClient::new(&env, &cid);
    let (oracle_pubkey, signing_key) = oracle_keypair(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();

    let mid = client.initialize_market(
        &admin,
        &String::from_str(&env, "settle?"),
        &(env.ledger().timestamp() + 86_400),
        &oracle_pubkey,
        &token,
    );
    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &10_000i128);
    client.deposit_collateral(&user, &mid, &10_000i128);
    client.update_position(&user, &mid, &10_000i128, &0i128, &5_000i128);

    let sig = sign_outcome(&env, &signing_key, mid, true);
    client.resolve_market(&String::from_str(&env, "1"), &true, &sig);
    client.settle_position(&user, &mid);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "position_settled_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "position_settled_event"));

    let m = data_map(&env, data);
    assert_eq!(data_i128(&env, &m, "payout"), 10_000);
    let _settled_at: u64 = data_u64(&env, &m, "settled_at");
}

// ── Treasury: fee_collected_event ────────────────────────────────────────────

#[test]
fn event_fee_collected_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let tid = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &tid);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &market);

    client.collect_fee(&market, &token, &5u32, &2_000i128);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "fee_collected_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "fee_collected_event"));
    let topic_mid: u32 = topics[1].clone().into_val(&env);
    assert_eq!(topic_mid, 5u32);

    let m = data_map(&env, data);
    assert_eq!(data_i128(&env, &m, "fee_amount"), 2_000);
    assert_eq!(data_i128(&env, &m, "new_token_balance"), 2_000);
    assert_eq!(data_i128(&env, &m, "new_cumulative_fees"), 2_000);
}

// ── Treasury: treasury_initialized_event ─────────────────────────────────────

#[test]
fn event_treasury_initialized_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let tid = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &tid);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    client.initialize(&admin, &market);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "treasury_initialized_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "treasury_initialized_event"));
    let topic_admin: Address = topics[1].clone().into_val(&env);
    assert_eq!(topic_admin, admin);

    let m = data_map(&env, data);
    let _ts: u64 = data_u64(&env, &m, "initialized_at");
}

// ── Resolution: candidate_proposed_event ─────────────────────────────────────

#[test]
fn event_candidate_proposed_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    let factory = Address::generate(&env);
    // market_contract here can be any address; mock_all_auths skips auth checks
    let market_contract = Address::generate(&env);
    client.initialize(&admin, &factory, &market_contract).unwrap();

    let proposer = Address::generate(&env);
    let sig = BytesN::from_array(&env, &[0xABu8; 64]);
    let uri = String::from_str(&env, "ipfs://evidence");
    let expiry = env.ledger().timestamp() + 3600;
    client.propose(&proposer, &1u32, &true, &sig, &expiry, &uri, &300u64).unwrap();

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "candidate_proposed_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "candidate_proposed_event"));
    let topic_cid: u32 = topics[1].clone().into_val(&env);
    assert_eq!(topic_cid, 1u32);
    let topic_mid: u32 = topics[2].clone().into_val(&env);
    assert_eq!(topic_mid, 1u32);

    let m = data_map(&env, data);
    assert_eq!(data_bool(&env, &m, "outcome"), true);
    let _deadline: u64 = data_u64(&env, &m, "challenge_deadline");
}

// ── Resolution: candidate_challenged_event ───────────────────────────────────

#[test]
fn event_candidate_challenged_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    client.initialize(&admin, &Address::generate(&env), &Address::generate(&env)).unwrap();

    let proposer = Address::generate(&env);
    let sig = BytesN::from_array(&env, &[0xABu8; 64]);
    let uri = String::from_str(&env, "ipfs://evidence");
    let expiry = env.ledger().timestamp() + 600;
    let cid = client.propose(&proposer, &2u32, &false, &sig, &expiry, &uri, &300u64).unwrap();

    let challenger = Address::generate(&env);
    let challenge_uri = String::from_str(&env, "ipfs://challenge");
    client.challenge(&challenger, &cid, &challenge_uri).unwrap();

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "candidate_challenged_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "candidate_challenged_event"));

    let m = data_map(&env, data);
    let ev_challenger: Address = data_addr(&env, &m, "challenger");
    assert_eq!(ev_challenger, challenger);
    let _challenged_at: u64 = data_u64(&env, &m, "challenged_at");
}

// ── Resolution: candidate_finalized_event ────────────────────────────────────

#[test]
fn event_candidate_finalized_shape() {
    use soroban_sdk::testutils::Ledger;

    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    client.initialize(&admin, &Address::generate(&env), &Address::generate(&env)).unwrap();

    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let proposer = Address::generate(&env);
    let sig = BytesN::from_array(&env, &[0xABu8; 64]);
    let uri = String::from_str(&env, "ipfs://evidence");
    let cid = client
        .propose(&proposer, &3u32, &true, &sig, &2_000u64, &uri, &60u64)
        .unwrap();

    env.ledger().with_mut(|l| l.timestamp = 1_061);
    let finalizer = Address::generate(&env);
    client.finalize(&finalizer, &cid);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "candidate_finalized_event has 3 topics");
    assert_eq!(topic_sym(&env, &topics, 0), Symbol::new(&env, "candidate_finalized_event"));

    let m = data_map(&env, data);
    assert_eq!(data_bool(&env, &m, "outcome"), true);
    let finalized_at: u64 = data_u64(&env, &m, "finalized_at");
    assert_eq!(finalized_at, 1_061);
    let ev_mid: u32 = data_u32(&env, &m, "market_id");
    assert_eq!(ev_mid, 3u32);
}
