#![no_std]
#![deny(clippy::all)]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Only the registered **market contract** may deposit fees
//! via [`TreasuryContract::collect_fee`]; the **admin** controls all other
//! privileged operations (withdrawal, market contract rotation).
//!
//! ## Fee flow
//!
//! ```text
//!  User withdrawal
//!      │  fee_amount > 0
//!      ▼
//!  MarketContract
//!      │  token.transfer(market → treasury, fee_amount)
//!      │  treasury.collect_fee(market, token, market_id, fee_amount)
//!      ▼
//!  TreasuryContract  ← accumulates per-token balances
//!      │  (admin only)
//!      ▼
//!  treasury.withdraw_fees(admin, token, to, amount)
//! ```
//!
//! ## Authorization model
//!
//! | Operation             | Who may call              |
//! |-----------------------|---------------------------|
//! | `initialize`          | anyone (once)             |
//! | `collect_fee`         | registered market contract|
//! | `withdraw_fees`       | admin                     |
//! | `set_market_contract` | admin                     |
//! | Getters               | anyone                    |
//!
//! ## Storage layout
//!
//! | Key                       | Type      | Description                              |
//! |---------------------------|-----------|------------------------------------------|
//! | `StorageVersion`          | `u32`     | Schema version guard                     |
//! | `Admin`                   | `Address` | Protocol admin                           |
//! | `AuthorizedMarket`        | `Address` | Authorized fee-depositing contract       |
//! | `TokenBalance(Address)`   | `i128`    | Current custodied balance (decreasable)  |
//! | `CumulativeFees(Address)` | `i128`    | Historical total collected (monotone)    |

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env};

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    // ── Lifecycle ──────────────────────────────────────────────────────────────

    /// Bootstrap the treasury.
    ///
    /// Sets the admin and registers the market contract permitted to call
    /// [`collect_fee`]. Must be called once after deployment; subsequent calls
    /// return [`TreasuryError::AlreadyInitialized`].
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
        storage::set_authorized_market(&env, &market_contract);
        storage::set_version(&env);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from a market withdrawal.
    ///
    /// Only the registered market contract may call this. The market contract
    /// must transfer `fee_amount` tokens to this contract's address *before*
    /// (or atomically with) this call so that on-chain balances stay consistent.
    ///
    /// Updates both the current `TokenBalance` and the monotone `CumulativeFees`
    /// counter for `token`.
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
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        let market_contract = storage::get_authorized_market(&env)?;
        if caller != market_contract {
            return Err(TreasuryError::CallerNotMarket);
        }
        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let prev_balance = storage::get_token_balance(&env, &token)?;
        let new_balance = prev_balance
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_token_balance(&env, &token, new_balance);

        let prev_cumulative = storage::get_cumulative_fees(&env, &token)?;
        let new_cumulative = prev_cumulative
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_cumulative_fees(&env, &token, new_cumulative);

        let prev_total = storage::get_total_collected(&env)?;
        storage::set_total_collected(
            &env,
            prev_total.checked_add(fee_amount).unwrap_or(i128::MAX),
        );

        events::emit_fee_collected(&env, market_id, &token, fee_amount, new_balance, new_cumulative);
        Ok(())
    }

    // ── Admin operations ───────────────────────────────────────────────────────

    /// Withdraw accumulated fees to a recipient address.
    ///
    /// Decrements the custodied `TokenBalance` but leaves `CumulativeFees`
    /// untouched — the historical counter is monotone by design.
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
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let balance = storage::get_token_balance(&env, &token)?;
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

    /// Transfer admin rights to a new address immediately.
    ///
    /// Only the current admin may call this.
    pub fn transfer_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::set_admin(&env, &new_admin);
        events::emit_admin_transferred(&env, &admin, &new_admin);
        Ok(())
    }

    /// Rotate the registered market contract address (e.g. after an upgrade).
    ///
    /// Only the admin may call this.
    pub fn set_market_contract(
        env: Env,
        caller: Address,
        new_market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let old = storage::get_authorized_market(&env)?;
        storage::set_authorized_market(&env, &new_market_contract);
        events::emit_market_contract_updated(&env, &old, &new_market_contract);
        Ok(())
    }

    /// Pause the treasury, blocking `collect_fee` and `withdraw_fees`.
    ///
    /// Intended for use during contract upgrades or incident response. Only the
    /// admin may call this.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    pub fn pause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, true);
        events::emit_treasury_paused(&env, &caller);
        Ok(())
    }

    /// Unpause the treasury, restoring normal operation.
    ///
    /// Only the admin may call this.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    pub fn unpause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, false);
        events::emit_treasury_unpaused(&env, &caller);
        Ok(())
    }

    // ── Getters ────────────────────────────────────────────────────────────────

    /// Return whether the treasury is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Return the admin address. Returns `UpgradeRequired` if version mismatches.
    pub fn admin(env: Env) -> Result<Address, TreasuryError> {
        storage::get_admin(&env)
    }

    /// Return the registered market contract address. Returns `UpgradeRequired` if version mismatches.
    pub fn market_contract(env: Env) -> Result<Address, TreasuryError> {
        storage::get_authorized_market(&env)
    }

    /// Return the current custodied balance for `token` (decreases on withdrawal).
    pub fn token_balance(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_token_balance(&env, &token)
    }

    /// Return the per-token cumulative fees collected for `token` since deployment.
    ///
    /// This counter never decreases: admin withdrawals do not affect it.
    pub fn get_cumulative_fees(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the global cumulative fees collected across all tokens since deployment.
    ///
    /// Monotone: never decreases regardless of admin withdrawals.
    pub fn total_collected(env: Env) -> Result<i128, TreasuryError> {
        storage::get_total_collected(&env)
    }
}
