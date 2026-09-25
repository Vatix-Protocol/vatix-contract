// Issue #765: Required no_std attribute for Soroban WASM contract execution
#![no_std]
#![warn(clippy::all)]

//! # Outcome Token Contract
//!
//! Manages per-market, per-side (YES/NO) outcome tokens for the Vatix protocol.
//! Only the registered market contract may mint or burn tokens. Balances and
//! total supplies are tracked per market, per user, per token kind.
//!
//! ## Storage layout
//!
//! | Key                                      | Type      | Description                                 |
//! |------------------------------------------|-----------|---------------------------------------------|
//! | `StorageVersion`                         | `u32`     | Schema version guard (#696)                 |
//! | `Config`                                 | `OutcomeTokenConfig` | Admin and market contract addresses |
//! | `Balance(u32, Address, TokenKind)`       | `i128`    | Per-user, per-market, per-side token balance|
//! | `TotalSupply(u32, TokenKind)`            | `i128`    | Per-market, per-side total token supply     |

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{MarketStatus, OutcomeTokenConfig, TokenKind};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, String, Symbol};

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
        // #790: empty name or symbol breaks SAC-compatible wallets and indexers.
        if name.is_empty() || symbol.is_empty() {
            return Err(ContractError::EmptyMetadata);
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
        storage::set_version(&env);
        Ok(())
    }

    pub fn get_config(env: Env) -> OutcomeTokenConfig {
        storage::get_config(&env)
    }

    /// Delay, in seconds, an admin-proposed `market_contract` (mint/burn
    /// authority) rotation must wait before it can be applied via
    /// [`Self::execute_market_contract`] (Issue #691). Matches the market
    /// contract's own address-change timelock so the mint authority cannot
    /// rotate instantly while the market side is already timelocked.
    pub const MARKET_CONTRACT_TIMELOCK_SECONDS: u64 = 172_800;

    /// Propose rotating the market contract address allowed to mint/burn
    /// tokens, subject to a timelock (Issue #691). Admin only. The change
    /// does not apply immediately — call [`Self::execute_market_contract`]
    /// once [`Self::MARKET_CONTRACT_TIMELOCK_SECONDS`] have elapsed.
    pub fn propose_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        let effective_at = env.ledger().timestamp() + Self::MARKET_CONTRACT_TIMELOCK_SECONDS;
        storage::set_pending_market_contract(
            &env,
            &crate::types::PendingAddressChange {
                new_address: market_contract.clone(),
                effective_at,
            },
        );
        events::emit_market_contract_proposed(&env, &market_contract, effective_at);
        Ok(())
    }

    /// Apply a previously-proposed `market_contract` rotation once its
    /// timelock has elapsed (Issue #691). Callable by anyone — the timelock
    /// itself is the access control.
    pub fn execute_market_contract(env: Env) -> Result<Address, ContractError> {
        let pending = storage::get_pending_market_contract(&env)
            .ok_or(ContractError::NoPendingMarketContractChange)?;
        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }
        let mut config = storage::get_config(&env);
        config.market_contract = pending.new_address.clone();
        storage::set_config(&env, &config);
        storage::clear_pending_market_contract(&env);
        events::emit_market_contract_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    /// Cancel a pending `market_contract` rotation before it takes effect.
    pub fn cancel_market_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        storage::clear_pending_market_contract(&env);
        Ok(())
    }

    /// Return the currently pending `market_contract` rotation, if any
    /// (Issue #691).
    pub fn get_pending_market_contract(env: Env) -> Option<crate::types::PendingAddressChange> {
        storage::get_pending_market_contract(&env)
    }

    /// Pause the contract, blocking `mint`, `burn`, and `transfer`.
    ///
    /// Only the stored admin may call this. Once paused, all three token
    /// mutation entrypoints reject with [`ContractError::ContractPaused`]
    /// until the admin calls [`Self::unpause`].
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] — `admin` is not the stored admin.
    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        storage::set_paused(&env, true);
        events::emit_contract_paused(&env, &admin);
        Ok(())
    }

    /// Unpause the contract, restoring normal token operations.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] — `admin` is not the stored admin.
    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        storage::set_paused(&env, false);
        events::emit_contract_unpaused(&env, &admin);
        Ok(())
    }

    /// Return whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Update the SAC metadata (name and symbol). Admin only.
    ///
    /// #867: SAC metadata invariants are enforced here so untrusted clients
    /// cannot bypass metadata policy. The call is idempotent: re-submitting
    /// the current `name`/`symbol` is a no-op that succeeds without emitting
    /// a change event. Empty values are rejected fail-closed.
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] — `admin` is not the stored admin.
    /// - [`ContractError::EmptyMetadata`] — `name` or `symbol` is empty.
    pub fn set_metadata(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let mut config = storage::get_config(&env);
        if admin != config.admin {
            return Err(ContractError::Unauthorized);
        }
        // #790: empty name or symbol breaks SAC-compatible wallets and indexers.
        if name.is_empty() || symbol.is_empty() {
            return Err(ContractError::EmptyMetadata);
        }
        // #867: idempotency — no-op when metadata is unchanged.
        if config.name == name && config.symbol == symbol {
            return Ok(());
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

    /// SAC-compatible decimals. Compile-time constant (7) per the outcome
    /// token README; never stored, so it cannot drift from the invariant.
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}
