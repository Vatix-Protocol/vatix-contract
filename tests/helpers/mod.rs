/// Integration test harness helpers used across the contract integration tests.
///
/// This module provides small convenience types and assertions that keep tests
/// concise and readable. It intentionally lives in `tests/helpers` so test
/// files can import the helpers with `use crate::helpers::*`.
///
/// Example:
/// ```rust
/// # use soroban_sdk::Env;
/// # use tests::helpers::MarketParams;
/// let env = Env::default();
/// let params = MarketParams::default_valid(&env);
/// // Use `params` to initialize the contract under test, then assert events:
/// // assert_event_emitted(&env, "market_created");
/// ```
use soroban_sdk::{
    testutils::Events as _,
    Address, BytesN, Env, IntoVal, String,
};

/// Failure reasons for the integration test harness.
#[derive(Debug, PartialEq)]
pub enum HarnessError {
    /// Market initialization failed: the contract rejected the provided parameters.
    /// Check that `end_time` is in the future and `question` is non-empty.
    MarketInitFailed,

    /// Storage lookup returned None for the given market_id.
    /// The market was never created or the wrong id was queried.
    MarketNotFound,
}

/// Minimal market parameters used across integration tests.
pub struct MarketParams {
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub collateral_token: Address,
}

impl MarketParams {
    /// Build a default valid set of params relative to the current ledger time.
    pub fn default_valid(env: &Env) -> Self {
        Self {
            question: String::from_str(env, "Will BTC reach $100k?"),
            end_time: env.ledger().timestamp() + 86_400,
            oracle_pubkey: BytesN::from_array(env, &[1u8; 32]),
            collateral_token: Address::generate(env),
        }
    }
}

/// Assert that at least one event was emitted after an action in the harness.
///
/// Call this after any contract action to verify the event was published.
/// The `expected_topic` string is matched against the first topic symbol of
/// the most recent event.
pub fn assert_event_emitted(env: &Env, expected_topic: &str) {
    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "expected at least one event to be emitted"
    );
    let last = events.last().unwrap();
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap().into_val(env);
    assert_eq!(
        topic,
        soroban_sdk::Symbol::new(env, expected_topic),
        "event topic mismatch: expected '{expected_topic}'"
    );
}
