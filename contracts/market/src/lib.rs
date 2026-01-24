#![no_std]

mod deposit;
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

use soroban_sdk::{contract, contractimpl, Address, Env, String};
use crate::error::ContractError;

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Deposit USDC collateral into a prediction market
    ///
    /// # Arguments
    /// * `user` - User's Stellar address (must authorize this call)
    /// * `market_id` - Market identifier
    /// * `amount` - Amount in stroops (1 USDC = 10^7 stroops)
    ///
    /// # Errors
    /// - `MarketNotFound`: market_id doesn't exist
    /// - `MarketNotActive`: Market is resolved or cancelled
    /// - `InvalidQuantity`: amount <= 0 or exceeds max
    /// - `TokenTransferFailed`: USDC transfer failed
    /// - `ArithmeticOverflow`: Amount would cause overflow
    pub fn deposit_collateral(
        env: Env,
        user: Address,
        market_id: String,
        amount: i128,
    ) -> Result<(), ContractError> {
        deposit::deposit_collateral(env, user, market_id, amount)
    }
}

