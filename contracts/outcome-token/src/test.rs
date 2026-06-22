use crate::{ContractError, OutcomeTokenContract, OutcomeTokenContractClient};
use crate::types::TokenKind;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (OutcomeTokenContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(OutcomeTokenContract, ());
    let client = OutcomeTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let market_contract = Address::generate(env);
    client.initialize(&admin, &market_contract);
    (client, admin, market_contract)
}

// ── initialize ──────────────────────────────────────────────────────────────

#[test]
fn initialize_stores_config() {
    let env = Env::default();
    let (client, admin, market_contract) = setup(&env);
    let config = client.get_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.market_contract, market_contract);
}

#[test]
fn initialize_twice_is_rejected() {
    let env = Env::default();
    let (client, admin, market_contract) = setup(&env);
    assert_eq!(
        client.try_initialize(&admin, &market_contract),
        Err(Ok(ContractError::AlreadyInitialized))
    );
}

// ── mint ────────────────────────────────────────────────────────────────────

#[test]
fn mint_increases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    let user = Address::generate(&env);
    client.mint(&1, &user, &TokenKind::Yes, &500);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 500);
    assert_eq!(client.total_supply(&1, &TokenKind::Yes), 500);
    assert_eq!(client.balance(&1, &user, &TokenKind::No), 0);
}

#[test]
fn mint_accumulates_across_calls() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    let user = Address::generate(&env);
    client.mint(&1, &user, &TokenKind::No, &200);
    client.mint(&1, &user, &TokenKind::No, &300);

    assert_eq!(client.balance(&1, &user, &TokenKind::No), 500);
    assert_eq!(client.total_supply(&1, &TokenKind::No), 500);
}

#[test]
fn mint_zero_amount_is_rejected() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);
    assert_eq!(
        client.try_mint(&1, &user, &TokenKind::Yes, &0),
        Err(Ok(ContractError::InvalidAmount))
    );
}

#[test]
fn mint_yes_and_no_are_independent() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &100);
    client.mint(&1, &user, &TokenKind::No, &200);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 100);
    assert_eq!(client.balance(&1, &user, &TokenKind::No), 200);
    assert_eq!(client.total_supply(&1, &TokenKind::Yes), 100);
    assert_eq!(client.total_supply(&1, &TokenKind::No), 200);
}

// ── burn ────────────────────────────────────────────────────────────────────

#[test]
fn burn_decreases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &1000);
    client.burn(&1, &user, &TokenKind::Yes, &400);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 600);
    assert_eq!(client.total_supply(&1, &TokenKind::Yes), 600);
}

#[test]
fn burn_insufficient_balance_is_rejected() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::No, &100);
    assert_eq!(
        client.try_burn(&1, &user, &TokenKind::No, &101),
        Err(Ok(ContractError::InsufficientBalance))
    );
}

#[test]
fn burn_zero_amount_is_rejected() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);
    assert_eq!(
        client.try_burn(&1, &user, &TokenKind::Yes, &0),
        Err(Ok(ContractError::InvalidAmount))
    );
}

#[test]
fn burn_full_balance_brings_to_zero() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&2, &user, &TokenKind::Yes, &300);
    client.burn(&2, &user, &TokenKind::Yes, &300);

    assert_eq!(client.balance(&2, &user, &TokenKind::Yes), 0);
    assert_eq!(client.total_supply(&2, &TokenKind::Yes), 0);
}

// ── market isolation ────────────────────────────────────────────────────────

#[test]
fn balances_are_isolated_across_markets() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &100);
    client.mint(&2, &user, &TokenKind::Yes, &200);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 100);
    assert_eq!(client.balance(&2, &user, &TokenKind::Yes), 200);
    assert_eq!(client.total_supply(&1, &TokenKind::Yes), 100);
    assert_eq!(client.total_supply(&2, &TokenKind::Yes), 200);
}

// ── set_market_contract ─────────────────────────────────────────────────────

#[test]
fn admin_can_update_market_contract() {
    let env = Env::default();
    let (client, admin, _old_market) = setup(&env);
    let new_market = Address::generate(&env);

    client.set_market_contract(&admin, &new_market);
    assert_eq!(client.get_config().market_contract, new_market);
}

#[test]
fn non_admin_cannot_update_market_contract() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let stranger = Address::generate(&env);
    let new_market = Address::generate(&env);
    assert_eq!(
        client.try_set_market_contract(&stranger, &new_market),
        Err(Ok(ContractError::Unauthorized))
    );
}
