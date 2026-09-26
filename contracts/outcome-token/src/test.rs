#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use crate::{OutcomeToken, OutcomeTokenClient};

fn setup(env: &Env) -> (OutcomeTokenClient, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(OutcomeToken, ());
    let client = OutcomeTokenClient::new(env, &contract_id);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn metadata_defaults_are_stable() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    assert_eq!(client.name(), String::from_str(&env, "Vatix Outcome Token"));
    assert_eq!(client.symbol(), String::from_str(&env, "vOUT"));
    assert_eq!(client.decimals(), 7);
}

#[test]
fn admin_can_update_metadata() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_metadata(
        &String::from_str(&env, "Vatix Outcome YES"),
        &String::from_str(&env, "vYES"),
        &7,
    );

    assert_eq!(client.name(), String::from_str(&env, "Vatix Outcome YES"));
    assert_eq!(client.symbol(), String::from_str(&env, "vYES"));
    assert_eq!(client.decimals(), 7);
}

#[test]
fn metadata_update_is_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let name = String::from_str(&env, "Vatix Outcome YES");
    let symbol = String::from_str(&env, "vYES");

    client.set_metadata(&name, &symbol, &7);
    client.set_metadata(&name, &symbol, &7);

    assert_eq!(client.name(), name);
    assert_eq!(client.symbol(), symbol);
    assert_eq!(client.decimals(), 7);
}

#[test]
#[should_panic]
fn non_admin_cannot_update_metadata() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Drop the blanket auth mock so the missing admin auth is enforced.
    env.mock_auths(&[]);

    client.set_metadata(
        &String::from_str(&env, "Hijacked"),
        &String::from_str(&env, "HJK"),
        &7,
    );
}

#[test]
#[should_panic]
fn wrong_admin_cannot_update_metadata() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Only a different, non-admin address authorizes the call; the stored
    // admin check must still reject it (deny-by-default).
    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "set_metadata",
            args: (
                String::from_str(&env, "Hijacked"),
                String::from_str(&env, "HJK"),
                7u32,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.set_metadata(
        &String::from_str(&env, "Hijacked"),
        &String::from_str(&env, "HJK"),
        &7,
    );
}

#[test]
#[should_panic]
fn metadata_rejects_empty_name() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_metadata(&String::from_str(&env, ""), &String::from_str(&env, "vYES"), &7);
}

#[test]
#[should_panic]
fn metadata_rejects_empty_symbol() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_metadata(&String::from_str(&env, "Vatix Outcome YES"), &String::from_str(&env, ""), &7);
}

#[test]
#[should_panic]
fn metadata_rejects_invalid_decimals() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_metadata(
        &String::from_str(&env, "Vatix Outcome YES"),
        &String::from_str(&env, "vYES"),
        &19,
    );
}
