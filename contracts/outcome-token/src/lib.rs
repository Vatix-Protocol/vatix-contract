#![no_std]

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{OutcomeTokenConfig, TokenKind};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

#[contract]
pub struct OutcomeTokenContract;

#[contractimpl]
impl OutcomeTokenContract {
    /// Bootstrap the contract.
    ///
    /// `name` and `symbol` are SAC-compatible metadata stored on-chain.
    /// `decimals` is a compile-time constant (7) and is not stored.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
        name: String,
        symbol: String,
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
                name,
                symbol,
            },
        );
        Ok(())
    }

    pub fn get_config(env: Env) -> OutcomeTokenConfig {
        storage::get_config(&env)
    }

    /// Update the market contract address allowed to mint/burn tokens.
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

    /// Update the SAC metadata (name and symbol). Admin only.
    pub fn set_metadata(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        config.name = name;
        config.symbol = symbol;
        storage::set_config(&env, &config);
        Ok(())
    }

    // ── SAC metadata getters ──────────────────────────────────────────────────

    pub fn name(env: Env) -> String {
        storage::get_config(&env).name
    }

    pub fn symbol(env: Env) -> String {
        storage::get_config(&env).symbol
    }

    /// Number of decimal places (SAC standard: 7).
    pub fn decimals(_env: Env) -> u32 {
        7
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
        let new_balance = balance.checked_add(amount).ok_or(ContractError::Overflow)?;
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
