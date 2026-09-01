use soroban_sdk::{contracttype, Address, BytesN, String, Symbol};

/// Represents the possible states of a prediction market.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MarketStatus {
    Active,
    Resolved,
    Canceled,
}

/// Coordinated emergency mode shared (or mirrored) across Market,
/// Treasury, and Resolution contracts (Issue #662).
///
/// | Variant         | Effect                                                       |
/// |-----------------|--------------------------------------------------------------|
/// | `Normal`        | All operations allowed.                                      |
/// | `TradingHalted` | Reject deposit/trade/propose; allow withdraw + settle/resolve.|
/// | `SettleOnly`    | Only settle & withdraw; block resolve & propose.             |
/// | `GlobalFreeze`  | Everything blocked except admin unpause/management.          |
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum EmergencyMode {
    Normal,
    TradingHalted,
    SettleOnly,
    GlobalFreeze,
}

/// Represents the oracle adapter type used for market resolution.
///
/// This enum determines which oracle adapter (Ed25519, Reflector, or Pyth)
/// will be used to verify the outcome when resolving the market.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum AdapterType {
    Ed25519,
    Reflector,
    Pyth,
}

/// Identifies the asset to query from the Reflector oracle.
///
/// Lives in `types` (not `oracle_adapter`) so the default wasm32 build can
/// serialize `MarketAdapterConfig` without requiring `--features oracle-adapter`.
/// Variant layout matches Reflector's `Asset` enum exactly.
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    /// A Stellar-native SAC token identified by its issuer address.
    Stellar(Address),
    /// Any other asset identified by a 4-byte symbol (e.g. `symbol_short!("BTC")`).
    Other(Symbol),
}

/// Maximum optimized market wasm size, including `--features oracle-adapter`.
///
/// Matches the current Soroban `maxContractSizeBytes` network limit (64 KiB).
/// CI (`scripts/check-market-wasm-size.sh`) fails if the built artifact exceeds
/// this budget so enabling the adapter cannot silently blow install/upgrade limits.
pub const MARKET_WASM32_SIZE_BUDGET: u32 = 65_536;

/// Core structure containing all relevant information for a Market.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Market {
    pub id: u32,
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub status: MarketStatus,
    pub result: Option<bool>,
    pub creator: Address,
    pub created_at: u64,
    pub collateral_token: Address,
    /// Current market price in basis points (0–10_000). Updated on every trade.
    pub price_bps: i128,
    /// Address of the resolver who resolved this market (only set when status is Resolved).
    pub resolver: Option<Address>,
    /// Timestamp when the market was resolved (only set when status is Resolved).
    pub resolved_at: Option<u64>,
    /// Oracle adapter type used for resolving this market.
    pub adapter_type: AdapterType,
    /// Number of possible outcomes for this market. Always 2 (YES/NO) for binary
    /// prediction markets. Set once at creation and immutable thereafter.
    pub outcome_count: u32,
    /// Flag indicating whether the market is closed to new deposits.
    /// When true, users cannot deposit new collateral, but can still withdraw and trade.
    pub closed_to_deposits: bool,
}

/// Tracks the position and shares of a specific user in a market.
///
/// # Storage layout (#482)
///
/// Fields are ordered largest/widest-first (`Address`, then the `i128` share
/// and collateral amounts) down to the narrowest fields (the `u32` market id
/// and the single-byte `is_settled` flag) last. This groups same-width fields
/// together to avoid wasted padding in the on-chain encoded representation
/// and keeps the compact layout intent explicit for future field additions.
///
/// Note: this is a breaking storage-layout change — reordering the declared
/// fields changes the serialized on-chain representation of `Position`. No
/// migration is included per the scope of #482; existing deployments would
/// need to redeploy/reinitialize as with any other breaking storage change
/// (see `STORAGE_VERSION` in `storage.rs`).
///
/// The `i128` amount fields (`yes_shares`, `no_shares`, `locked_collateral`,
/// `total_deposited`) are intentionally left wide: they represent token
/// quantities that can legitimately grow very large, so narrowing them would
/// risk overflow. Only the naturally-bounded `market_id` (`u32`) and
/// `is_settled` (`bool`) fields are narrow.
///
/// # Invariant: `locked_collateral <= total_deposited`
///
/// This must hold after every successful `deposit_collateral`,
/// `update_position`, and `withdraw_unused_collateral` call. It is enforced
/// by refusing state transitions that would violate it, never by clamping:
/// - `MarketContract::update_position` (`lib.rs`) rejects any trade whose
///   recalculated lock would exceed `total_deposited` with
///   `ContractError::InsufficientCollateral`, *before* calling
///   `positions::update_position` to persist anything.
/// - `withdraw_unused_collateral` (`withdraw.rs`) only allows withdrawing up
///   to `total_deposited - locked_collateral`, so a withdrawal can never
///   itself push the balance below the lock.
///
/// See `tests/locked_le_deposited_invariant_test.rs`,
/// `tests/collateral_invariant_test.rs` and
/// `tests/proptest_locked_invariant.rs` for the invariant tests, including
/// the documented rejection ("failure") case where an over-leveraged trade
/// is refused and the position is left unchanged.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Position {
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    /// Collateral required to back current YES/NO shares (from calculate_locked_collateral).
    pub locked_collateral: i128,
    /// Total collateral deposited by user in this market (never decreased except by withdraw).
    pub total_deposited: i128,
    pub market_id: u32,
    pub is_settled: bool,
}

/// A fee-rate change awaiting its timelock delay before it can take effect
/// (Issue #496). Only one change may be pending at a time; proposing a new
/// one overwrites any earlier pending change.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingFeeRateChange {
    pub new_rate_bps: i128,
    /// Ledger timestamp at or after which `execute_fee_rate_change` may apply this change.
    pub effective_at: u64,
}

/// An address change awaiting its timelock delay before it can take effect.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingAddressChange {
    pub new_address: Address,
    pub effective_at: u64,
}

/// An adapter type change awaiting its timelock delay before it can take effect.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingAdapterTypeChange {
    pub new_adapter: AdapterType,
    pub effective_at: u64,
}

/// Per-market Reflector/Pyth oracle adapter configuration (#681).
///
/// `Market` carries an `adapter_type` but, before this, had nowhere to store
/// the Reflector asset/contract or the price threshold a market resolves
/// against — so an admin could mark a market `AdapterType::Reflector` but
/// there was no way to actually configure it. Stored separately (keyed by
/// `market_id`, see `StorageKey::MarketAdapterConfig` in `storage.rs`)
/// rather than as inline `Market` fields so existing `Market` storage
/// entries need no migration; a market with no entry here simply has no
/// adapter config and `oracle::verify_market_outcome` fails closed if that adapter is enabled
/// without config (#734); Ed25519 fallback is only used while the adapter is disabled.
#[derive(Clone, Debug)]
#[contracttype]
pub struct MarketAdapterConfig {
    /// Address of the Reflector (or Pyth) oracle contract on the target
    /// network.
    pub oracle_contract: Address,
    /// Reflector asset identifier queried via `lastprice`. Unused for Pyth.
    pub asset: Asset,
    /// Price threshold, in the oracle's native fixed-point units (Reflector:
    /// 7 decimals). The market resolves YES when the fetched price is
    /// `>= resolution_price`.
    pub resolution_price: i128,
}

/// A 32-byte change (like oracle pubkey) awaiting its timelock delay before it can take effect.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingBytesNChange {
    pub new_bytes: BytesN<32>,
    pub effective_at: u64,
}

/// A threshold signer set and quorum change awaiting its timelock delay (#665).
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingThresholdSignersChange {
    pub signers: soroban_sdk::Vec<BytesN<32>>,
    pub quorum: u32,
    pub effective_at: u64,
}

impl Position {
    /// Create an empty position for a user in a market.
    /// Used when a position has not been previously recorded in storage.
    pub fn new_empty(market_id: u32, user: Address) -> Self {
        Position {
            market_id,
            user,
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        }
    }
}

#[cfg(test)]
mod wasm_budget_tests {
    use super::MARKET_WASM32_SIZE_BUDGET;

    /// Guard the documented Soroban 64 KiB install/upgrade limit (#734).
    /// Bumping this constant without a network-limit change must fail CI.
    #[test]
    fn market_wasm32_budget_is_soroban_64kib_limit() {
        assert_eq!(MARKET_WASM32_SIZE_BUDGET, 65_536);
    }
}
