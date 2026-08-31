use crate::types::TokenKind;
use crate::{ContractError, OutcomeTokenContract, OutcomeTokenContractClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, String,
};

fn setup(env: &Env) -> (OutcomeTokenContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(OutcomeTokenContract, ());
    let client = OutcomeTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let market_contract = Address::generate(env);
    let name = String::from_str(env, "Vatix YES Token");
    let symbol = String::from_str(env, "vYES");
    client.initialize(&admin, &market_contract, &name, &symbol);
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
    assert_eq!(config.name, String::from_str(&env, "Vatix YES Token"));
    assert_eq!(config.symbol, String::from_str(&env, "vYES"));
}

#[test]
fn initialize_twice_is_rejected() {
    let env = Env::default();
    let (client, admin, market_contract) = setup(&env);
    let name = String::from_str(&env, "X");
    let symbol = String::from_str(&env, "X");
    assert_eq!(
        client.try_initialize(&admin, &market_contract, &name, &symbol),
        Err(Ok(ContractError::AlreadyInitialized))
    );
}

// ── SAC metadata (#382) ──────────────────────────────────────────────────────

#[test]
fn name_and_symbol_getters_return_stored_values() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.name(), String::from_str(&env, "Vatix YES Token"));
    assert_eq!(client.symbol(), String::from_str(&env, "vYES"));
}

#[test]
fn decimals_returns_seven() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.decimals(), 7u32);
}

#[test]
fn admin_can_update_metadata() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    let new_name = String::from_str(&env, "Vatix NO Token");
    let new_symbol = String::from_str(&env, "vNO");
    client.set_metadata(&admin, &new_name, &new_symbol);
    assert_eq!(client.name(), new_name);
    assert_eq!(client.symbol(), new_symbol);
}

#[test]
fn non_admin_cannot_update_metadata() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    let n = String::from_str(&env, "Bad");
    let s = String::from_str(&env, "BAD");
    assert_eq!(
        client.try_set_metadata(&stranger, &n, &s),
        Err(Ok(ContractError::Unauthorized))
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

// ── mint/burn authorization ─────────────────────────────────────────────────
//
// The tests above all run under `setup()`'s `env.mock_all_auths()`, which
// makes every `require_auth()` call succeed unconditionally — so they never
// actually exercise the `config.market_contract.require_auth()` gate inside
// `mint`/`burn`. The tests below build their own environment without blanket
// auth mocking so that gate is genuinely exercised: `mint`/`burn` must
// succeed when (and only when) the call carries a valid authorization for
// the registered market contract.

fn setup_unmocked(env: &Env) -> (OutcomeTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(OutcomeTokenContract, ());
    let client = OutcomeTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let market_contract = Address::generate(env);
    let name = String::from_str(env, "Vatix YES Token");
    let symbol = String::from_str(env, "vYES");

    // `initialize` only needs the admin's own auth, mocked for this one call.
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &market_contract, &name, &symbol).into_val(env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &market_contract, &name, &symbol);

    (client, admin, market_contract, contract_id)
}

#[test]
fn mint_succeeds_when_authorized_by_market_contract() {
    let env = Env::default();
    let (client, _admin, market_contract, contract_id) = setup_unmocked(&env);
    let user = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &market_contract,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint",
            args: (&1u32, &user, &TokenKind::Yes, &500i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint(&1, &user, &TokenKind::Yes, &500);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 500);
}

#[test]
#[should_panic]
fn mint_fails_without_market_contract_authorization() {
    // No auths are mocked at all here (unlike every other test in this file),
    // so `config.market_contract.require_auth()` inside `mint` has nothing to
    // satisfy it with — this is what stops an external EOA (or any caller
    // other than the registered market contract) from minting tokens.
    let env = Env::default();
    let (client, _admin, _market_contract, _contract_id) = setup_unmocked(&env);
    let user = Address::generate(&env);

    client.mint(&1, &user, &TokenKind::Yes, &500);
}

#[test]
fn burn_succeeds_when_authorized_by_market_contract() {
    let env = Env::default();
    let (client, _admin, market_contract, contract_id) = setup_unmocked(&env);
    let user = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &market_contract,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint",
            args: (&1u32, &user, &TokenKind::Yes, &500i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint(&1, &user, &TokenKind::Yes, &500);

    env.mock_auths(&[MockAuth {
        address: &market_contract,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "burn",
            args: (&1u32, &user, &TokenKind::Yes, &200i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.burn(&1, &user, &TokenKind::Yes, &200);

    assert_eq!(client.balance(&1, &user, &TokenKind::Yes), 300);
}

#[test]
#[should_panic]
fn burn_fails_without_market_contract_authorization() {
    let env = Env::default();
    let (client, _admin, market_contract, contract_id) = setup_unmocked(&env);
    let user = Address::generate(&env);

    // Fund the user first (this one mint call is authorized) so the burn
    // attempt below fails on the auth gate itself, not InsufficientBalance.
    env.mock_auths(&[MockAuth {
        address: &market_contract,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint",
            args: (&1u32, &user, &TokenKind::Yes, &500i128).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint(&1, &user, &TokenKind::Yes, &500);

    // No mocked auth for this call — an unauthorized burn must panic.
    client.burn(&1, &user, &TokenKind::Yes, &200);
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
