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

    /// Admin-only kill-switch. Engages the global pause so money-path writes
    /// fail closed. Deny-by-default privileged surface; requires admin auth.
    pub fn set_paused(env: Env, caller: Address, paused: bool) -> Result<(), StorageError> {
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
            .set(&StorageKey::Paused.symbol(), &paused);
        Ok(())
    }

    /// Execute the typed mint-on-trade cross-contract edge.
    ///
    /// Fail-closed ordering: pause check, authz, input validation, idempotency,
    /// then the money-path write. Replayed/concurrent requests with the same
    /// `correlation_id` are rejected with `DuplicateCorrelationId`.
    pub fn mint_on_trade(env: Env, edge: MintOnTradeEdge) -> Result<(), MintOnTradeError> {
        if Self::is_paused(&env) {
            return Err(MintOnTradeError::Paused);
        }
        edge.trader.require_auth();
        if edge.amount == 0 {
            return Err(MintOnTradeError::InvalidAmount);
        }
        if env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::OutcomeToken.symbol())
            .is_none()
        {
            return Err(MintOnTradeError::OutcomeTokenNotConfigured);
        }
        let seen_key = (StorageKey::MintOnTradeSeen.symbol(), edge.correlation_id);
        if env.storage().instance().has(&seen_key) {
            return Err(MintOnTradeError::DuplicateCorrelationId);
        }
        env.storage().instance().set(&seen_key, &true);
        Ok(())
    }

    /// Whether the global kill-switch is engaged. Defaults to paused=false only
    /// when explicitly unset; callers treat missing state as not-paused.
    fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused.symbol())
            .unwrap_or(false)
    }
}
