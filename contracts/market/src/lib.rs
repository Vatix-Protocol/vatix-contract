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

use crate::error::ContractError;
use crate::types::{MarketStatus, Position};
use vatix_outcome_token_contract::{types::TokenKind, OutcomeTokenContractClient};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

// `types` is self-contained (only depends on `soroban_sdk`) and carries its
// own `#[cfg(test)]` binding-stability/wasm-budget tests (#1012, #1013); wire
// it in so `cargo test -p vatix-market-contract` actually exercises them.
pub mod types;

// Module graph restored incrementally (#1058, #1060, #1061): each module
// below is compiled into the crate because at least one wired entrypoint or
// wired module depends on it. The deposit/trade/withdraw graph is included
// for the executable money-path regressions (#1080, #1081).
pub mod deposit;
pub mod error;
pub mod events;
pub mod fee_rate;
pub mod fee_waiver;
pub mod oracle;
pub mod positions;
pub mod reconciliation;
pub mod storage;
pub mod validation;
pub mod withdraw;

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
            StorageKey::Admin => symbol_short!("admin"),
            StorageKey::Paused => symbol_short!("paused"),
            StorageKey::MarketConfig => symbol_short!("mkt_cfg"),
            StorageKey::LiquidityPool => symbol_short!("liq_pool"),
            StorageKey::Position => symbol_short!("position"),
            StorageKey::Settlement => symbol_short!("settle"),
            StorageKey::FeeAccrued => symbol_short!("fee_acc"),
            StorageKey::OraclePrice => symbol_short!("oracle"),
            StorageKey::ReentrancyGuard => symbol_short!("reent"),
            StorageKey::OutcomeToken => symbol_short!("out_tok"),
            StorageKey::MintOnTradeSeen => symbol_short!("mot_seen"),
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
    pub fn set_key(
        env: Env,
        caller: Address,
        key: StorageKey,
        value: u64,
    ) -> Result<(), StorageError> {
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
    pub fn set_outcome_token(
        env: Env,
        caller: Address,
        outcome_token: Address,
    ) -> Result<(), StorageError> {
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

    /// Initialize the contract with its admin (#1058).
    ///
    /// First call wins: stores `admin` under [`StorageKey::Admin`] and stamps
    /// the storage version via [`crate::storage::set_version`]. A repeat call
    /// fails closed with [`StorageError::Unauthorized`] so an initialized
    /// admin can never be overwritten by re-initialization. This is the
    /// contract's official initialization path used by the integration-test
    /// harness (`tests/helpers`).
    pub fn initialize(env: Env, admin: Address) -> Result<(), StorageError> {
        if env.storage().instance().has(&StorageKey::Admin.symbol()) {
            return Err(StorageError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::Admin.symbol(), &admin);
        crate::storage::set_version(&env);
        Ok(())
    }

    /// Read-only view: whether `address` currently holds a fee waiver (#1060).
    ///
    /// Thin wrapper over [`crate::fee_waiver::has_fee_waiver`]. Performs no
    /// auth — any client can query waiver status. Returns `false` for
    /// addresses never added and for addresses whose waiver was removed.
    pub fn get_fee_waiver(env: Env, address: Address) -> bool {
        crate::fee_waiver::has_fee_waiver(&env, &address)
    }

    /// Configure the token ledger used by positions and reconciliation.
    pub fn set_outcome_token_contract(
        env: Env,
        admin: Address,
        token: Address,
    ) -> Result<(), error::ContractError> {
        admin.require_auth();
        validation::require_initialized(&env)?;
        if admin != storage::get_admin(&env) {
            return Err(error::ContractError::Unauthorized);
        }
        storage::set_outcome_token_contract(&env, &token);
        Ok(())
    }

    pub fn deposit_collateral(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        deposit::deposit_collateral(env, user, market_id, amount)
    }

    pub fn withdraw_unused_collateral(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        withdraw::withdraw_unused_collateral(env, user, market_id, amount)
    }

    pub fn update_position(
        env: Env,
        user: Address,
        market_id: u32,
        yes_delta: i128,
        no_delta: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        // Emergency mode: trading is blocked unless mode is Normal
        validation::require_emergency_mode_allows(&env, &[crate::types::EmergencyMode::Normal])?;
        // 1. Authorization
        user.require_auth();

        // 2. Validate market state: must exist, be Active, and not be expired
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }
        if env.ledger().timestamp() >= market.end_time {
            return Err(ContractError::MarketExpired);
        }

        // 3. Validate the market price up front for a clear ContractError
        validation::validate_market_price(market_price)?;

        // 4. Enforce that deposited collateral covers any increase in the lock,
        //    and that a market closed to deposits cannot be used to open new
        //    exposure. Trades that keep the lock flat or reduce it (selling /
        //    closing out an existing position) are always allowed, even when
        //    closed_to_deposits is set, since they reduce risk rather than add
        //    it — only opening/increasing a position is "last-minute" new
        //    exposure. Negative-share deltas are left for
        //    positions::update_position to reject (it also emits a
        //    PositionLimitExceeded event).
        let existing_position = storage::get_position(&env, market_id, &user)?;
        let position = existing_position
            .clone()
            .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));
        let new_yes = position.yes_shares + yes_delta;
        let new_no = position.no_shares + no_delta;
        let locked_elsewhere = if new_yes >= 0 && new_no >= 0 {
            let prospective_locked =
                positions::calculate_locked_collateral(new_yes, new_no, market_price);
            let lock_increased = prospective_locked > position.locked_collateral;
            if lock_increased && market.closed_to_deposits {
                return Err(ContractError::MarketClosedToDeposits);
            }
            // Protocol-wide collateral check (ADR-002, issue #685): collateral
            // is a single balance per user shared across every market
            // (`storage::CollateralBalance`) rather than siloed per market.
            // A trade is only rejected here when it would *increase* this
            // market's lock beyond what the user's balance can cover once
            // collateral already locked in every *other* market
            // (`storage::TotalLockedCollateral` minus this market's current
            // lock) is accounted for.
            let locked_elsewhere = storage::get_total_locked_collateral(&env, &user)
                .saturating_sub(position.locked_collateral);
            if lock_increased {
                let protocol_balance = storage::get_collateral_balance(&env, &user);
                if positions::check_protocol_collateral(
                    prospective_locked,
                    protocol_balance,
                    locked_elsewhere,
                )
                .is_err()
                {
                    return Err(ContractError::InsufficientCollateral);
                }
            }
            Some(locked_elsewhere)
        } else {
            None
        };

        reconciliation::assert_position_token_parity(&env, market_id, &user)?;

        // 5. Apply the share deltas (persists the position and emits an event)
        let result =
            positions::update_position(&env, market_id, &user, yes_delta, no_delta, market_price)
                .map_err(|e| match e {
                positions::PositionError::ShareBalanceBelowZero => {
                    ContractError::InvalidShareAmount
                }
                positions::PositionError::InvalidMarketPrice => ContractError::InvalidPrice,
                positions::PositionError::InsufficientProtocolCollateral => {
                    ContractError::InsufficientCollateral
                }
                _ => ContractError::InvalidShareAmount,
            })?;

        if let Some(locked_elsewhere) = locked_elsewhere {
            // Keep the protocol-wide aggregate in sync only after the actual
            // position update succeeds, so failed trades cannot corrupt the
            // global lock accounting for this user.
            let next_total_locked = locked_elsewhere.saturating_add(result.locked_collateral);
            storage::set_total_locked_collateral(&env, &user, next_total_locked);
        }

        // 5a. Track first-time participants so the market can later be
        //     settled page-by-page via `settle_positions_page` (Issue #495)
        //     without requiring an off-chain index of every trader.
        if existing_position.is_none() {
            storage::add_market_participant(&env, market_id, &user)?;
        }

        // 5b. Mint or burn outcome tokens for the updated position.
        if let Some(outcome_token_address) = storage::get_outcome_token_contract(&env) {
            let token_client = OutcomeTokenContractClient::new(&env, &outcome_token_address);
            if yes_delta > 0 {
                token_client.mint(&market_id, &user, &TokenKind::Yes, &yes_delta);
            } else if yes_delta < 0 {
                token_client.burn(&market_id, &user, &TokenKind::Yes, &(-yes_delta));
            }

            if no_delta > 0 {
                token_client.mint(&market_id, &user, &TokenKind::No, &no_delta);
            } else if no_delta < 0 {
                token_client.burn(&market_id, &user, &TokenKind::No, &(-no_delta));
            }
        }

        // 6. Persist the updated price so withdraw and other callers see it
        market.price_bps = market_price;
        storage::set_market(&env, market_id, &market)?;

        Ok(result)
    }

    /// Whether the kill-switch is engaged. Defaults to paused (fail-closed) when
    /// the flag has never been written, so an uninitialized contract denies
    /// money-path writes rather than silently allowing them.
    fn is_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused.symbol())
            .unwrap_or(true)
    }
}

/// Reviewer checklist test (#1010): every `StorageKey` variant must have a
/// unique `symbol()`, a non-empty `name()`, and `is_money_path()` must match
/// the documented set exactly. Fails closed on a variant added to the enum
/// but not to `ALL_STORAGE_KEYS`, or one that silently collides with another.
#[cfg(test)]
mod storage_key_tests {
    use super::*;

    /// Regression test (#1106): `Symbol::short` silently truncates any input
    /// longer than 9 characters instead of erroring, so every `symbol()`
    /// source string must stay within that limit or a key collision/data
    /// loss bug could slip in unnoticed. This pins the exact strings used by
    /// `symbol()` (kept in sync with the match arms above) so a future
    /// variant with a longer string fails this test instead of silently
    /// truncating on-chain.
    #[test]
    fn every_symbol_source_string_fits_symbol_short() {
        const SYMBOL_SOURCES: [(&str, &str); 11] = [
            ("Admin", "admin"),
            ("Paused", "paused"),
            ("MarketConfig", "mkt_cfg"),
            ("LiquidityPool", "liq_pool"),
            ("Position", "position"),
            ("Settlement", "settle"),
            ("FeeAccrued", "fee_acc"),
            ("OraclePrice", "oracle"),
            ("ReentrancyGuard", "reent"),
            ("OutcomeToken", "out_tok"),
            ("MintOnTradeSeen", "mot_seen"),
        ];
        for (name, source) in SYMBOL_SOURCES {
            assert!(
                source.len() <= 9,
                "StorageKey::{name}'s symbol() source \"{source}\" is {} chars; \
                 Symbol::short silently truncates anything over 9 chars",
                source.len()
            );
        }
    }

    #[test]
    fn all_storage_keys_covers_every_variant() {
        assert_eq!(
            ALL_STORAGE_KEYS.len(),
            11,
            "ALL_STORAGE_KEYS must list every StorageKey variant"
        );
    }

    #[test]
    fn every_symbol_is_unique() {
        for (i, a) in ALL_STORAGE_KEYS.iter().enumerate() {
            for b in ALL_STORAGE_KEYS.iter().skip(i + 1) {
                let (a, b) = (*a, *b);
                assert_ne!(
                    a.symbol(),
                    b.symbol(),
                    "StorageKey::{} and StorageKey::{} share a symbol()",
                    a.name(),
                    b.name()
                );
            }
        }
    }

    #[test]
    fn every_name_is_non_empty_and_unique() {
        for (i, key_i) in ALL_STORAGE_KEYS.iter().enumerate() {
            let name_i = key_i.name();
            assert!(!name_i.is_empty(), "StorageKey::name() must not be empty");
            for key_j in ALL_STORAGE_KEYS.iter().skip(i + 1) {
                let name_j = key_j.name();
                assert_ne!(name_i, name_j, "duplicate StorageKey name: {}", name_i);
            }
        }
    }

    #[test]
    fn is_money_path_matches_documented_set() {
        let expected_money_path = [
            StorageKey::LiquidityPool,
            StorageKey::Position,
            StorageKey::Settlement,
            StorageKey::FeeAccrued,
            StorageKey::MintOnTradeSeen,
        ];
        for key in ALL_STORAGE_KEYS {
            let expected = expected_money_path.contains(&key);
            assert_eq!(
                key.is_money_path(),
                expected,
                "StorageKey::{}.is_money_path() should be {}",
                key.name(),
                expected
            );
        }
    }
}
