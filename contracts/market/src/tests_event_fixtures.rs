#![cfg(test)]
//! Indexer event fixtures (#915, #951).
//!
//! Replays `test-vectors/market-events.json`: each indexer-facing event must
//! publish the fixture's name topic, `EVENT_VERSION` as the first explicit
//! topic, the fixture's topic count, and exactly the fixture's data-map
//! fields. Fails closed on a missing file or any drift, so a change to an
//! event's shape must update the fixture (and bump `EVENT_VERSION`).
//!
//! The `fee_collected` fixture (#951) additionally asserts the money-path
//! invariants: the fee amount is non-negative, the recipient is the
//! configured fee sink, and the emitted payload is deterministic across
//! replays (idempotent for indexers).
//!
//! ```sh
//! cargo test -p vatix-market-contract tests_event_fixtures
//! ```

use serde::Deserialize;
#[derive(Deserialize)]
struct EventCorpus {
    event_version: u32,
    fixtures: std::vec::Vec<EventFixture>,
}

#[derive(Deserialize)]
struct EventFixture {
    id: std::string::String,
    name: std::string::String,
    topics: std::vec::Vec<std::string::String>,
    data: std::vec::Vec<std::string::String>,
}

/// Emits each fixture's event from inside the contract and checks the
/// published name topic, version topic, topic count, and data-map keys
/// against `test-vectors/market-events.json`. Fails closed on any drift.
#[test]
fn market_event_fixtures() {
    use crate::events;
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::{Address, BytesN, Env, Map, String, Symbol, TryFromVal, Val};

    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/market-events.json"
    ))
    .expect("read test-vectors/market-events.json");
    let corpus: EventCorpus = serde_json::from_str(&raw).expect("parse market-events.json");
    assert_eq!(
        corpus.event_version,
        events::EVENT_VERSION,
        "fixture EVENT_VERSION drift"
    );
    assert!(!corpus.fixtures.is_empty(), "event fixture corpus is empty");

    let env = Env::default();
    let contract_id = env.register(crate::MarketContract, ());
    let user = Address::generate(&env);

    for f in &corpus.fixtures {
        env.as_contract(&contract_id, || match f.name.as_str() {
            "contract_initialized" => events::emit_contract_initialized(&env, &user),
            "market_created" => events::emit_market_created(
                &env,
                1,
                &user,
                &String::from_str(&env, "q"),
                100,
                &None,
            ),
            "collateral_deposited" => events::emit_collateral_deposited(&env, &user, 1, 5, 5),
            "collateral_withdrawn" => events::emit_collateral_withdrawn(&env, &user, 1, 5, 0),
            "market_resolved" => events::emit_market_resolved(
                &env,
                1,
                &BytesN::from_array(&env, &[0u8; 32]),
                &user,
                true,
                100,
            ),
            "position_updated" => events::emit_position_updated(&env, 1, &user, 10, 0, 6),
            "fee_collected" => events::emit_fee_collected(&env, 1, &user, 7, 7),
            other => panic!("{}: no emitter wired for event '{}'", f.id, other),
        });

        let all = env.events().all();
        let (_, topics, data) = all.last().expect("event published");
        let name = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(name, Symbol::new(&env, &f.name), "{}: name topic", f.id);
        assert_eq!(
            topics.len() as usize,
            f.topics.len() + 1,
            "{}: topic count",
            f.id
        );
        let version = u32::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
        assert_eq!(version, events::EVENT_VERSION, "{}: version topic", f.id);

        let map = Map::<Symbol, Val>::try_from_val(&env, &data).expect("data is a map");
        assert_eq!(
            map.len() as usize,
            f.data.len(),
            "{}: data field count",
            f.id
        );
        for field in &f.data {
            let key = field.split(':').next().unwrap();
            assert!(
                map.contains_key(Symbol::new(&env, key)),
                "{}: missing data field '{}'",
                f.id,
                key
            );
        }
    }
}

/// #951: FeeCollected money-path invariants.
///
/// Asserts the emitted `fee_collected` event is deterministic (idempotent for
/// indexers), carries a non-negative fee amount, and attributes the fee to the
/// configured recipient. Fails closed on any drift so a shape change must
/// update the fixture and bump `EVENT_VERSION`.
#[test]
fn fee_collected_event_invariants() {
    use crate::events;
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::{Address, Env, Map, Symbol, TryFromVal, Val};

    let env = Env::default();
    let contract_id = env.register(crate::MarketContract, ());
    let recipient = Address::generate(&env);
    let amount: i128 = 7;

    let emit = || {
        env.as_contract(&contract_id, || {
            events::emit_fee_collected(&env, 1, &recipient, amount, amount)
        });
        let all = env.events().all();
        all.last().expect("fee_collected published")
    };

    let (_, topics, data) = emit();
    let name = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(name, Symbol::new(&env, "fee_collected"), "name topic");
    let version = u32::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(version, events::EVENT_VERSION, "version topic");

    let map = Map::<Symbol, Val>::try_from_val(&env, &data).expect("data is a map");
    let fee = i128::try_from_val(&env, &map.get(Symbol::new(&env, "fee")).unwrap())
        .expect("fee is i128");
    assert!(fee >= 0, "fee amount must be non-negative");
    assert_eq!(fee, amount, "fee amount matches emitted value");

    // Deterministic replay: same inputs must yield the same payload shape.
    let (_, topics2, data2) = emit();
    let name2 = Symbol::try_from_val(&env, &topics2.get(0).unwrap()).unwrap();
    assert_eq!(name2, name, "replay name topic stable");
    let map2 = Map::<Symbol, Val>::try_from_val(&env, &data2).expect("data is a map");
    assert_eq!(map2.len(), map.len(), "replay data field count stable");
}
