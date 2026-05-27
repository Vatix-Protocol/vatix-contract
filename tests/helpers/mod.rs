use soroban_sdk::{Address, BytesN, Env, String};

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
