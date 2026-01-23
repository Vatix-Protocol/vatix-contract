#![no_std]

mod error;
mod events;
mod oracle;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
mod settlement;
mod storage;
mod test;
mod types;
mod validation;

use soroban_sdk::{
    contract, contractimpl, Address, Bytes, BytesN, Env, String,
};

use crate::{
    error::ContractError,
    types::{Market, MarketStatus},
};

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Initialize a new prediction market
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

        // 3. Generate deterministic market ID: hash(question + end_time)
        let mut input = Bytes::new(&env);
        input.append(&question.clone().into_bytes());
        input.append(&end_time.to_be_bytes());

        let hash = env.crypto().sha256(&input);
        let market_id = String::from_str(
            &env,
            &hex::encode(&hash.to_array()[..16]), // shorten for readability
        );

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

        // 6. Emit event
        events::emit_market_created(
            &env,
            &market_id,
            &question,
            end_time,
            &creator,
        );

        // 7. Return market ID
        Ok(market_id)
    }
}
