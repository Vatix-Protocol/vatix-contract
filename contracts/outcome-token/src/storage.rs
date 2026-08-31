use crate::error::ContractError;
use crate::types::{OutcomeTokenConfig, PendingAddressChange, TokenKind};
use soroban_sdk::{contracttype, Address, Env};

/// Bump this constant whenever the outcome-token storage layout changes in a
/// breaking way. `initialize()` writes this value; state-mutating entry
/// points assert it via [`assert_version`] before touching storage — mirrors
/// the pattern already used by `contracts/market/src/storage.rs` and
/// `contracts/treasury/src/storage.rs` (Issue #696). Previously this crate
/// had no storage-version guard at all, so a partial cross-contract upgrade
/// could silently brick `mint`/`burn` against a stale on-chain layout instead
/// of failing closed with `UpgradeRequired`.
///
/// ## Version history
/// - **v1:** Initial versioned storage layout (Issue #696). No layout change
///   from the pre-versioning schema — this just adds the guard itself.
pub const STORAGE_VERSION: u32 = 1;

#[contracttype]
pub enum StorageKey {
    /// Written by `initialize`; used to detect stale or uninitialized deployments.
    StorageVersion,
    Config,
    /// Per-market, per-user, per-side balance.
    Balance(u32, Address, TokenKind),
    /// Per-market, per-side total supply.
    TotalSupply(u32, TokenKind),
    /// A proposed `market_contract` (mint authority) rotation awaiting its
    /// timelock delay (Issue #691).
    PendingMarketContract,
    /// Stores whether the contract is administratively paused. When `true`,
    /// `mint`, `burn`, and `transfer` are all rejected.
    Paused,
}

// ── Version ───────────────────────────────────────────────────────────────

pub fn set_version(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn get_version(env: &Env) -> Option<u32> {
    env.storage().persistent().get(&StorageKey::StorageVersion)
}

/// Guard state-mutating entry points against a stale/pre-migration
/// deployment. Returns [`ContractError::UpgradeRequired`] when the on-chain
/// schema version does not match the compiled contract version.
pub fn assert_version(env: &Env) -> Result<(), ContractError> {
    if get_version(env) != Some(STORAGE_VERSION) {
        return Err(ContractError::UpgradeRequired);
    }
    Ok(())
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

pub fn get_pending_market_contract(env: &Env) -> Option<PendingAddressChange> {
    env.storage()
        .persistent()
        .get(&StorageKey::PendingMarketContract)
}

pub fn set_pending_market_contract(env: &Env, pending: &PendingAddressChange) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingMarketContract, pending);
}

pub fn clear_pending_market_contract(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingMarketContract);
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

// ── Pause flag ────────────────────────────────────────────────────────────

/// Returns `true` when the contract has been administratively paused.
/// Defaults to `false` when the key is absent (fresh deployment).
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

/// Set the pause flag. `true` blocks `mint`, `burn`, and `transfer`.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&StorageKey::Paused, &paused);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_assert_version_passes_when_current() {
        let env = Env::default();
        let contract_id = env.register(crate::OutcomeTokenContract, ());
        env.as_contract(&contract_id, || {
            set_version(&env);
            assert!(assert_version(&env).is_ok());
        });
    }

    #[test]
    fn test_assert_version_fails_when_stale() {
        let env = Env::default();
        let contract_id = env.register(crate::OutcomeTokenContract, ());
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_assert_version_fails_when_missing() {
        let env = Env::default();
        let contract_id = env.register(crate::OutcomeTokenContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }
}
