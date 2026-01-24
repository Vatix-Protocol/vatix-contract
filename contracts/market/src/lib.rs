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
#[cfg(test)]
mod test;
mod types;
#[allow(dead_code)]
mod validation;
use crate::error::ContractError;
use crate::types::{Market, MarketStatus};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Initialize a new market
    pub fn initialize_market(
        env: Env,
        creator: Address,
        question: String,
        end_time: u64,
        oracle_pubkey: BytesN<32>,
        collateral_token: Address,
    ) -> Result<u32, ContractError> {
        // 1. Verify creator is admin
        creator.require_auth();
        let admin = storage::get_admin(&env);
        if creator != admin {
            return Err(ContractError::Unauthorized);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;

        // 3. Generate market ID
        let market_id = storage::increment_market_id(&env);

        // 4. Create Market struct
        let market = Market {
            id: market_id,
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
        storage::set_market(&env, market_id, &market);

        // 6. Emit event
        events::emit_market_created(&env, market_id, &question, end_time);

        // 7. Return market ID
        Ok(market_id)
    }
}
