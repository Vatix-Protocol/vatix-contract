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
    env.storage().instance().set(&StorageKey::PendingAdmin, pending);
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
    env.storage().instance().get(&StorageKey::PendingMarketContract)
}

pub fn set_pending_market_contract(env: &Env, pending: &PendingAddressChange) {
    env.storage().instance().set(&StorageKey::PendingMarketContract, pending);
}

pub fn clear_pending_market_contract(env: &Env) {
    env.storage().instance().remove(&StorageKey::PendingMarketContract);
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

// ── Fee token registry (#484: multi-token fee collection support) ────────────

/// Return every distinct token mint that has ever had a fee collected for it.
pub fn get_fee_tokens(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::FeeTokens)
        .unwrap_or_else(|| Vec::new(env))
}

/// Record `token` in the fee-token registry if it hasn't been seen before.
/// Idempotent: re-registering an already-known token is a no-op.
pub fn register_fee_token(env: &Env, token: &Address) {
    let mut tokens = get_fee_tokens(env);
    if !tokens.contains(token) {
        tokens.push_back(token.clone());
        env.storage()
            .instance()
            .set(&StorageKey::FeeTokens, &tokens);
    }
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

// ── Pause flag ────────────────────────────────────────────────────────────────

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::Paused, &paused);
}

// ── Stakeholder revenue share (#485) ──────────────────────────────────────────

/// Return the configured `(stakeholder, share_bps)` list, or an empty list if
/// `set_stakeholders` has never been called.
pub fn get_stakeholders(env: &Env) -> Result<Vec<(Address, u32)>, TreasuryError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .instance()
        .get(&StorageKey::Stakeholders)
        .unwrap_or_else(|| Vec::new(env)))
}

pub fn set_stakeholders(env: &Env, stakeholders: &Vec<(Address, u32)>) {
    env.storage()
        .instance()
        .set(&StorageKey::Stakeholders, stakeholders);
}

/// A proposed stakeholder list awaiting its timelock delay (Issue #689), if any.
pub fn get_pending_stakeholders(env: &Env) -> Option<PendingStakeholders> {
    env.storage().instance().get(&StorageKey::PendingStakeholders)
}

pub fn set_pending_stakeholders(env: &Env, pending: &PendingStakeholders) {
    env.storage()
        .instance()
        .set(&StorageKey::PendingStakeholders, pending);
}

pub fn clear_pending_stakeholders(env: &Env) {
    env.storage()
        .instance()
        .remove(&StorageKey::PendingStakeholders);
}

// ── Emergency Mode (Issue #662) ─────────────────────────────────────────────

/// Mirrored emergency mode coordinated with the Market contract. Defaults to
/// `Normal` when never explicitly set. Only the admin may change this value.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmergencyMode {
    Normal,
    TradingHalted,
    SettleOnly,
    GlobalFreeze,
}

/// Return the current mirrored emergency mode. Defaults to `Normal` when unset.
pub fn get_emergency_mode(env: &Env) -> EmergencyMode {
    env.storage()
        .instance()
        .get(&StorageKey::EmergencyMode)
        .unwrap_or(EmergencyMode::Normal)
}

/// Set the mirrored emergency mode. Only the admin may call this (enforced in
/// `lib.rs`). Operators should keep this value in sync with the Market and
/// Resolution contracts for coordinated behaviour.
pub fn set_emergency_mode(env: &Env, mode: &EmergencyMode) {
    env.storage()
        .instance()
        .set(&StorageKey::EmergencyMode, mode);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup_versioned(env: &Env) -> Address {
        let contract_id = env.register(crate::TreasuryContract, ());
        env.as_contract(&contract_id, || set_version(env));
        contract_id
    }

    #[test]
    fn test_assert_version_passes_when_current() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        env.as_contract(&contract_id, || {
            assert!(assert_version(&env).is_ok());
        });
    }

    #[test]
    fn test_assert_version_fails_when_stale() {
        let env = Env::default();
        let contract_id = env.register(crate::TreasuryContract, ());
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(TreasuryError::UpgradeRequired));
        });
    }

    #[test]
    fn test_assert_version_fails_when_missing() {
        let env = Env::default();
        let contract_id = env.register(crate::TreasuryContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(TreasuryError::UpgradeRequired));
        });
    }

    #[test]
    fn test_admin_round_trip() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert!(!has_admin(&env));
            set_admin(&env, &admin);
            assert!(has_admin(&env));
            assert_eq!(get_admin(&env).unwrap(), admin);
        });
    }

    #[test]
    fn test_authorized_markets_empty_by_default() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        env.as_contract(&contract_id, || {
            let markets = get_authorized_markets(&env);
            assert_eq!(markets.len(), 0);
        });
    }

    #[test]
    fn test_authorized_markets_round_trip() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let market1 = Address::generate(&env);
        let market2 = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let mut markets = soroban_sdk::vec![&env, market1.clone(), market2.clone()];
            set_authorized_markets(&env, &markets);
            assert!(is_authorized_market(&env, &market1));
            assert!(is_authorized_market(&env, &market2));
            assert_eq!(get_authorized_market(&env).unwrap(), market1);
            // Remove first market
            markets = soroban_sdk::vec![&env, market2.clone()];
            set_authorized_markets(&env, &markets);
            assert!(!is_authorized_market(&env, &market1));
            assert!(is_authorized_market(&env, &market2));
        });
    }

    #[test]
    fn test_token_balance_defaults_to_zero() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let token = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(get_token_balance(&env, &token).unwrap(), 0);
        });
    }

    #[test]
    fn test_token_balance_round_trip() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let token = Address::generate(&env);
        env.as_contract(&contract_id, || {
            set_token_balance(&env, &token, 1_000_000);
            assert_eq!(get_token_balance(&env, &token).unwrap(), 1_000_000);
        });
    }

    #[test]
    fn test_cumulative_fees_defaults_to_zero() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let token = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(get_cumulative_fees(&env, &token).unwrap(), 0);
        });
    }

    #[test]
    fn test_fee_tokens_registry_is_idempotent() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        let token = Address::generate(&env);
        env.as_contract(&contract_id, || {
            register_fee_token(&env, &token);
            register_fee_token(&env, &token); // idempotent
            assert_eq!(get_fee_tokens(&env).len(), 1);
        });
    }

    #[test]
    fn test_pause_flag_defaults_to_false() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        env.as_contract(&contract_id, || {
            assert!(!is_paused(&env));
        });
    }

    #[test]
    fn test_pause_flag_round_trip() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        env.as_contract(&contract_id, || {
            set_paused(&env, true);
            assert!(is_paused(&env));
            set_paused(&env, false);
            assert!(!is_paused(&env));
        });
    }

    #[test]
    fn test_total_collected_defaults_to_zero() {
        let env = Env::default();
        let contract_id = setup_versioned(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(get_total_collected(&env).unwrap(), 0);
        });
    }
}
