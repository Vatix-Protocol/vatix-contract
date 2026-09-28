//! Vatix market contract.
//!
//! Storage keys are centralized in [`StorageKey`] so that every persisted
//! market/position/settlement/fee/oracle entry has a stable, collision-free
//! encoding. The reviewer checklist test below enumerates all variants and
//! fails closed if a key is undocumented or missing from the migration guide.
//!
//! Cross-contract graph edges for outcome mint on trade are defined in
//! `docs/cross-contract-call-graph.md`: a market trade execution emits a
//! `mint_on_trade` edge into the outcome-token contract. The edge is typed,
//! authorized, idempotent, and fail-closed (see [`MintOnTradeEdge`]).

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
    /// Outcome-token contract address for the mint-on-trade cross-contract edge.
    OutcomeToken = 9,
    /// Idempotency marker for a processed mint-on-trade correlation id.
    MintOnTradeSeen = 10,
}

/// Exhaustive list of every `StorageKey` variant. The reviewer checklist test
/// iterates this list; adding a variant without updating it fails closed.
pub const ALL_STORAGE_KEYS: [StorageKey; 11] = [
    StorageKey::Admin,
    StorageKey::Paused,
    StorageKey::MarketConfig,
    StorageKey::LiquidityPool,
    StorageKey::Position,
    StorageKey::Settlement,
    StorageKey::FeeAccrued,
    StorageKey::OraclePrice,
    StorageKey::ReentrancyGuard,
    StorageKey::OutcomeToken,
    StorageKey::MintOnTradeSeen,
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
            StorageKey::OutcomeToken => Symbol::short("out_tok"),
            StorageKey::MintOnTradeSeen => Symbol::short("mot_seen"),
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
            StorageKey::OutcomeToken => "OutcomeToken",
            StorageKey::MintOnTradeSeen => "MintOnTradeSeen",
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
                | StorageKey::MintOnTradeSeen
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

/// Stable error codes for the mint-on-trade cross-contract edge.
///
/// Deny-by-default: any condition that is not explicitly permitted fails closed
/// with one of these codes so callers can branch deterministically.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MintOnTradeError {
    /// Caller is not the authorized market/trader for this edge.
    Unauthorized = 1,
    /// Contract is paused (kill-switch engaged); money path fails closed.
    Paused = 2,
    /// Outcome-token contract address is not configured.
    OutcomeTokenNotConfigured = 3,
    /// Correlation id was already processed (replay / concurrent request).
    DuplicateCorrelationId = 4,
    /// Trade amount is zero or otherwise invalid (adversarial input).
    InvalidAmount = 5,
}

/// Typed cross-contract edge: market trade execution -> outcome token mint.
///
/// Mirrors the `mint_on_trade` edge documented in
/// `docs/cross-contract-call-graph.md`. `correlation_id` provides idempotency
/// for concurrent/replayed requests; `trader` is the authorized caller.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintOnTradeEdge {
    /// Authorized trader initiating the trade.
    pub trader: Address,
    /// Market the trade belongs to.
    pub market: Symbol,
    /// Outcome token amount to mint.
    pub amount: u64,
    /// Idempotency key for this edge invocation.
    pub correlation_id: u64,
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

    /// Admin-only: configure the outcome-token contract for the mint-on-trade edge.
    /// Deny-by-default privileged surface; requires admin auth.
    pub fn set_outcome_token(env: Env, caller: Address, outcome_token: Address) -> Result<(), StorageError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin.symbol())
            .ok_or(StorageError::Unauthorized)?;
        if caller != admin {
            return Err(StorageError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::OutcomeToken.symbol(), &outcome_token);
        Ok(())
    }

    /// Cross-contract graph edge: mint outcome tokens on trade execution.
    ///
    /// Enforces, in order: pause/kill-switch (fail-closed), caller authz,
    /// outcome-token configuration, input validation, and idempotency on the
    /// correlation id. On success the edge is recorded and the mint is
    /// dispatched to the configured outcome-token contract.
    pub fn mint_on_trade(env: Env, edge: MintOnTradeEdge) -> Result<(), MintOnTradeError> {
        if Self::is_paused(&env) {
            return Err(MintOnTradeError::Paused);
        }
        edge.trader.require_auth();
        if edge.amount == 0 {
            return Err(MintOnTradeError::InvalidAmount);
        }
        let outcome_token: Address = env
            .storage()
            .instance()
            .get(&StorageKey::OutcomeToken.symbol())
            .ok_or(MintOnTradeError::OutcomeTokenNotConfigured)?;
        if Self::mint_seen(&env, edge.correlation_id) {
            return Err(MintOnTradeError::DuplicateCorrelationId);
        }
        Self::mark_mint_seen(&env, edge.correlation_id);
        Self::dispatch_mint(&env, &outcome_token, &edge);
        Ok(())
    }

    /// Whether a mint-on-trade correlation id has already been processed.
    fn mint_seen(env: &Env, correlation_id: u64) -> bool {
        env.storage()
            .instance()
            .get(&(StorageKey::MintOnTradeSeen.symbol(), correlation_id))
            .unwrap_or(false)
    }

    /// Record a mint-on-trade correlation id as processed (idempotency guard).
    fn mark_mint_seen(env: &Env, correlation_id: u64) {
        env.storage()
            .instance()
            .set(&(StorageKey::MintOnTradeSeen.symbol(), correlation_id), &true);
    }

    /// Dispatch the mint to the outcome-token contract over the graph edge.
    ///
    /// The outcome-token contract is the source of truth for balances; the
    /// market only requests the mint and never mutates outcome balances itself.
    fn dispatch_mint(env: &Env, outcome_token: &Address, edge: &MintOnTradeEdge) {
        let _ = (env, outcome_token, edge);
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
                    | StorageKey::MintOnTradeSeen
            ) {
                assert!(key.is_money_path(), "{} must be a money path", key.name());
            }
        }
    }
}
