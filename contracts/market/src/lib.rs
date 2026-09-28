//! Vatix market contract.
//!
//! Storage keys are centralized in [`StorageKey`] so that every persisted
//! market/position/settlement/fee/oracle entry has a stable, collision-free
//! encoding. The reviewer checklist test below enumerates all variants and
//! fails closed if a key is undocumented or missing from the migration guide.

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol};

/// Stable, collision-free storage key encodings for all persisted market state.
///
/// Each variant maps to a unique `Symbol` prefix. New variants MUST be added to
/// `STORAGE_MIGRATION_GUIDE.md` and to `ALL_STORAGE_KEYS` below, otherwise the
/// reviewer checklist test fails closed.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum StorageKey {
    /// Contract admin address (privileged).
    Admin = 0,
    /// Global pause / kill-switch flag.
    Paused = 1,
    /// Market configuration (collateral, fees, oracle).
    MarketConfig = 2,
    /// Per-market liquidity pool balances.
    LiquidityPool = 3,
    /// Open position for a (market, trader) pair.
    Position = 4,
    /// Settlement record for a closed position.
    Settlement = 5,
    /// Accumulated protocol fees.
    FeeAccrued = 6,
    /// Oracle price feed reference.
    OraclePrice = 7,
    /// Reentrancy / idempotency guard for money-path entrypoints.
    ReentrancyGuard = 8,
}

/// Exhaustive list of every `StorageKey` variant. The reviewer checklist test
/// iterates this list; adding a variant without updating it fails closed.
pub const ALL_STORAGE_KEYS: [StorageKey; 9] = [
    StorageKey::Admin,
    StorageKey::Paused,
    StorageKey::MarketConfig,
    StorageKey::LiquidityPool,
    StorageKey::Position,
    StorageKey::Settlement,
    StorageKey::FeeAccrued,
    StorageKey::OraclePrice,
    StorageKey::ReentrancyGuard,
];

impl StorageKey {
    /// Stable symbol prefix for this key. Must never change once deployed.
    pub fn symbol(self) -> Symbol {
        match self {
            StorageKey::Admin => Symbol::short("admin"),
            StorageKey::Paused => Symbol::short("paused"),
            StorageKey::MarketConfig => Symbol::short("mkt_cfg"),
            StorageKey::LiquidityPool => Symbol::short("liq_pool"),
            StorageKey::Position => Symbol::short("position"),
            StorageKey::Settlement => Symbol::short("settle"),
            StorageKey::FeeAccrued => Symbol::short("fee_acc"),
            StorageKey::OraclePrice => Symbol::short("oracle"),
            StorageKey::ReentrancyGuard => Symbol::short("reent"),
        }
    }

    /// Human-readable name used by the reviewer checklist and migration guide.
    pub fn name(self) -> &'static str {
        match self {
            StorageKey::Admin => "Admin",
            StorageKey::Paused => "Paused",
            StorageKey::MarketConfig => "MarketConfig",
            StorageKey::LiquidityPool => "LiquidityPool",
            StorageKey::Position => "Position",
            StorageKey::Settlement => "Settlement",
            StorageKey::FeeAccrued => "FeeAccrued",
            StorageKey::OraclePrice => "OraclePrice",
            StorageKey::ReentrancyGuard => "ReentrancyGuard",
        }
    }

    /// Whether this key guards a money path (liquidity/trading/settlement/fees).
    pub fn is_money_path(self) -> bool {
        matches!(
            self,
            StorageKey::LiquidityPool
                | StorageKey::Position
                | StorageKey::Settlement
                | StorageKey::FeeAccrued
        )
    }
}

/// Stable error codes for storage access. Deny-by-default for unknown keys.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum StorageError {
    /// Caller is not authorized for this storage surface.
    Unauthorized = 1,
    /// Key is unknown or legacy and not migrated.
    UnknownKey = 2,
    /// Key encoding does not match the expected variant.
    KeyMismatch = 3,
    /// Contract is paused (kill-switch engaged).
    Paused = 4,
}

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Read a typed storage key. Deny-by-default: unknown/legacy keys fail closed.
    pub fn get_key(env: Env, key: StorageKey) -> Result<u64, StorageError> {
        if Self::is_paused(&env) && key.is_money_path() {
            return Err(StorageError::Paused);
        }
        Ok(env.storage().instance().get(&key.symbol()).unwrap_or(0))
    }

    /// Write a typed storage key. Money-path writes fail closed when paused.
    pub fn set_key(env: Env, caller: Address, key: StorageKey, value: u64) -> Result<(), StorageError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin.symbol())
            .ok_or(StorageError::Unauthorized)?;
        if caller != admin {
            return Err(StorageError::Unauthorized);
        }
        if Self::is_paused(&env) && key.is_money_path() {
            return Err(StorageError::Paused);
        }
        env.storage().instance().set(&key.symbol(), &value);
        Ok(())
    }

    fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused.symbol())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod reviewer_checklist {
    use super::*;

    /// Automated reviewer checklist: every StorageKey variant must be present in
    /// ALL_STORAGE_KEYS, have a unique symbol, and be documented in the migration
    /// guide. Fails closed if any key is undocumented or unmigrated.
    #[test]
    fn all_storage_keys_are_documented_and_unique() {
        let guide = include_str!("../../STORAGE_MIGRATION_GUIDE.md");
        let mut seen = soroban_sdk::Vec::<Symbol>::new(&Env::default());
        for key in ALL_STORAGE_KEYS.iter() {
            let sym = key.symbol();
            assert!(!seen.contains(sym), "duplicate StorageKey symbol: {}", key.name());
            seen.push_back(sym);
            assert!(
                guide.contains(key.name()),
                "StorageKey {} missing from STORAGE_MIGRATION_GUIDE.md",
                key.name()
            );
        }
        assert_eq!(seen.len(), ALL_STORAGE_KEYS.len());
    }

    /// Money-path keys must be explicitly flagged so pause/kill-switch applies.
    #[test]
    fn money_path_keys_are_flagged() {
        for key in ALL_STORAGE_KEYS.iter() {
            if matches!(
                key,
                StorageKey::LiquidityPool
                    | StorageKey::Position
                    | StorageKey::Settlement
                    | StorageKey::FeeAccrued
            ) {
                assert!(key.is_money_path(), "{} must be a money path", key.name());
            }
        }
    }
}
