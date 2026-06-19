#![no_std]
//! # Vatix Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market. The treasury is intentionally minimal: it accepts fees from exactly
//! one authorised market contract and lets the admin drain them to any address.
//!
//! ## Deployment flow
//! 1. Deploy this contract to Soroban and note its contract ID.
//! 2. Call `initialize(admin, market_contract_address)`.
//! 3. Register the treasury address in the Market contract via
//!    `set_treasury_contract(admin, treasury_address)`.
//! 4. From this point on every `withdraw_unused_collateral` call in the Market
//!    contract will deduct a protocol fee and forward it here via
//!    `collect_fee`.
//!
//! ## Security invariants
//! - Only the stored `authorized_market` address may call `collect_fee`.
//!   Any other caller is rejected with [`TreasuryError::Unauthorized`].
//! - Only the stored `admin` may call `withdraw_fees`.
//! - `initialize` can only be called once; replaying it returns
//!   [`TreasuryError::AlreadyInitialized`].

mod error;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token::Client as TokenClient, Address, Env};

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    /// Bootstrap the treasury.
    ///
    /// Must be called once immediately after deployment. Stores the `admin`
    /// (who can withdraw fees) and the `authorized_market` (the only address
    /// permitted to call `collect_fee`).
    ///
    /// # Errors
    /// - [`TreasuryError::AlreadyInitialized`] – called a second time
    pub fn initialize(
        env: Env,
        admin: Address,
        authorized_market: Address,
    ) -> Result<(), TreasuryError> {
        admin.require_auth();

        if storage::has_admin(&env) {
            return Err(TreasuryError::AlreadyInitialized);
        }

        storage::set_admin(&env, &admin);
        storage::set_authorized_market(&env, &authorized_market);

        events::emit_treasury_initialized(&env, &admin, &authorized_market);
        Ok(())
    }

    /// Collect a protocol fee from the authorised market contract.
    ///
    /// Transfers `amount` stroops of `token` from the **caller** into the
    /// treasury. The caller must be the address registered as
    /// `authorized_market` during initialization, and it must have already
    /// approved the transfer (SAC `approve` or direct transfer authority).
    ///
    /// This function is designed to be invoked via a cross-contract call from
    /// the Market contract's `withdraw_unused_collateral` implementation after
    /// the fee is deducted from the user's withdrawal.
    ///
    /// # Arguments
    /// * `env`    – Contract environment
    /// * `caller` – Must equal the stored `authorized_market`
    /// * `token`  – SAC token address (e.g. USDC)
    /// * `amount` – Fee in stroops (must be > 0)
    ///
    /// # Returns
    /// The new cumulative total fees collected for this token.
    ///
    /// # Errors
    /// - [`TreasuryError::Unauthorized`]      – `caller` ≠ authorized_market
    /// - [`TreasuryError::InvalidAmount`]     – `amount` ≤ 0
    /// - [`TreasuryError::NotInitialized`]    – `initialize` was never called
    /// - [`TreasuryError::ArithmeticOverflow`] – cumulative counter overflows
    ///
    /// # Events
    /// Emits [`FeeCollectedEvent`] with the token, caller, amount, and new
    /// cumulative total.
    pub fn collect_fee(
        env: Env,
        caller: Address,
        token: Address,
        amount: i128,
    ) -> Result<i128, TreasuryError> {
        // 1. Guard: treasury must be initialized
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }

        // 2. Only the registered market contract may collect fees
        let authorized = storage::get_authorized_market(&env);
        if caller != authorized {
            return Err(TreasuryError::Unauthorized);
        }
        caller.require_auth();

        // 3. Validate amount
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        // 4. Pull the fee tokens from the market contract into this treasury.
        //    The market pre-approved this contract as a spender via `token.approve`,
        //    so we use transfer_from (spender=treasury, from=market) rather than
        //    transfer(from=market), which would require market's auth from within
        //    a sub-invocation and fail in production.
        let treasury_address = env.current_contract_address();
        TokenClient::new(&env, &token).transfer_from(
            &treasury_address,
            &caller,
            &treasury_address,
            &amount,
        );

        // 5. Update cumulative counter
        let previous = storage::get_cumulative_fees(&env, &token);
        let new_cumulative = previous
            .checked_add(amount)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        storage::set_cumulative_fees(&env, &token, new_cumulative);

        // 6. Emit event
        events::emit_fee_collected(&env, &token, &caller, amount, new_cumulative);

        Ok(new_cumulative)
    }

    /// Withdraw accumulated fees to an external address.
    ///
    /// Only the stored `admin` may call this. Transfers `amount` stroops of
    /// `token` held by the treasury to `to`.
    ///
    /// # Arguments
    /// * `env`    – Contract environment
    /// * `admin`  – Must match the stored admin address
    /// * `token`  – SAC token to withdraw
    /// * `to`     – Destination address
    /// * `amount` – Amount in stroops (must be > 0)
    ///
    /// # Errors
    /// - [`TreasuryError::NotAdmin`]       – `admin` ≠ stored admin
    /// - [`TreasuryError::InvalidAmount`]  – `amount` ≤ 0
    /// - [`TreasuryError::NotInitialized`] – `initialize` was never called
    ///
    /// # Events
    /// Emits [`FeesWithdrawnEvent`].
    pub fn withdraw_fees(
        env: Env,
        admin: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        // 1. Guard: treasury must be initialized
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }

        // 2. Caller must be admin
        let stored_admin = storage::get_admin(&env);
        if admin != stored_admin {
            return Err(TreasuryError::NotAdmin);
        }
        admin.require_auth();

        // 3. Validate amount
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        // 4. Transfer out (SAC will panic if balance insufficient — expected
        //    behaviour: admin should not overdraw)
        let treasury_address = env.current_contract_address();
        TokenClient::new(&env, &token).transfer(&treasury_address, &to, &amount);

        // 5. Emit event
        events::emit_fees_withdrawn(&env, &admin, &token, &to, amount);

        Ok(())
    }

    /// Query the SAC token balance held by the treasury for a given token.
    ///
    /// This reflects the *live* on-chain balance rather than the cumulative
    /// counter, so it correctly accounts for any direct transfers that may
    /// have occurred outside of `collect_fee`.
    ///
    /// # Arguments
    /// * `env`   – Contract environment
    /// * `token` – SAC token address to query
    ///
    /// # Returns
    /// Current balance in stroops.
    pub fn get_balance(env: Env, token: Address) -> i128 {
        let treasury_address = env.current_contract_address();
        TokenClient::new(&env, &token).balance(&treasury_address)
    }

    /// Return the cumulative fees collected for a token since initialization.
    ///
    /// Unlike [`get_balance`], this is a monotonically increasing counter and
    /// does not decrease when the admin withdraws.
    pub fn get_cumulative_fees(env: Env, token: Address) -> i128 {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the address of the admin.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – `initialize` was never called
    pub fn get_admin(env: Env) -> Result<Address, TreasuryError> {
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        Ok(storage::get_admin(&env))
    }

    /// Return the address of the authorised market contract.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – `initialize` was never called
    pub fn get_authorized_market(env: Env) -> Result<Address, TreasuryError> {
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        Ok(storage::get_authorized_market(&env))
    }
}
