//! Persistent storage helpers for the Vatix Treasury contract.

use soroban_sdk::{contracttype, Address, Env};

// ── Storage keys ──────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// The address that can call `withdraw_fees` and admin operations.
    Admin,
    /// The single market contract address allowed to call `collect_fee`.
    AuthorizedMarket,
    /// Cumulative fees collected, keyed by SAC token address.
    CumulativeFees(Address),
}

// ── Admin ─────────────────────────────────────────────────────────────────

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&StorageKey::Admin)
}

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&StorageKey::Admin)
        .expect("treasury not initialized")
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&StorageKey::Admin, admin);
}

// ── Authorized market ─────────────────────────────────────────────────────

pub fn get_authorized_market(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&StorageKey::AuthorizedMarket)
        .expect("treasury not initialized")
}

pub fn set_authorized_market(env: &Env, market: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::AuthorizedMarket, market);
}

// ── Cumulative fees ───────────────────────────────────────────────────────

/// Return the cumulative fees accumulated for a given token (0 if none yet).
pub fn get_cumulative_fees(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::CumulativeFees(token.clone()))
        .unwrap_or(0i128)
}

/// Overwrite the cumulative fee total for a token.
pub fn set_cumulative_fees(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::CumulativeFees(token.clone()), &amount);
}
