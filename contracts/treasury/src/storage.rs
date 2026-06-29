//! Persistent storage helpers for the Vatix Treasury contract.

use soroban_sdk::{contracttype, Address, Env, Vec};

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// The address that can call `withdraw_fees` and other admin operations.
    Admin,
    /// The set of market contract addresses allowed to call `collect_fee`.
    AuthorizedMarkets,
    /// Current custodied balance for a specific token (decreases on withdrawal).
    TokenBalance(Address),
    /// Monotonically increasing cumulative fees collected per token (never decreases).
    CumulativeFees(Address),
    /// Global monotone counter: total of all fees ever collected across all tokens.
    TotalCollected,
}

// ── Admin ─────────────────────────────────────────────────────────────────────

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

// ── Authorized markets registry ───────────────────────────────────────────────

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

pub fn is_authorized_market(env: &Env, market: &Address) -> bool {
    get_authorized_markets(env).contains(market)
}

// ── Token balance (current, decreasable on withdrawal) ────────────────────────

pub fn get_token_balance(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::TokenBalance(token.clone()))
        .unwrap_or(0i128)
}

pub fn set_token_balance(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TokenBalance(token.clone()), &amount);
}

// ── Cumulative fees (monotone historical counter per token) ───────────────────

pub fn get_cumulative_fees(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::CumulativeFees(token.clone()))
        .unwrap_or(0i128)
}

pub fn set_cumulative_fees(env: &Env, token: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::CumulativeFees(token.clone()), &amount);
}

// ── Global cumulative (sum across all tokens, monotone) ───────────────────────

pub fn get_total_collected(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&StorageKey::TotalCollected)
        .unwrap_or(0i128)
}

pub fn set_total_collected(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&StorageKey::TotalCollected, &amount);
}
