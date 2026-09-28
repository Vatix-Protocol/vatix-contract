//! Persistent storage helpers for the Vatix Treasury contract.

use crate::error::TreasuryError;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Bump this constant whenever the treasury storage layout changes in a breaking way.
/// `initialize()` writes this value so that future migrations can detect stale deployments.
///
/// ## Version history
/// - **v3:** Added `EmergencyMode` for coordinated emergency mode (#662).
///   The variant was missing from `StorageKey` itself until #722 — see the
///   `EmergencyMode` variant's doc comment below and
///   `docs/treasury-storage.md`'s Reviewer Checklist for what that gap
///   looked like and how to catch it earlier next time.
/// - **v2:** Completed the multi-market `AuthorizedMarkets` registry
///   (`add_market`/`remove_market`/`list_markets`/`is_authorized_market`) and
///   added the `Stakeholders` fee-distribution list (#485).
/// - **v1:** Initial storage layout.
pub const STORAGE_VERSION: u32 = 3;

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// Written by `initialize`; used to detect stale or uninitialized deployments.
    StorageVersion,
    /// The address that can call `withdraw_fees` and other admin operations.
    Admin,
    /// The set of market contract addresses allowed to call `collect_fee`.
    AuthorizedMarkets,
    /// Current custodied balance for a specific token (decreases on withdrawal).
    TokenBalance(Address),
    /// Monotonically increasing cumulative fees collected per token (never decreases).
    CumulativeFees(Address),
    /// Global counter: total of all fees currently held across all tokens. Decreases on withdrawal.
    TotalCollected,
    /// When `true`, `collect_fee` and `withdraw_fees` are blocked until unpaused.
    Paused,
    /// Ordered list of `(stakeholder, share_bps)` pairs used by `distribute_fees`
    /// (#485). `share_bps` values must sum to exactly 10_000.
    Stakeholders,
    /// Registry of every distinct token mint that has ever had a fee routed
    /// through `collect_fee` (#484). Lets callers enumerate which tokens hold
    /// a balance without needing prior knowledge of the token address.
    FeeTokens,
    PendingMarketContract,
    PendingAdmin,
    /// A proposed stakeholder revenue-share list awaiting its timelock delay
    /// before it can take effect (Issue #689). Mirrors `PendingAdmin` /
    /// `PendingMarketContract` so the revenue split cannot be rewritten
    /// instantly.
    PendingStakeholders,
    /// Coordinated emergency mode mirrored with the Market/Resolution
    /// contracts (#662). Defaults to `EmergencyMode::Normal` when unset.
    ///
    /// This variant was missing from the enum even though
    /// `get_emergency_mode`/`set_emergency_mode` (below) referenced
    /// `StorageKey::EmergencyMode` since the mode was introduced — the crate
    /// has not compiled since that commit (#722). Restoring it here is the
    /// fix, not a new schema addition: `STORAGE_VERSION` was already bumped
    /// to `3` and the version-history comment above already documented this
    /// key as if it existed.
    EmergencyMode,
    /// When `true`, deposits into the treasury are blocked until deposits are
    /// explicitly re-enabled (#959). Distinct from `Paused` (which gates
    /// `collect_fee`/`withdraw_fees`) so operators can halt inbound liquidity
    /// without freezing outbound settlement. Defaults to `false` (deposits
    /// allowed) when unset, and is fail-closed: any read error is treated as
    /// paused by the caller.
    DepositsPaused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingAddressChange {
    pub new_address: Address,
    pub effective_at: u64,
}

/// A proposed stakeholder revenue-share list awaiting its timelock delay
/// (Issue #689).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingStakeholders {
    pub stakeholders: Vec<(Address, u32)>,
    pub effective_at: u64,
}

// ── Version ───────────────────────────────────────────────────────────────────

pub fn set_version(env: &Env) {
    env.storage()
        .instance()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn get_version(env: &Env) -> Option<u32> {
    env.storage().instance().get(&StorageKey::StorageVersion)
}

/// Guard every storage accessor against a stale/pre-migration deployment.
///
/// Returns [`TreasuryError::UpgradeRequired`] when the on-chain schema version
/// does not match the compiled contract version.
pub fn assert_version(env: &Env) -> Result<(), TreasuryError> {
    if get_version(env) != Some(STORAGE_VERSION) {
        return Err(TreasuryError::UpgradeRequired);
    }
    Ok(())
}

// ── Admin ─────────────────────────────────────────────────────────────────────

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&StorageKey::Admin)
}

pub fn get_admin(env: &Env) -> Result<Address, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .expect("treasury not initialized"))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

pub fn get_pending_admin(env: &Env) -> Option<PendingAddressChange> {
    env.storage().instance().get(&StorageKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, pending: &PendingAddressChange) {
    env.storage()
        .instance()
        .set(&StorageKey::PendingAdmin, pending);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&StorageKey::PendingAdmin);
}

// ── Authorized markets registry ───────────────────────────────────────────────
//
// Note: fixed alongside #484 (multi-token fee collection) since this file was
// touched for that change — `get_authorized_markets`/`is_authorized_market`
// previously referenced a non-existent singular `AuthorizedMarket` key.

/// Return the full list of markets currently authorized to call `collect_fee`.
///
/// Returns an empty list (rather than erroring) when nothing has been
/// registered yet, mirroring the market contract's `Vec`-storage convention.
pub fn get_authorized_markets(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::AuthorizedMarkets)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_authorized_markets(env: &Env, markets: &Vec<Address>) {
    env.storage()
        .instance()
        .set(&StorageKey::AuthorizedMarkets, markets);
}

/// Return the first registered market — kept for backwards compatibility with
/// the original single-market `market_contract()` getter.
pub fn get_authorized_market(env: &Env) -> Result<Address, TreasuryError> {
    let markets = get_authorized_markets(env);
    markets.get(0).ok_or(TreasuryError::NotInitialized)
}

pub fn is_authorized_market(env: &Env, market: &Address) -> bool {
    get_authorized_markets(env).contains(market)
}

pub fn get_pending_market_contract(env: &Env) -> Option<PendingAddressChange> {
    env.storage()
        .instance()
        .get(&StorageKey::PendingMarketContract)
}

pub fn set_pending_market_contract(env: &Env, pending: &PendingAddressChange) {
    env.storage()
        .instance()
        .set(&StorageKey::PendingMarketContract, pending);
}

pub fn clear_pending_market_contract(env: &Env) {
    env.storage()
        .instance()
        .remove(&StorageKey::PendingMarketContract);
}

// ── Deposit pause (Issue #959) ────────────────────────────────────────────────
//
// Fail-closed gate for the inbound money path. `Paused` already blocks
// `collect_fee`/`withdraw_fees`; `DepositsPaused` is a separate switch so
// operators can halt deposits without freezing outbound settlement. Reads
// default to `false` (deposits allowed) only when the key is genuinely absent;
// callers must treat any error as paused.

/// Returns `true` when deposits are currently paused.
///
/// Defaults to `false` (deposits allowed) when the flag has never been set,
/// preserving pre-#959 behavior for existing deployments.
pub fn is_deposits_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::DepositsPaused)
        .unwrap_or(false)
}

/// Set the deposit-pause flag. Idempotent: writing the same value twice is a
/// no-op from the caller's perspective and safe against replayed requests.
pub fn set_deposits_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::DepositsPaused, &paused);
}

/// Fail-closed guard for the deposit path.
///
/// Returns [`TreasuryError::DepositsPaused`] when deposits are paused so the
/// caller rejects the write before any balance mutation occurs.
pub fn assert_deposits_not_paused(env: &Env) -> Result<(), TreasuryError> {
    if is_deposits_paused(env) {
        return Err(TreasuryError::DepositsPaused);
    }
    Ok(())
}

// ── Token balance (current, decreasable on withdrawal) ────────────────────────

pub fn get_token_balance(env: &Env, token: &Address) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::TokenBalance(token.clone()))
        .unwrap_or(0i128))
}

pub fn set_token_balance(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TokenBalance(token.clone()), &amount);
}

// ── Cumulative fees (monotone historical counter per token) ───────────────────

pub fn get_cumulative_fees(env: &Env, token: &Address) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::CumulativeFees(token.clone()))
        .unwrap_or(0i128))
}

pub fn set_cumulative_fees(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::CumulativeFees(token.clone()), &amount);
}
