#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String};

use crate::error::ContractError;
use crate::types::TokenKind;
use crate::{OutcomeTokenContract, OutcomeTokenContractClient};

fn setup<'a>(env: &'a Env) -> (OutcomeTokenContractClient<'a>, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let market_contract = Address::generate(env);
    let contract_id = env.register(OutcomeTokenContract, ());
    let client = OutcomeTokenContractClient::new(env, &contract_id);
    client.initialize(
        &admin,
        &market_contract,
        &String::from_str(env, "Vatix Outcome Token"),
        &String::from_str(env, "vOUT"),
    );
    (client, admin, market_contract)
}

#[test]
fn metadata_defaults_are_stable() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    assert_eq!(client.name(), String::from_str(&env, "Vatix Outcome Token"));
    assert_eq!(client.symbol(), String::from_str(&env, "vOUT"));
    assert_eq!(client.decimals(), 7);
}

#[test]
fn admin_can_update_metadata() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    client.set_metadata(
        &admin,
        &String::from_str(&env, "Vatix Outcome YES"),
        &String::from_str(&env, "vYES"),
    );

    assert_eq!(client.name(), String::from_str(&env, "Vatix Outcome YES"));
    assert_eq!(client.symbol(), String::from_str(&env, "vYES"));
    // Decimals is a compile-time constant; it cannot be changed by
    // `set_metadata` and always reports 7 (Issue #929).
    assert_eq!(client.decimals(), 7);
}

#[test]
fn metadata_update_is_idempotent() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    let name = String::from_str(&env, "Vatix Outcome YES");
    let symbol = String::from_str(&env, "vYES");

    client.set_metadata(&admin, &name, &symbol);
    client.set_metadata(&admin, &name, &symbol);

    assert_eq!(client.name(), name);
    assert_eq!(client.symbol(), symbol);
}

#[test]
fn non_admin_cannot_update_metadata() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    // Drop the blanket auth mock so the missing admin auth is enforced.
    env.mock_auths(&[]);

    let attacker = Address::generate(&env);
    let result = client.try_set_metadata(
        &attacker,
        &String::from_str(&env, "Hijacked"),
        &String::from_str(&env, "HJK"),
    );
    assert_eq!(result, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn wrong_admin_cannot_update_metadata() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    // Only a different, non-admin address authorizes the call; the stored
    // admin check must still reject it (deny-by-default).
    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_metadata",
            args: (
                attacker.clone(),
                String::from_str(&env, "Hijacked"),
                String::from_str(&env, "HJK"),
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_metadata(
        &attacker,
        &String::from_str(&env, "Hijacked"),
        &String::from_str(&env, "HJK"),
    );
    assert_eq!(result, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn metadata_rejects_empty_name() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, ""),
        &String::from_str(&env, "vYES"),
    );
    assert_eq!(result, Err(Ok(ContractError::EmptyMetadata)));
}

#[test]
fn metadata_rejects_empty_symbol() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "Vatix Outcome YES"),
        &String::from_str(&env, ""),
    );
    assert_eq!(result, Err(Ok(ContractError::EmptyMetadata)));
}

#[test]
fn metadata_accepts_name_at_cap() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    // Exactly at the maximum allowed name length must be accepted.
    let name = String::from_str(&env, &"N".repeat(crate::MAX_NAME_LEN as usize));
    let symbol = String::from_str(&env, "vYES");

    client.set_metadata(&admin, &name, &symbol);

    assert_eq!(client.name(), name);
    assert_eq!(client.symbol(), symbol);
}

#[test]
fn metadata_accepts_symbol_at_cap() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    // Exactly at the maximum allowed symbol length must be accepted.
    let name = String::from_str(&env, "Vatix Outcome YES");
    let symbol = String::from_str(&env, &"S".repeat(crate::MAX_SYMBOL_LEN as usize));

    client.set_metadata(&admin, &name, &symbol);

    assert_eq!(client.name(), name);
    assert_eq!(client.symbol(), symbol);
}

#[test]
fn metadata_rejects_name_over_cap() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    // One byte over the cap must fail closed with a stable error code.
    let name = String::from_str(&env, &"N".repeat(crate::MAX_NAME_LEN as usize + 1));
    let result = client.try_set_metadata(
        &admin,
        &name,
        &String::from_str(&env, "vYES"),
    );
    assert_eq!(result, Err(Ok(ContractError::MetadataTooLong)));
}

#[test]
fn metadata_rejects_symbol_over_cap() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    // One byte over the cap must fail closed with a stable error code.
    let symbol = String::from_str(&env, &"S".repeat(crate::MAX_SYMBOL_LEN as usize + 1));
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "Vatix Outcome YES"),
        &symbol,
    );
    assert_eq!(result, Err(Ok(ContractError::MetadataTooLong)));
}

#[test]
fn metadata_over_cap_does_not_mutate_state() {
    let env = Env::default();
    let (client, admin, _market) = setup(&env);

    let original_name = client.name();
    let original_symbol = client.symbol();

    let name = String::from_str(&env, &"N".repeat(crate::MAX_NAME_LEN as usize + 1));
    let result = client.try_set_metadata(
        &admin,
        &name,
        &String::from_str(&env, "vYES"),
    );
    assert_eq!(result, Err(Ok(ContractError::MetadataTooLong)));

    // Fail-closed: rejected updates must not partially apply.
    assert_eq!(client.name(), original_name);
    assert_eq!(client.symbol(), original_symbol);
}

#[test]
fn non_admin_cannot_bypass_length_caps() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);

    // Drop the blanket auth mock so the missing admin auth is enforced.
    env.mock_auths(&[]);

    let attacker = Address::generate(&env);
    let name = String::from_str(&env, &"N".repeat(crate::MAX_NAME_LEN as usize + 1));
    let result = client.try_set_metadata(
        &attacker,
        &name,
        &String::from_str(&env, "vYES"),
    );
    // Authz is checked before length validation: untrusted callers are
    // rejected outright and cannot probe or bypass the caps.
    assert_eq!(result, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn mint_increases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1u32, &user, &TokenKind::Yes, &100i128);

    assert_eq!(client.balance(&1u32, &user, &TokenKind::Yes), 100i128);
    assert_eq!(client.total_supply(&1u32, &TokenKind::Yes), 100i128);
    let _ = market;
}

#[test]
fn burn_decreases_balance_and_supply() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&1u32, &user, &TokenKind::Yes, &100i128);
    client.burn(&1u32, &user, &TokenKind::Yes, &40i128);

    assert_eq!(client.balance(&1u32, &user, &TokenKind::Yes), 60i128);
    assert_eq!(client.total_supply(&1u32, &TokenKind::Yes), 60i128);
}

#[test]
fn burn_rejects_insufficient_balance() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    let result = client.try_burn(&1u32, &user, &TokenKind::Yes, &1i128);
    assert_eq!(result, Err(Ok(ContractError::InsufficientBalance)));
}

#[test]
#[should_panic]
fn mint_rejects_non_market_contract_caller() {
    let env = Env::default();
    let (client, _admin, _market) = setup(&env);
    let user = Address::generate(&env);

    // Drop the blanket auth mock so only the real market contract could
    // satisfy `config.market_contract.require_auth()`.
    env.mock_auths(&[]);

    client.mint(&1u32, &user, &TokenKind::Yes, &100i128);
}
