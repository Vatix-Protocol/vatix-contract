use soroban_sdk::{contracttype, Address, String};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum TokenKind {
    Yes,
    No,
}

/// Local mirror of `vatix_market::types::MarketStatus`.
///
/// The outcome-token contract cannot depend on the market crate directly
/// (the market crate already depends on this one), so this enum must be kept
/// in variant-for-variant sync with the market contract's `MarketStatus` for
/// cross-contract calls to `get_market_status` to decode correctly.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MarketStatus {
    Active,
    Resolved,
    Canceled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OutcomeTokenConfig {
    pub admin: Address,
    /// Only this address may call mint and burn.
    pub market_contract: Address,
    /// Human-readable token name (SAC metadata).
    pub name: String,
    /// Ticker symbol (SAC metadata).
    pub symbol: String,
}

/// A proposed `market_contract` (mint/burn authority) rotation awaiting its
/// timelock delay before it can take effect (Issue #691). Mirrors the market
/// contract's own address-change timelock pattern so mint authority cannot
/// rotate instantly.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingAddressChange {
    pub new_address: Address,
    pub effective_at: u64,
}
