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
        if storage::is_paused(&env) {
            return Err(ContractError::ContractPaused);
        }
        // Storage-version guard (Issue #696): a stale/partially-upgraded
        // deployment must fail closed here rather than let `mint` write
        // balances/supply under a storage layout the compiled contract no
        // longer understands.
        storage::assert_version(&env)?;
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
        if storage::is_paused(&env) {
            return Err(ContractError::ContractPaused);
        }
        storage::assert_version(&env)?;
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

    /// Transfer `amount` tokens of `kind` from `from` to `to` within `market_id`.
    ///
    /// Before resolution, positions can only change through [`mint`]/[`burn`]
    /// driven by the market contract itself, so a direct peer-to-peer
    /// transfer is rejected with [`ContractError::MarketNotResolved`] — this
    /// keeps a market's price-discovery phase free of secondary-market
    /// transfers of unsettled claims.
    ///
    /// Once the market has resolved, peer-to-peer transfer is *also*
    /// rejected — this time with
    /// [`ContractError::TransferBlockedAfterResolve`] — because the market
    /// contract's settlement logic (`settlement.rs`) pays out against the
    /// `Position` record it stores for the *original* depositor's address,
    /// not against whichever address currently holds the outcome-token
    /// balance (see `reconciliation.rs`, which only reconciles a single
    /// user's own Position/token divergence and cannot repair a transfer to
    /// a *different* address). Allowing a transfer here would let a holder
    /// move their balance to a fresh address post-resolution while the
    /// original Position still entitles them to the full payout — the same
    /// claim paid out twice (Issue #690). Closing that gap by blocking the
    /// transfer entirely is far simpler and safer than trying to atomically
    /// migrate the `Position` record across a cross-contract call.
    pub fn transfer(
        env: Env,
        market_id: u32,
        from: Address,
        _to: Address,
        _kind: TokenKind,
        amount: i128,
    ) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        from.require_auth();
        if storage::is_paused(&env) {
            return Err(ContractError::ContractPaused);
        }
        storage::assert_version(&env)?;

        let config = storage::get_config(&env);
        let status: MarketStatus = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "get_market_status"),
            soroban_sdk::vec![&env, market_id.into_val(&env)],
        );
        if status == MarketStatus::Resolved {
            return Err(ContractError::TransferBlockedAfterResolve);
        }
        Err(ContractError::MarketNotResolved)
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
