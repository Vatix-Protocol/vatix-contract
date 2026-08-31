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
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, IntoVal, String,
};
use vatix_market_contract::{oracle, storage, MarketContract};

/// Stroops per USDC (1 USDC = 10^7 stroops), shared across integration tests.
pub const STROOPS_PER_USDC: i128 = 10_000_000;

/// Register the `MarketContract` and initialize it with an admin.
///
/// Returns `(admin, contract_id)`. Uses the `initialize` entry point
/// to set the admin, which is the contract's official initialization path.
/// The storage version is set during `initialize`.
pub fn register_contract(env: &Env) -> (Address, Address) {
    let contract_id = env.register(MarketContract, ());
    let admin = Address::generate(env);
    env.as_contract(&contract_id, || {
        MarketContract::initialize(env.clone(), admin.clone()).unwrap();
    });
    (admin, contract_id)
}

/// Generate an oracle Ed25519 keypair, returning the on-chain public key and
/// the signing key used to sign a market resolution.
pub fn oracle_keypair(env: &Env) -> (BytesN<32>, SigningKey) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    (pubkey, signing_key)
}

/// Sign a market resolution outcome with the oracle signing key, producing a
/// signature the contract's `resolve_market` will accept. Mirrors the on-chain
/// message construction in `oracle::construct_oracle_message`.
pub fn sign_outcome(env: &Env, key: &SigningKey, market_id: u32, outcome: bool) -> BytesN<64> {
    let message = oracle::construct_oracle_message(env, market_id, outcome);
    let signature = key.sign(message.to_array().as_slice());
    BytesN::from_array(env, &signature.to_bytes())
}

/// Failure reasons for the integration test harness.
///
/// Each variant carries a human-readable message that pinpoints the failing
/// operation and the most likely cause, so test output is self-explanatory
/// without having to read the harness source.
#[derive(Debug, PartialEq)]
pub enum HarnessError {
    /// `initialize_market` returned a contract error.
    ///
    /// Possible causes:
    /// - `end_time` is in the past or more than one year from now
    ///   (`ContractError::InvalidTimestamp` / `#32`)
    /// - `question` is empty or ≥ 500 characters
    ///   (`ContractError::InvalidQuestion` / `#33`)
    /// - `oracle_pubkey` is all-zero bytes
    ///   (`ContractError::InvalidSignature` / `#20`)
    /// - Caller is not the stored admin
    ///   (`ContractError::NotAdmin` / `#41`)
    MarketInitFailed,

    /// `storage::get_market` returned `None` for the queried `market_id`.
    ///
    /// Possible causes:
    /// - The market was never created (check that `initialize_market` succeeded)
    /// - The wrong `market_id` was passed (IDs are auto-incremented from 1)
    /// - The market was created in a different contract instance or environment
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