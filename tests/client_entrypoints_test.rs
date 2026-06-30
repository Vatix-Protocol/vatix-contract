//! #353: Integration tests covering every public entrypoint across all 4 contracts.
//! Each test exercises the client API through the generated `*ContractClient`.

#[allow(dead_code)]
mod helpers;

use helpers::{oracle_keypair, register_contract, sign_outcome, MarketParams};
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env, String,
};
use vatix_market_contract::{storage, MarketContract, MarketContractClient};
use vatix_resolution_contract::{ResolutionContract, ResolutionContractClient};
use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

const STROOPS: i128 = 10_000_000;

// ── helpers ───────────────────────────────────────────────────────────────────

fn market_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id) = register_contract(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    (env, admin, contract_id, token)
}

fn make_market(
    client: &MarketContractClient,
    env: &Env,
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
    )
}

// ── Market: initialize ────────────────────────────────────────────────────────

#[test]
fn market_initialize_sets_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin).unwrap();

    env.as_contract(&contract_id, || {
        assert_eq!(storage::get_admin(&env).unwrap(), admin);
    });
}

#[test]
fn market_initialize_twice_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin).unwrap();
    assert!(client.try_initialize(&admin).is_err());
}

// ── Market: propose_admin / accept_admin ──────────────────────────────────────

#[test]
fn market_two_step_admin_transfer() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin).unwrap();
    client.accept_admin(&new_admin).unwrap();

    env.as_contract(&contract_id, || {
        assert_eq!(storage::get_admin(&env).unwrap(), new_admin);
    });
}

// ── Market: initialize_market ─────────────────────────────────────────────────

#[test]
fn market_initialize_market_returns_sequential_ids() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);

    let id1 = make_market(&client, &env, &admin, &token);
    let id2 = make_market(&client, &env, &admin, &token);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

// ── Market: deposit_collateral ────────────────────────────────────────────────

#[test]
fn market_deposit_collateral_records_total_deposited() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS));

    let pos = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert_eq!(pos.total_deposited, 100 * STROOPS);
    assert_eq!(pos.locked_collateral, 0);
}

// ── Market: withdraw_unused_collateral ────────────────────────────────────────

#[test]
fn market_withdraw_unused_collateral_decrements_total() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS));
    client.withdraw_unused_collateral(&user, &market_id, &(40 * STROOPS));

    let pos = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert_eq!(pos.total_deposited, 60 * STROOPS);
    assert_eq!(TokenClient::new(&env, &token).balance(&user), 40 * STROOPS);
}

// ── Market: update_position ───────────────────────────────────────────────────

#[test]
fn market_update_position_sets_shares_and_locks() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS));
    client.update_position(&user, &market_id, &(50 * STROOPS), &0i128, &5_000i128);

    let pos = env.as_contract(&contract_id, || {
        storage::get_position(&env, market_id, &user)
            .unwrap()
            .expect("position must exist")
    });
    assert_eq!(pos.yes_shares, 50 * STROOPS);
    assert_eq!(pos.locked_collateral, 25 * STROOPS); // 50% of 50 * STROOPS
}

// ── Market: cancel_market / withdraw_canceled_collateral ─────────────────────

#[test]
fn market_cancel_and_reclaim_collateral() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(50 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(50 * STROOPS));

    client.cancel_market(&admin, &market_id);
    let refund = client.withdraw_canceled_collateral(&user, &market_id);
    assert_eq!(refund, 50 * STROOPS);
    assert_eq!(TokenClient::new(&env, &token).balance(&user), 50 * STROOPS);
}

// ── Market: resolve_market ────────────────────────────────────────────────────

#[test]
fn market_resolve_market_sets_resolved_status() {
    use vatix_market_contract::types::MarketStatus;

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let (oracle_pubkey, signing_key) = oracle_keypair(&env);
    let token = Address::generate(&env);
    let market_id = client.initialize_market(
        &admin,
        &String::from_str(&env, "Resolved?"),
        &(env.ledger().timestamp() + 86_400),
        &oracle_pubkey,
        &token,
    );

    let sig = sign_outcome(&env, &signing_key, market_id, true);
    client.resolve_market(&String::from_str(&env, "1"), &true, &sig);

    let market = client.get_market(&market_id).unwrap();
    assert_eq!(market.status, MarketStatus::Resolved);
    assert_eq!(market.result, Some(true));
}

// ── Market: settle_position ───────────────────────────────────────────────────

#[test]
fn market_settle_position_pays_out_winner() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let (oracle_pubkey, signing_key) = oracle_keypair(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let market_id = client.initialize_market(
        &admin,
        &String::from_str(&env, "Win?"),
        &(env.ledger().timestamp() + 86_400),
        &oracle_pubkey,
        &token,
    );

    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS));
    client.update_position(&user, &market_id, &(100 * STROOPS), &0i128, &5_000i128);

    let sig = sign_outcome(&env, &signing_key, market_id, true);
    client.resolve_market(&String::from_str(&env, "1"), &true, &sig);

    let payout = client.settle_position(&user, &market_id);
    assert_eq!(payout, 100 * STROOPS);
    assert_eq!(TokenClient::new(&env, &token).balance(&user), 100 * STROOPS);
}

// ── Market: batch_settle_positions ────────────────────────────────────────────

#[test]
fn market_batch_settle_positions_settles_multiple_users() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MarketContract, ());
    let client = MarketContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        storage::set_version(&env);
        storage::set_admin(&env, &admin);
    });

    let (oracle_pubkey, signing_key) = oracle_keypair(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = StellarAssetClient::new(&env, &token);

    let market_id = client.initialize_market(
        &admin,
        &String::from_str(&env, "Batch?"),
        &(env.ledger().timestamp() + 86_400),
        &oracle_pubkey,
        &token,
    );

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    sac.mint(&user1, &(50 * STROOPS));
    sac.mint(&user2, &(50 * STROOPS));
    client.deposit_collateral(&user1, &market_id, &(50 * STROOPS));
    client.deposit_collateral(&user2, &market_id, &(50 * STROOPS));
    client.update_position(&user1, &market_id, &(50 * STROOPS), &0i128, &5_000i128);
    client.update_position(&user2, &market_id, &(50 * STROOPS), &0i128, &5_000i128);

    let sig = sign_outcome(&env, &signing_key, market_id, true);
    client.resolve_market(&String::from_str(&env, "1"), &true, &sig);

    let users = soroban_sdk::vec![&env, user1.clone(), user2.clone()];
    let total = client.batch_settle_positions(&market_id, &users);
    assert_eq!(total, 100 * STROOPS);
}

// ── Market: set_treasury / get_treasury ───────────────────────────────────────

#[test]
fn market_set_and_get_treasury() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let treasury = Address::generate(&env);

    client.set_treasury(&admin, &treasury).unwrap();
    assert_eq!(client.get_treasury(), Some(treasury));
}

// ── Market: set_fee_rate / get_fee_rate ───────────────────────────────────────

#[test]
fn market_set_and_get_fee_rate() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);

    client.set_fee_rate(&admin, &250i128).unwrap();
    assert_eq!(client.get_fee_rate(), 250i128);
}

// ── Market: set_outcome_token_contract / get_outcome_token_contract ───────────

#[test]
fn market_set_and_get_outcome_token_contract() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let ot = Address::generate(&env);

    client.set_outcome_token_contract(&admin, &ot).unwrap();
    assert_eq!(client.get_outcome_token_contract(), Some(ot));
}

// ── Market: set_resolution_contract / get_resolution_contract ────────────────

#[test]
fn market_set_and_get_resolution_contract() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let res = Address::generate(&env);

    client.set_resolution_contract(&admin, &res).unwrap();
    assert_eq!(client.get_resolution_contract(), Some(res));
}

// ── Market: set_threshold_signers / get_threshold_signers/quorum ──────────────

#[test]
fn market_set_and_get_threshold_signers() {
    let (env, admin, contract_id, _token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);

    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let signers = soroban_sdk::vec![&env, signer.clone()];
    client.set_threshold_signers(&admin, &signers, &1u32).unwrap();

    assert_eq!(client.get_threshold_quorum(), 1u32);
    assert_eq!(client.get_threshold_signers().get(0).unwrap(), signer);
}

// ── Market: get_market / get_outcome_count ────────────────────────────────────

#[test]
fn market_get_market_returns_stored_market() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    let market = client.get_market(&market_id).unwrap();
    assert_eq!(market.id, market_id);
    assert_eq!(market.outcome_count, 2);
}

#[test]
fn market_get_outcome_count_returns_two() {
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&client, &env, &admin, &token);

    assert_eq!(client.get_outcome_count(&market_id).unwrap(), 2u32);
}

// ── Treasury: initialize ──────────────────────────────────────────────────────

#[test]
fn treasury_initialize_stores_admin_and_market() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    client.initialize(&admin, &market);
    assert_eq!(client.admin(), admin);
    assert_eq!(client.market_contract(), market);
}

// ── Treasury: collect_fee ─────────────────────────────────────────────────────

#[test]
fn treasury_collect_fee_accumulates_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &market);

    client.collect_fee(&market, &token, &1u32, &1_000i128);
    client.collect_fee(&market, &token, &2u32, &500i128);

    assert_eq!(client.token_balance(&token), 1_500);
    assert_eq!(client.total_collected(), 1_500);
}

// ── Treasury: withdraw_fees ───────────────────────────────────────────────────

#[test]
fn treasury_withdraw_fees_transfers_to_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let tid = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &tid);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    client.initialize(&admin, &market);

    StellarAssetClient::new(&env, &token).mint(&tid, &2_000i128);
    client.collect_fee(&market, &token, &1u32, &2_000i128);

    let recipient = Address::generate(&env);
    client.withdraw_fees(&admin, &token, &recipient, &1_000i128);

    assert_eq!(TokenClient::new(&env, &token).balance(&recipient), 1_000);
    assert_eq!(client.token_balance(&token), 1_000);
    assert_eq!(client.get_cumulative_fees(&token), 2_000); // monotone
}

// ── Treasury: set_market_contract / transfer_admin ───────────────────────────

#[test]
fn treasury_set_market_contract_rotates_authorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let tid = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &tid);
    let admin = Address::generate(&env);
    let old_market = Address::generate(&env);
    let new_market = Address::generate(&env);
    let token = Address::generate(&env);
    client.initialize(&admin, &old_market);

    client.set_market_contract(&admin, &new_market);
    assert!(client.try_collect_fee(&old_market, &token, &1u32, &100i128).is_err());
    client.collect_fee(&new_market, &token, &1u32, &100i128);
    assert_eq!(client.total_collected(), 100);
}

#[test]
fn treasury_transfer_admin_changes_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let tid = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &tid);
    let admin = Address::generate(&env);
    let market = Address::generate(&env);
    client.initialize(&admin, &market);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&admin, &new_admin);
    assert_eq!(client.admin(), new_admin);
}

// ── Resolution: initialize ────────────────────────────────────────────────────

#[test]
fn resolution_initialize_stores_config() {
    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    let factory = Address::generate(&env);
    let market_contract = Address::generate(&env);

    client.initialize(&admin, &factory, &market_contract).unwrap();
    let config = client.get_config();
    assert_eq!(config.factory, factory);
    assert_eq!(config.market_contract, market_contract);
}

// ── Resolution: set_factory / set_market_contract ────────────────────────────

#[test]
fn resolution_set_factory_updates_config() {
    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    client.initialize(&admin, &Address::generate(&env), &Address::generate(&env)).unwrap();

    let new_factory = Address::generate(&env);
    client.set_factory(&admin, &new_factory).unwrap();
    assert_eq!(client.get_config().factory, new_factory);
}

#[test]
fn resolution_set_market_contract_updates_config() {
    let env = Env::default();
    env.mock_all_auths();
    let rid = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(&env, &rid);
    let admin = Address::generate(&env);
    client.initialize(&admin, &Address::generate(&env), &Address::generate(&env)).unwrap();

    let new_market = Address::generate(&env);
    client.set_market_contract(&admin, &new_market).unwrap();
    assert_eq!(client.get_config().market_contract, new_market);
}

use vatix_outcome_token_contract::{OutcomeTokenContract, OutcomeTokenContractClient, types::TokenKind};

fn ot_setup(env: &Env) -> (OutcomeTokenContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let cid = env.register(OutcomeTokenContract, ());
    let client = OutcomeTokenContractClient::new(env, &cid);
    let admin = Address::generate(env);
    let market = Address::generate(env);
    client.initialize(&admin, &market).unwrap();
    (client, admin, market)
}

// ── OutcomeToken: initialize ──────────────────────────────────────────────────

#[test]
fn outcome_token_initialize_stores_config() {
    let env = Env::default();
    let (client, admin, market) = ot_setup(&env);
    let cfg = client.get_config();
    assert_eq!(cfg.admin, admin);
    assert_eq!(cfg.market_contract, market);
}

#[test]
fn outcome_token_initialize_twice_is_rejected() {
    let env = Env::default();
    let (client, admin, market) = ot_setup(&env);
    assert!(client.try_initialize(&admin, &market).is_err());
}

// ── OutcomeToken: mint ────────────────────────────────────────────────────────

#[test]
fn outcome_token_mint_increases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &500);
    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 500);
    assert_eq!(client.total_supply(&1, &TokenKind::Yes), 500);
}

#[test]
fn outcome_token_mint_zero_is_rejected() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let user = Address::generate(&env);
    assert!(client.try_mint(&1, &user, &TokenKind::Yes, &0).is_err());
}

// ── OutcomeToken: burn ────────────────────────────────────────────────────────

#[test]
fn outcome_token_burn_decreases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::No, &1_000);
    client.burn(&1, &user, &TokenKind::No, &400);
    assert_eq!(client.balance(&1, &user, &TokenKind::No), 600);
    assert_eq!(client.total_supply(&1, &TokenKind::No), 600);
}

#[test]
fn outcome_token_burn_exceeding_balance_is_rejected() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let user = Address::generate(&env);
    client.mint(&1, &user, &TokenKind::Yes, &100);
    assert!(client.try_burn(&1, &user, &TokenKind::Yes, &101).is_err());
}

// ── OutcomeToken: set_market_contract ────────────────────────────────────────

#[test]
fn outcome_token_set_market_contract_updates_config() {
    let env = Env::default();
    let (client, admin, _market) = ot_setup(&env);
    let new_market = Address::generate(&env);
    client.set_market_contract(&admin, &new_market).unwrap();
    assert_eq!(client.get_config().market_contract, new_market);
}

#[test]
fn outcome_token_non_admin_cannot_set_market_contract() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let stranger = Address::generate(&env);
    let new_market = Address::generate(&env);
    assert!(client.try_set_market_contract(&stranger, &new_market).is_err());
}

// ── OutcomeToken: decimals ────────────────────────────────────────────────────

#[test]
fn outcome_token_decimals_returns_seven() {
    let env = Env::default();
    let (client, _, _) = ot_setup(&env);
    assert_eq!(client.decimals(), 7u32);
}

// ── OutcomeToken: balance / total_supply isolation ───────────────────────────

#[test]
fn outcome_token_balances_isolated_across_markets() {
    let env = Env::default();
    let (client, _admin, _market) = ot_setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &100);
    client.mint(&2, &user, &TokenKind::Yes, &200);
    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 100);
    assert_eq!(client.balance(&2, &user, &TokenKind::Yes), 200);
}
