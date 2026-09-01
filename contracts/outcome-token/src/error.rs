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
    /// contract build expects (Issue #696).
    UpgradeRequired = 8,
    /// `execute_market_contract` was called but no pending rotation exists.
    NoPendingMarketContractChange = 9,
    /// The timelock delay for a pending `market_contract` rotation has not
    /// elapsed yet.
    TimelockNotElapsed = 10,
    /// A peer-to-peer `transfer` was attempted after the market resolved.
    /// Post-resolution transfers are blocked because settlement pays out
    /// against the original depositor's position record, not the current
    /// token holder (Issue #690).
    TransferBlockedAfterResolve = 11,
}
