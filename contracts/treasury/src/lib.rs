#![no_std]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Any address in the authorized market registry may deposit
//! fees via [`TreasuryContract::collect_fee`]; the **admin** controls all
//! other privileged operations (withdrawal, registry management).
//!
//! ## Authorization model
//!
//! | Operation              | Who may call                     |
//! |------------------------|----------------------------------|
//! | `initialize`           | anyone (once)                    |
//! | `collect_fee`          | any registered market contract   |
//! | `withdraw_fees`        | admin                            |
//! | `add_market`           | admin                            |
//! | `remove_market`        | admin                            |
//! | Getters                | anyone                           |

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    // ── Lifecycle ──────────────────────────────────────────────────────────────

    /// Bootstrap the treasury with an initial market contract in the registry.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        admin.require_auth();
        if storage::has_admin(&env) {
            return Err(TreasuryError::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        let mut markets = Vec::new(&env);
        markets.push_back(market_contract.clone());
        storage::set_authorized_markets(&env, &markets);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from any registered market contract.
    pub fn collect_fee(
        env: Env,
        caller: Address,
        token: Address,
        market_id: u32,
        fee_amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if !storage::is_authorized_market(&env, &caller) {
            return Err(TreasuryError::CallerNotMarket);
        }
        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let prev_balance = storage::get_token_balance(&env, &token);
        let new_balance = prev_balance
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_token_balance(&env, &token, new_balance);

        let prev_cumulative = storage::get_cumulative_fees(&env, &token);
        let new_cumulative = prev_cumulative
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_cumulative_fees(&env, &token, new_cumulative);

        let prev_total = storage::get_total_collected(&env);
        storage::set_total_collected(&env, prev_total.checked_add(fee_amount).unwrap_or(i128::MAX));

        events::emit_fee_collected(&env, market_id, &token, fee_amount, new_balance, new_cumulative);
        Ok(())
    }

    // ── Admin operations ───────────────────────────────────────────────────────

    /// Withdraw accumulated fees to a recipient address.
    pub fn withdraw_fees(
        env: Env,
        caller: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env);
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let balance = storage::get_token_balance(&env, &token);
        if amount > balance {
            return Err(TreasuryError::InsufficientBalance);
        }

        let treasury = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&treasury, &to, &amount);

        let remaining = balance - amount;
        storage::set_token_balance(&env, &token, remaining);

        events::emit_fees_withdrawn(&env, &token, &to, amount, remaining);
        Ok(())
    }

    /// Add a market contract to the authorized registry (admin only).
    ///
    /// No-ops if the market is already registered.
    pub fn add_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env);
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
            events::emit_market_contract_updated(&env, &market_contract, &market_contract);
        }
        Ok(())
    }

    /// Remove a market contract from the authorized registry (admin only).
    ///
    /// Returns [`TreasuryError::CallerNotMarket`] if the address is not registered.
    pub fn remove_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env);
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            return Err(TreasuryError::CallerNotMarket);
        }
        let updated: Vec<Address> = markets
            .iter()
            .filter(|m| m != market_contract)
            .collect();
        storage::set_authorized_markets(&env, &updated);
        Ok(())
    }

    // ── Getters ────────────────────────────────────────────────────────────────

    pub fn admin(env: Env) -> Address {
        storage::get_admin(&env)
    }

    /// Return the list of all authorized market contracts.
    pub fn list_markets(env: Env) -> Vec<Address> {
        storage::get_authorized_markets(&env)
    }

    /// Return whether `market` is in the authorized registry.
    pub fn is_authorized_market(env: Env, market: Address) -> bool {
        storage::is_authorized_market(&env, &market)
    }

    /// Return the current custodied balance for `token`.
    pub fn token_balance(env: Env, token: Address) -> i128 {
        storage::get_token_balance(&env, &token)
    }

    /// Return the per-token cumulative fees collected for `token`.
    pub fn get_cumulative_fees(env: Env, token: Address) -> i128 {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the global cumulative fees collected across all tokens.
    pub fn total_collected(env: Env) -> i128 {
        storage::get_total_collected(&env)
    }
}
