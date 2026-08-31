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
//! | `MarketAdded`           | `market_added`                     |
//! | `MarketRemoved`         | `market_removed`                   |
//! | `StakeholdersUpdated`   | `stakeholders_updated`             |
//! | `FeesDistributed`       | `fees_distributed`                 |
//! | `EmergencyModeChanged`  | `emergency_mode_changed`           |

use soroban_sdk::{contractevent, Address, Env, Vec};

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

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposed {
    #[topic]
    pub old_admin: Address,
    #[topic]
    pub new_admin: Address,
    pub effective_at: u64,
}

pub fn emit_admin_transfer_proposed(env: &Env, old_admin: &Address, new_admin: &Address, effective_at: u64) {
    AdminTransferProposed {
        old_admin: old_admin.clone(),
        new_admin: new_admin.clone(),
        effective_at,
    }.publish(env);
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

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractProposed {
    #[topic]
    pub new_market_contract: Address,
    pub effective_at: u64,
}

pub fn emit_market_contract_proposed(env: &Env, new_market_contract: &Address, effective_at: u64) {
    MarketContractProposed {
        new_market_contract: new_market_contract.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractSet {
    #[topic]
    pub new_market_contract: Address,
    pub set_at: u64,
}

pub fn emit_market_contract_set(env: &Env, new_market_contract: &Address) {
    MarketContractSet {
        new_market_contract: new_market_contract.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

// ── Market registry (add/remove) ──────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketAdded {
    #[topic]
    pub market_contract: Address,
}

pub fn emit_market_added(env: &Env, market_contract: &Address) {
    MarketAdded { market_contract: market_contract.clone() }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketRemoved {
    #[topic]
    pub market_contract: Address,
}

pub fn emit_market_removed(env: &Env, market_contract: &Address) {
    MarketRemoved { market_contract: market_contract.clone() }.publish(env);
}

// ── Stakeholder fee distribution (#485) ───────────────────────────────────────

/// Emitted when an admin proposes a new stakeholder revenue-share list
/// (Issue #689). Carries the full `(stakeholder, share_bps)` payload — as
/// parallel `stakeholders` / `shares_bps` vectors — so an off-chain indexer
/// can reconstruct the proposed split without a separate on-chain read. Does
/// not take effect until `effective_at` and `execute_stakeholders` is called.
#[contractevent]
#[derive(Clone, Debug)]
pub struct StakeholdersProposed {
    pub stakeholders: Vec<Address>,
    pub shares_bps: Vec<u32>,
    pub effective_at: u64,
}

pub fn emit_stakeholders_proposed(
    env: &Env,
    stakeholders: &Vec<Address>,
    shares_bps: &Vec<u32>,
    effective_at: u64,
) {
    StakeholdersProposed {
        stakeholders: stakeholders.clone(),
        shares_bps: shares_bps.clone(),
        effective_at,
    }
    .publish(env);
}

/// Emitted once a proposed stakeholder list's timelock has elapsed and it is
/// applied. Carries the full payload (not just a count) for indexer
/// reconstruction (Issue #689).
#[contractevent]
#[derive(Clone, Debug)]
pub struct StakeholdersUpdated {
    pub stakeholder_count: u32,
    pub stakeholders: Vec<Address>,
    pub shares_bps: Vec<u32>,
    pub updated_at: u64,
}

pub fn emit_stakeholders_updated(env: &Env, stakeholders: &Vec<Address>, shares_bps: &Vec<u32>) {
    StakeholdersUpdated {
        stakeholder_count: stakeholders.len(),
        stakeholders: stakeholders.clone(),
        shares_bps: shares_bps.clone(),
        updated_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeesDistributed {
    #[topic]
    pub token: Address,
    pub distributed_amount: i128,
    pub remaining_token_balance: i128,
    pub stakeholder_count: u32,
    pub distributed_at: u64,
}

/// Emitted once per `distribute_fees` call, summarizing the payout across all
/// configured stakeholders for `token`.
pub fn emit_fees_distributed(
    env: &Env,
    token: &Address,
    distributed_amount: i128,
    remaining_token_balance: i128,
    stakeholder_count: u32,
) {
    FeesDistributed {
        token: token.clone(),
        distributed_amount,
        remaining_token_balance,
        stakeholder_count,
        distributed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Pause ─────────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryPaused {
    #[topic]
    pub admin: Address,
    pub paused_at: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryUnpaused {
    #[topic]
    pub admin: Address,
    pub unpaused_at: u64,
}

pub fn emit_treasury_paused(env: &Env, admin: &Address) {
    TreasuryPaused {
        admin: admin.clone(),
        paused_at: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_treasury_unpaused(env: &Env, admin: &Address) {
    TreasuryUnpaused {
        admin: admin.clone(),
        unpaused_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Emergency mode (Issue #662) ──────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryEmergencyModeChanged {
    #[topic]
    pub new_mode: crate::storage::EmergencyMode,
    pub admin: Address,
    pub changed_at: u64,
}

pub fn emit_emergency_mode_changed(env: &Env, new_mode: &crate::storage::EmergencyMode, admin: &Address) {
    TreasuryEmergencyModeChanged {
        new_mode: new_mode.clone(),
        admin: admin.clone(),
        changed_at: env.ledger().timestamp(),
    }
    .publish(env);
}
