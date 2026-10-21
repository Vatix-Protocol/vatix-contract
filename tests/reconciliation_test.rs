use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, BytesN, Env, String,
};
use vatix_market_contract::{
    storage,
    types::{AdapterType, Market, MarketStatus},
    MarketContract, MarketContractClient,
};
const STROOPS: i128 = 10_000_000;

fn market_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(MarketContract, ());
    let admin = Address::generate(&env);
    MarketContractClient::new(&env, &contract).initialize(&admin);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    (env, admin, contract, token)
}

// initialize_market is not exposed by the current reduced contract ABI.
// Seed only market metadata; deposits, trades, token registration and
// withdrawals below all run through the production client entrypoints.
fn make_market(env: &Env, contract: &Address, admin: &Address, token: &Address) -> u32 {
    env.as_contract(contract, || {
        storage::set_market(
            env,
            1,
            &Market {
                id: 1,
                question: String::from_str(env, "Will it rain?"),
                end_time: 86400,
                oracle_pubkey: BytesN::from_array(env, &[1; 32]),
                status: MarketStatus::Active,
                result: None,
                creator: admin.clone(),
                created_at: 0,
                collateral_token: token.clone(),
                price_bps: 5000,
                resolver: None,
                resolved_at: None,
                adapter_type: AdapterType::Ed25519,
                outcome_count: 2,
                closed_to_deposits: false,
            },
        )
        .unwrap()
    });
    1
}

// A real second ledger is required: parity without an outcome-token contract
// is vacuously true and would not detect broken mint/burn wiring.
fn assert_registered_token_parity_after_withdraw(alter_balance: bool) {
    use soroban_sdk::testutils::Ledger;
    use vatix_outcome_token_contract::{
        types::TokenKind, OutcomeTokenContract, OutcomeTokenContractClient,
    };
    let (env, admin, contract_id, token) = market_env();
    let client = MarketContractClient::new(&env, &contract_id);
    let market_id = make_market(&env, &contract_id, &admin, &token);
    let outcome_id = env.register(OutcomeTokenContract, ());
    let outcome = OutcomeTokenContractClient::new(&env, &outcome_id);
    outcome.initialize(
        &admin,
        &contract_id,
        &String::from_str(&env, "Outcome"),
        &String::from_str(&env, "OUT"),
    );
    client.set_outcome_token_contract(&admin, &outcome_id);
    let user = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&user, &(100 * STROOPS));
    client.deposit_collateral(&user, &market_id, &(100 * STROOPS));
    client.update_position(
        &user,
        &market_id,
        &(50 * STROOPS),
        &(10 * STROOPS),
        &5_000i128,
    );
    env.ledger().set_timestamp(env.ledger().timestamp() + 3600);
    client.withdraw_unused_collateral(&user, &market_id, &(40 * STROOPS));
    if alter_balance {
        outcome.burn(&market_id, &user, &TokenKind::Yes, &STROOPS);
    }
    let parity = env.as_contract(&contract_id, || {
        vatix_market_contract::reconciliation::get_position_token_parity(&env, market_id, &user)
            .unwrap()
    });
    assert_eq!(parity.yes_shares, 50 * STROOPS);
    assert_eq!(parity.no_shares, 10 * STROOPS);
    assert_eq!(
        parity.yes_token_balance,
        (if alter_balance { 49 } else { 50 }) * STROOPS
    );
    assert_eq!(parity.no_token_balance, 10 * STROOPS);
    assert_eq!(parity.is_matched, !alter_balance);
    assert_eq!(
        env.as_contract(&contract_id, || storage::get_position(
            &env, market_id, &user
        )
        .unwrap()
        .unwrap()
        .total_deposited),
        60 * STROOPS
    );
}

#[test]
fn registered_outcome_token_matches_after_deposit_trade_withdraw() {
    assert_registered_token_parity_after_withdraw(false);
}

#[test]
fn registered_outcome_token_detects_altered_balance_after_withdraw() {
    assert_registered_token_parity_after_withdraw(true);
}
