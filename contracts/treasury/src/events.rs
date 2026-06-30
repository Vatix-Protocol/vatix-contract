//! Event emission helpers for the Vatix Treasury contract.
//!
//! # Index-friendly topic naming (Issue #389)
//!
//! Event struct names follow the `{Noun}{Verb}` PascalCase pattern. Soroban
//! converts them to snake_case topics automatically, producing clean,
//! indexer-friendly topic strings without redundant `_event` suffixes.
//!
//! | Struct                  | Topic symbol                       |
//! |-------------------------|------------------------------------|
//! | `TreasuryInitialized`   | `treasury_initialized`             |
//! | `FeeCollected`          | `fee_collected`                    |
//! | `FeesWithdrawn`         | `fees_withdrawn`                   |
//! | `AdminTransferred`      | `admin_transferred`                |
//! | `MarketContractUpdated` | `market_contract_updated`          |

use soroban_sdk::{contractevent, Address, Env};

// ── Initialization ────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryInitialized {
    #[topic]
    pub admin: Address,
    #[topic]
    pub market_contract: Address,
    pub initialized_at: u64,
}

pub fn emit_treasury_initialized(env: &Env, admin: &Address, market_contract: &Address) {
    TreasuryInitialized {
        admin: admin.clone(),
        market_contract: market_contract.clone(),
        initialized_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Fee collection ────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollected {
    /// Market that generated the fee.
    #[topic]
    pub market_id: u32,
    /// Token in which the fee was paid.
    #[topic]
    pub token: Address,
    /// Fee collected in this call (stroops).
    pub fee_amount: i128,
    /// Current custodied balance of `token` after this call.
    pub new_token_balance: i128,
    /// Cumulative fees for `token` after this call (monotone).
    pub new_cumulative_fees: i128,
}

pub fn emit_fee_collected(
    env: &Env,
    market_id: u32,
    token: &Address,
    fee_amount: i128,
    new_token_balance: i128,
    new_cumulative_fees: i128,
) {
    FeeCollected {
        market_id,
        token: token.clone(),
        fee_amount,
        new_token_balance,
        new_cumulative_fees,
    }
    .publish(env);
}

// ── Admin withdrawal ──────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeesWithdrawn {
    #[topic]
    pub token: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
    pub remaining_token_balance: i128,
}

pub fn emit_fees_withdrawn(
    env: &Env,
    token: &Address,
    to: &Address,
    amount: i128,
    remaining_token_balance: i128,
) {
    FeesWithdrawn {
        token: token.clone(),
        to: to.clone(),
        amount,
        remaining_token_balance,
    }
    .publish(env);
}

// ── Admin transfer ────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferred {
    #[topic]
    pub old_admin: Address,
    #[topic]
    pub new_admin: Address,
    pub transferred_at: u64,
}

pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    AdminTransferred {
        old_admin: old_admin.clone(),
        new_admin: new_admin.clone(),
        transferred_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Market contract rotation ──────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractUpdated {
    #[topic]
    pub old_market_contract: Address,
    #[topic]
    pub new_market_contract: Address,
}

pub fn emit_market_contract_updated(
    env: &Env,
    old_market_contract: &Address,
    new_market_contract: &Address,
) {
    MarketContractUpdated {
        old_market_contract: old_market_contract.clone(),
        new_market_contract: new_market_contract.clone(),
    }
    .publish(env);
}

// ── Pause ─────────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryPausedEvent {
    #[topic]
    pub admin: Address,
    pub paused_at: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryUnpausedEvent {
    #[topic]
    pub admin: Address,
    pub unpaused_at: u64,
}

pub fn emit_treasury_paused(env: &Env, admin: &Address) {
    TreasuryPausedEvent {
        admin: admin.clone(),
        paused_at: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_treasury_unpaused(env: &Env, admin: &Address) {
    TreasuryUnpausedEvent {
        admin: admin.clone(),
        unpaused_at: env.ledger().timestamp(),
    }
    .publish(env);
}
