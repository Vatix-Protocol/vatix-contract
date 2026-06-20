#![no_std]

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{OutcomeTokenConfig, TokenKind};
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct OutcomeTokenContract;

#[contractimpl]
impl OutcomeTokenContract {
    /// Bootstrap the contract by registering the admin and the sole market
    /// contract allowed to call `mint` and `burn`.
    ///
    /// Must be called once immediately after deployment. Returns
    /// [`ContractError::AlreadyInitialized`] on subsequent calls.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(ContractError::AlreadyInitialized);
        }
        storage::set_config(
            &env,
            &OutcomeTokenConfig {
                admin,
                market_contract,
            },
        );
        Ok(())
    }

    pub fn get_config(env: Env) -> OutcomeTokenConfig {
        storage::get_config(&env)
    }

    /// Update the market contract address allowed to mint/burn tokens.
    ///
    /// Only the stored admin may call this.
    pub fn set_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        config.market_contract = market_contract;
        storage::set_config(&env, &config);
        Ok(())
    }

    /// Mint `amount` tokens of `kind` (Yes or No) for `user` in `market_id`.
    ///
    /// Only the registered market contract may call this function.
    pub fn mint(
        env: Env,
        market_id: u32,
        user: Address,
        kind: TokenKind,
        amount: i128,
    ) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        let config = storage::get_config(&env);
        config.market_contract.require_auth();

        let balance = storage::get_balance(&env, market_id, &user, &kind);
        let new_balance = balance
            .checked_add(amount)
            .ok_or(ContractError::Overflow)?;
        storage::set_balance(&env, market_id, &user, &kind, new_balance);

        let supply = storage::get_total_supply(&env, market_id, &kind);
        let new_supply = supply.checked_add(amount).ok_or(ContractError::Overflow)?;
        storage::set_total_supply(&env, market_id, &kind, new_supply);

        events::emit_token_minted(&env, market_id, &user, kind, amount, new_balance);
        Ok(())
    }

    /// Burn `amount` tokens of `kind` from `user` in `market_id`.
    ///
    /// Only the registered market contract may call this function. Returns
    /// [`ContractError::InsufficientBalance`] if the user holds fewer tokens
    /// than `amount`.
    pub fn burn(
        env: Env,
        market_id: u32,
        user: Address,
        kind: TokenKind,
        amount: i128,
    ) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        let config = storage::get_config(&env);
        config.market_contract.require_auth();

        let balance = storage::get_balance(&env, market_id, &user, &kind);
        if balance < amount {
            return Err(ContractError::InsufficientBalance);
        }
        let new_balance = balance - amount;
        storage::set_balance(&env, market_id, &user, &kind, new_balance);

        let supply = storage::get_total_supply(&env, market_id, &kind);
        let new_supply = supply - amount;
        storage::set_total_supply(&env, market_id, &kind, new_supply);

        events::emit_token_burned(&env, market_id, &user, kind, amount, new_balance);
        Ok(())
    }

    /// Return the token balance for a specific `(market_id, user, kind)` triple.
    pub fn balance(env: Env, market_id: u32, user: Address, kind: TokenKind) -> i128 {
        storage::get_balance(&env, market_id, &user, &kind)
    }

    /// Return the total outstanding supply for a `(market_id, kind)` pair.
    pub fn total_supply(env: Env, market_id: u32, kind: TokenKind) -> i128 {
        storage::get_total_supply(&env, market_id, &kind)
    }
}
