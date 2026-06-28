//! Persistent storage helpers for the Vatix Treasury contract.

use soroban_sdk::{contracttype, Address, Env};

use crate::error::TreasuryError;

// ── Version ───────────────────────────────────────────────────────────────────

/// Bump this whenever the storage layout changes in a breaking way.
/// `initialize()` writes this value; every storage read asserts it.
pub const STORAGE_VERSION: u32 = 1;

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// Written at initialize; read by `assert_version` to guard all data reads.
    StorageVersion,
    /// The address that can call `withdraw_fees` and other admin operations.
    Admin,
    /// The single market contract address allowed to call `collect_fee`.
    AuthorizedMarket,
    /// Current custodied balance for a specific token (decreases on withdrawal).
    TokenBalance(Address),
    /// Monotonically increasing cumulative fees collected per token (never decreases).
    CumulativeFees(Address),
    /// Global monotone counter: total of all fees ever collected across all tokens.
    TotalCollected,
}

// ── Version helpers ───────────────────────────────────────────────────────────

pub fn set_version(env: &Env) {
    env.storage()
        .instance()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn get_version(env: &Env) -> Option<u32> {
    env.storage().instance().get(&StorageKey::StorageVersion)
}

/// Returns `Err(UpgradeRequired)` when the on-chain version is absent or
/// does not match `STORAGE_VERSION`.
pub fn assert_version(env: &Env) -> Result<(), TreasuryError> {
    let on_chain: Option<u32> = env.storage().instance().get(&StorageKey::StorageVersion);
    if on_chain != Some(STORAGE_VERSION) {
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

// ── Authorized market ─────────────────────────────────────────────────────────

pub fn get_authorized_market(env: &Env) -> Result<Address, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::AuthorizedMarket)
        .expect("treasury not initialized"))
}

pub fn set_authorized_market(env: &Env, market: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::AuthorizedMarket, market);
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

// ── Global cumulative (sum across all tokens, monotone) ───────────────────────

pub fn get_total_collected(env: &Env) -> Result<i128, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::TotalCollected)
        .unwrap_or(0i128))
}

pub fn set_total_collected(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalCollected, &amount);
}
