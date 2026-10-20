//! Shared assertion coverage for `assert_event_emitted` (#1058).
//!
//! These tests pin the helper's documented matching rule (scan all emitted
//! events, compare each event's first topic symbol with exact equality):
//! it must pass when the topic appears among several emitted events, fail
//! when the topic was never emitted, and must NOT match a topic that is
//! only a substring of an emitted topic.

#[allow(dead_code)]
mod helpers;

use helpers::{assert_event_emitted, register_contract};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};
use vatix_market_contract::events;

/// Register the harness contract and emit `topics` in order, returning the
/// contract id for the emission context.
///
/// Each topic is produced through a real contract event emission so the
/// helper reads topics recorded in the test env's event log (the deprecated
/// `Env::events().publish` does not record events in soroban-sdk 23, so that
/// path is deliberately avoided).
fn emit_topics(env: &Env, topics: &[&str]) -> Address {
    let (_admin, contract_id) = register_contract(env);
    let account = Address::generate(env);
    let market_id = 1u32;

    // Emit every event inside one contract frame: the test host records the
    // event log per invocation, so a multi-event assertion must observe
    // events published by a single contract action.
    env.as_contract(&contract_id, || {
        for &topic in topics {
            match topic {
                "trade_executed" => {
                    events::emit_trade_executed(&env, market_id, &account, 100, 5000, true, 0);
                }
                "position_updated" => {
                    events::emit_position_updated(&env, market_id, &account, 100, 45, 0);
                }
                "market_created" => {
                    events::emit_market_created(
                        &env,
                        market_id,
                        &account,
                        &String::from_str(env, "Test question"),
                        1000000,
                        &None,
                    );
                }
                other => panic!("no event emission path for '{other}' in assertion helper"),
            }
        }
    });

    contract_id
}

#[test]
fn assert_event_emitted_passes_for_topic_present_among_several_events() {
    let env = Env::default();
    emit_topics(
        &env,
        &["trade_executed", "position_updated", "market_created"],
    );

    // Present but NOT the most recent event: proves the helper scans all
    // emitted events rather than only checking the last one.
    assert_event_emitted(&env, "trade_executed");
    // The most recent event matches too.
    assert_event_emitted(&env, "market_created");
}

#[test]
#[should_panic(expected = "event topic mismatch")]
fn assert_event_emitted_fails_for_topic_never_emitted() {
    let env = Env::default();
    emit_topics(&env, &["trade_executed", "market_created"]);

    assert_event_emitted(&env, "resolution_executed_event");
}

#[test]
#[should_panic(expected = "event topic mismatch")]
fn assert_event_emitted_does_not_match_substring_topic() {
    let env = Env::default();
    // Only the longer topic is emitted: exact equality must reject the
    // substring form `trade` of `trade_executed`.
    emit_topics(&env, &["trade_executed"]);

    assert_event_emitted(&env, "trade");
}

#[test]
fn assert_event_emitted_passes_for_real_contract_event_topics() {
    let env = Env::default();
    let (_admin, contract_id) = register_contract(&env);
    let account = Address::generate(&env);
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Real `#[contractevent]` emissions through the market crate's event
        // module: first topics are the event names (`fee_waiver_added`,
        // `fee_waiver_removed`), which the helper scans among several events.
        events::emit_fee_waiver_added(&env, &account, &admin);
        events::emit_fee_waiver_removed(&env, &account, &admin);
    });

    assert_event_emitted(&env, "fee_waiver_added");
    assert_event_emitted(&env, "fee_waiver_removed");
}
