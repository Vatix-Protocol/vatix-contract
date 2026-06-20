use crate::types::{OutcomeTokenConfig, TokenKind};
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
pub enum StorageKey {
    Config,
    /// Per-market, per-user, per-side balance.
    Balance(u32, Address, TokenKind),
    /// Per-market, per-side total supply.
    TotalSupply(u32, TokenKind),
}

pub fn has_config(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Config)
}

pub fn get_config(env: &Env) -> OutcomeTokenConfig {
    env.storage()
        .persistent()
        .get(&StorageKey::Config)
        .expect("outcome-token config not set")
}

pub fn set_config(env: &Env, config: &OutcomeTokenConfig) {
    env.storage().persistent().set(&StorageKey::Config, config);
}

pub fn get_balance(env: &Env, market_id: u32, user: &Address, kind: &TokenKind) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::Balance(market_id, user.clone(), kind.clone()))
        .unwrap_or(0i128)
}

pub fn set_balance(env: &Env, market_id: u32, user: &Address, kind: &TokenKind, amount: i128) {
    env.storage().persistent().set(
        &StorageKey::Balance(market_id, user.clone(), kind.clone()),
        &amount,
    );
}

pub fn get_total_supply(env: &Env, market_id: u32, kind: &TokenKind) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::TotalSupply(market_id, kind.clone()))
        .unwrap_or(0i128)
}

pub fn set_total_supply(env: &Env, market_id: u32, kind: &TokenKind, supply: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TotalSupply(market_id, kind.clone()), &supply);
}
