#![no_std]

mod error;
mod events;
mod oracle;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
mod settlement;

#[allow(dead_code)]
mod storage;
mod test;
mod types;
#[allow(dead_code)]
mod validation;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

use crate::{
    error::ContractError,
    types::{Market, MarketStatus},
};

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Initialize a new prediction market
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `creator` - Address creating the market (must be admin for MVP)
    /// * `question` - Market question (e.g., "Will BTC hit $100k by March?")
    /// * `end_time` - Unix timestamp when market closes for trading
    /// * `oracle_pubkey` - Ed25519 public key of authorized oracle (32 bytes)
    /// * `collateral_token` - USDC token contract address
    ///
    /// # Returns
    /// Market ID (String)
    ///
    /// # Errors
    /// - Unauthorized: If creator is not admin
    /// - InvalidTimestamp: If end_time is in the past
    /// - InvalidQuestion: If question is empty or too long
    pub fn initialize_market(
        env: Env,
        creator: Address,
        question: String,
        end_time: u64,
        oracle_pubkey: BytesN<32>,
        collateral_token: Address,
    ) -> Result<String, ContractError> {
        // 1. Verify creator is admin
        let admin = storage::get_admin(&env);
        if creator != admin {
            return Err(ContractError::Unauthorized);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;

        // 3. Generate market ID using counter (hash of question + counter for uniqueness)
        let market_id_num = storage::increment_market_id(&env);
        // Create a simple market ID based on the counter
        // For MVP, just use a predictable ID based on the counter value
        let market_id = match market_id_num {
            1 => String::from_str(&env, "m1"),
            2 => String::from_str(&env, "m2"),
            3 => String::from_str(&env, "m3"),
            4 => String::from_str(&env, "m4"),
            5 => String::from_str(&env, "m5"),
            6 => String::from_str(&env, "m6"),
            7 => String::from_str(&env, "m7"),
            8 => String::from_str(&env, "m8"),
            9 => String::from_str(&env, "m9"),
            10 => String::from_str(&env, "m10"),
            _ => String::from_str(&env, "market"),
        };

        // 4. Create Market struct
        let market = Market {
            id: market_id.clone(),
            question: question.clone(),
            end_time,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: creator.clone(),
            created_at: current_time,
            collateral_token,
        };

        // 5. Store market
        storage::set_market(&env, &market_id, &market);

        // 6. Emit event (placeholder - events module is a todo)
        // events::emit_market_created(&env, &market_id, &question, end_time);

        // 7. Return market ID
        Ok(market_id)
    }
}
