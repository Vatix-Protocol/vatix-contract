#![no_std]
#![warn(clippy::all)]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Any address in the authorized market registry may deposit
//! fees via [`TreasuryContract::collect_fee`]; the **admin** controls all
//! other privileged operations (withdrawal, registry management).
//!
//! ## Authorization model
//!
//! | Operation                          | Who may call              |
//! |------------------------------------|---------------------------|
//! | `initialize`                       | anyone (once)             |
//! | `collect_fee`                      | registered market contract|
//! | `withdraw_fees`                    | admin                     |
//! | `add_market` / `remove_market`     | admin                     |
//! | `set_market_contract`              | admin                     |
//! | `set_stakeholders`                 | admin                     |
//! | `distribute_fees`                  | admin                     |
//! | Getters                            | anyone                    |
//!
//! ## Storage layout
//!
//! | Key                       | Type                  | Description                               |
//! |---------------------------|-----------------------|-------------------------------------------|
//! | `StorageVersion`          | `u32`                 | Schema version guard                      |
//! | `Admin`                   | `Address`             | Protocol admin                            |
//! | `AuthorizedMarkets`       | `Vec<Address>`        | Market contracts allowed to call `collect_fee` |
//! | `TokenBalance(Address)`   | `i128`                | Current custodied balance per token (decreasable) |
//! | `CumulativeFees(Address)` | `i128`                | Historical total collected per token (monotone)   |
//! | `TotalCollected`          | `i128`                | Global monotone counter across all tokens |
//! | `Stakeholders`            | `Vec<(Address, u32)>` | Revenue-share list, `share_bps` sums to 10_000 (#485) |
//! | `FeeTokens`               | `Vec<Address>`        | Registry of every token ever collected (#484) |

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};

/// Basis-point denominator: stakeholder shares must sum to exactly this value.
const BPS_DENOMINATOR: i128 = 10_000;

/// Uniform timelock delay for privileged address mutations
pub const ADDRESS_TIMELOCK_SECONDS: u64 = 172_800;

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
        let markets = soroban_sdk::vec![&env, market_contract.clone()];
        storage::set_authorized_markets(&env, &markets);
        storage::set_version(&env);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from any registered market contract.
    ///
    /// `token` identifies which token mint the fee was paid in (#484): the
    /// treasury custodies an independent balance per token, so markets using
    /// different collateral tokens can all route fees through the same
    /// treasury deployment without their balances colliding.
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
        // Emergency mode: fee collection is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
        if !storage::is_authorized_market(&env, &caller) {
            return Err(TreasuryError::CallerNotMarket);
        }
        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        // Track every token we've ever seen so callers can enumerate the full
        // set of fee-bearing tokens without prior knowledge (#484).
        storage::register_fee_token(&env, &token);

        let prev_balance = storage::get_token_balance(&env, &token)?;
        let new_balance = prev_balance.checked_add(fee_amount).unwrap_or(i128::MAX);
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

        events::emit_fee_collected(
            &env,
            market_id,
            &token,
            fee_amount,
            new_balance,
            new_cumulative,
        );
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
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        // Emergency mode: fee withdrawal is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
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

        let prev_total = storage::get_total_collected(&env)?;
        storage::set_total_collected(&env, prev_total.checked_sub(amount).unwrap_or(0));

        events::emit_fees_withdrawn(&env, &token, &to, amount, remaining);
        Ok(())
    }

    pub fn propose_admin(
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

        let effective_at = env.ledger().timestamp() + ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_admin(
            &env,
            &storage::PendingAddressChange {
                new_address: new_admin.clone(),
                effective_at,
            },
        );

        events::emit_admin_transfer_proposed(&env, &admin, &new_admin, effective_at);
        Ok(())
    }

    pub fn execute_admin(env: Env) -> Result<Address, TreasuryError> {
        let pending = storage::get_pending_admin(&env)
            .ok_or(TreasuryError::Unauthorized)?; // Using Unauthorized as fallback for now

        if env.ledger().timestamp() < pending.effective_at {
            return Err(TreasuryError::Unauthorized); // TimelockNotElapsed
        }

        let current_admin = storage::get_admin(&env)?;
        storage::set_admin(&env, &pending.new_address);
        storage::clear_pending_admin(&env);

        events::emit_admin_transferred(&env, &current_admin, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_admin(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::clear_pending_admin(&env);
        Ok(())
    }

    /// Register an additional market contract allowed to call `collect_fee`.
    ///
    /// Idempotent — adding an already-registered market is a no-op. Only the
    /// admin may call this.
    pub fn add_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
            events::emit_market_added(&env, &market_contract);
        }
        Ok(())
    }

    /// Deregister a market contract, revoking its ability to call `collect_fee`.
    ///
    /// Removing an unknown market is a no-op that preserves the existing
    /// registry contents.
    ///
    /// Only the admin may call this.
    pub fn remove_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            return Ok(());
        }
        let mut updated = Vec::new(&env);
        for m in markets.iter() {
            if m != market_contract {
                updated.push_back(m);
            }
        }
        storage::set_authorized_markets(&env, &updated);
        events::emit_market_removed(&env, &market_contract);
        Ok(())
    }

    pub fn propose_market_contract(
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

        let effective_at = env.ledger().timestamp() + ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_market_contract(
            &env,
            &storage::PendingAddressChange {
                new_address: new_market_contract.clone(),
                effective_at,
            },
        );

        events::emit_market_contract_proposed(&env, &new_market_contract, effective_at);
        Ok(())
    }

    pub fn execute_market_contract(env: Env) -> Result<Address, TreasuryError> {
        let pending = storage::get_pending_market_contract(&env)
            .ok_or(TreasuryError::Unauthorized)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = soroban_sdk::vec![&env, pending.new_address.clone()];
        storage::set_authorized_markets(&env, &markets);
        storage::clear_pending_market_contract(&env);

        events::emit_market_contract_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_market_contract(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::clear_pending_market_contract(&env);
        Ok(())
    }

    /// Pause the treasury, blocking `collect_fee` and `withdraw_fees`.
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

    /// Set the mirrored emergency mode (Issue #662).
    ///
    /// Only the treasury admin may call this. Operators should keep this value
    /// in sync with the Market and Resolution contracts for coordinated behaviour.
    pub fn set_emergency_mode(
        env: Env,
        caller: Address,
        new_mode: storage::EmergencyMode,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_emergency_mode(&env, &new_mode);
        events::emit_emergency_mode_changed(&env, &new_mode, &caller);
        Ok(())
    }

    /// Return the current mirrored emergency mode.
    pub fn get_emergency_mode(env: Env) -> storage::EmergencyMode {
        storage::get_emergency_mode(&env)
    }

    // ── Stakeholder fee distribution (#485) ────────────────────────────────────

    /// Configure the stakeholder revenue-share list (admin only).
    ///
    /// `stakeholders` is a list of `(address, share_bps)` pairs. `share_bps`
    /// values must sum to exactly 10_000 (100%); this fully replaces any
    /// previously configured list.
    pub fn set_stakeholders(
        env: Env,
        caller: Address,
        stakeholders: Vec<(Address, u32)>,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if stakeholders.is_empty() {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let mut total: i128 = 0;
        for (_, share_bps) in stakeholders.iter() {
            total = total
                .checked_add(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        }
        if total != BPS_DENOMINATOR {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let count = stakeholders.len();
        storage::set_stakeholders(&env, &stakeholders);
        events::emit_stakeholders_updated(&env, count);
        Ok(())
    }

    /// Return the configured `(stakeholder, share_bps)` list.
    pub fn get_stakeholders(env: Env) -> Result<Vec<(Address, u32)>, TreasuryError> {
        storage::get_stakeholders(&env)
    }

    /// Distribute the treasury's current `token` balance to the configured
    /// stakeholders, proportionally to their `share_bps` weight (admin only).
    ///
    /// Each stakeholder receives `floor(balance * share_bps / 10_000)`. Any
    /// integer-division remainder (dust) stays in the treasury balance and
    /// rolls into the next distribution.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::ContractPaused`] – treasury is paused.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    /// - [`TreasuryError::NoStakeholdersConfigured`] – `set_stakeholders` has
    ///   never been called.
    /// - [`TreasuryError::InsufficientBalance`] – the current `token` balance is zero.
    pub fn distribute_fees(env: Env, caller: Address, token: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        // Emergency mode: fee distribution is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let stakeholders = storage::get_stakeholders(&env)?;
        if stakeholders.is_empty() {
            return Err(TreasuryError::NoStakeholdersConfigured);
        }

        let balance = storage::get_token_balance(&env, &token)?;
        if balance <= 0 {
            return Err(TreasuryError::InsufficientBalance);
        }

        let treasury = env.current_contract_address();
        let token_client = token::Client::new(&env, &token);

        let mut distributed: i128 = 0;
        for (stakeholder, share_bps) in stakeholders.iter() {
            let amount = balance
                .checked_mul(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?
                / BPS_DENOMINATOR;
            if amount > 0 {
                token_client.transfer(&treasury, &stakeholder, &amount);
                distributed = distributed
                    .checked_add(amount)
                    .ok_or(TreasuryError::ArithmeticOverflow)?;
            }
        }

        let remaining = balance - distributed;
        storage::set_token_balance(&env, &token, remaining);
        events::emit_fees_distributed(&env, &token, distributed, remaining, stakeholders.len());
        Ok(())
    }

    // ── Getters ────────────────────────────────────────────────────────────────

    /// Return whether the treasury is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Return the admin address.
    pub fn admin(env: Env) -> Result<Address, TreasuryError> {
        storage::get_admin(&env)
    }

    /// Return the primary registered market contract address (the first entry
    /// in the authorized-markets registry). Returns `NotInitialized` if no
    /// market has ever been registered.
    pub fn market_contract(env: Env) -> Result<Address, TreasuryError> {
        storage::get_authorized_markets(&env)
            .get(0)
            .ok_or(TreasuryError::NotInitialized)
    }

    /// Return whether `market` is currently authorized to call `collect_fee`.
    pub fn is_authorized_market(env: Env, market: Address) -> bool {
        storage::is_authorized_market(&env, &market)
    }

    /// Return every market contract currently authorized to call `collect_fee`.
    pub fn list_markets(env: Env) -> Result<Vec<Address>, TreasuryError> {
        Ok(storage::get_authorized_markets(&env))
    }

    /// Return every distinct token mint that has ever had a fee collected for it (#484).
    pub fn list_fee_tokens(env: Env) -> Vec<Address> {
        storage::get_fee_tokens(&env)
    }

    /// Return the current custodied balance for `token` (decreases on withdrawal).
    pub fn token_balance(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_token_balance(&env, &token)
    }

    /// Return the per-token cumulative fees collected for `token` since deployment.
    pub fn get_cumulative_fees(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the global cumulative fees collected across all tokens since deployment.
    pub fn total_collected(env: Env) -> Result<i128, TreasuryError> {
        storage::get_total_collected(&env)
    }
}

/// Guard: reject operations that are not permitted under the current emergency
/// mode (Issue #662).
///
/// `allowed_modes` specifies the set of modes under which the guarded operation
/// is permitted. If the current mode is not in this set, the call is rejected
/// with [`TreasuryError::EmergencyModeActive`].
fn require_emergency_mode_allows(
    env: &Env,
    allowed_modes: &[storage::EmergencyMode],
) -> Result<(), TreasuryError> {
    let current = storage::get_emergency_mode(env);
    if !allowed_modes.contains(&current) {
        return Err(TreasuryError::EmergencyModeActive);
    }
    Ok(())
}
