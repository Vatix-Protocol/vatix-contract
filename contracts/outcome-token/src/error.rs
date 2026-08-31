use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InsufficientBalance = 4,
    InvalidAmount = 5,
    Overflow = 6,
    /// A peer-to-peer `transfer` was attempted before the associated market
    /// resolved. Outcome tokens are only transferable once the market they
    /// belong to has settled its outcome.
    MarketNotResolved = 7,
    /// The on-chain storage schema version does not match the version this
    /// contract build expects (Issue #696). Mirrors
    /// `vatix_market_contract::ContractError::UpgradeRequired` /
    /// `vatix_treasury_contract::TreasuryError::UpgradeRequired` — blocks
    /// `mint`/`burn`/`transfer` against a stale on-chain layout after a
    /// partial cross-contract upgrade instead of silently corrupting balances.
    UpgradeRequired = 8,
}
