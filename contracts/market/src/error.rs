use soroban_sdk::contracterror;

/// Error codes for the Vatix market contract.
///
/// Errors are grouped by category with reserved number ranges:
/// - Market Errors: 1-9
/// - Position Errors: 10-19
/// - Oracle Errors: 20-29
/// - Validation Errors: 30-39
/// - Authorization Errors: 40-49
/// - Token Errors: 50-59
/// - Arithmetic Errors: 60-69
///
/// # Example
/// ```ignore
/// use vatix_market::error::ContractError;
///
/// // Check for specific error
/// match result {
///     Err(ContractError::MarketNotFound) => println!("Market does not exist"),
///     Err(ContractError::InvalidQuestion) => println!("Question is invalid"),
///     Ok(_) => println!("Success"),
/// }
/// ```
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    // ========== Market Errors (1-9) ==========
    /// The requested market does not exist in storage.
    ///
    /// Returned when attempting to access a market with an invalid or non-existent ID.
    MarketNotFound = 1,

    /// Attempted to resolve a market that has already been resolved.
    ///
    /// Each market can only be resolved once. Attempting to resolve again will fail.
    MarketAlreadyResolved = 2,

    /// Settlement was attempted but the market has not been resolved yet.
    ///
    /// Wait for the oracle to submit a valid resolution before settling positions.
    MarketNotResolved = 3,

    /// Market has passed its end_time and is no longer active for trading.
    ///
    /// No new positions can be opened or modified after the market expires.
    MarketExpired = 4,

    /// Market is not in Active status (may be Resolved or Canceled).
    ///
    /// Only Active markets accept new trades and collateral deposits.
    MarketNotActive = 5,

    /// New collateral deposits are disabled for this market.
    ///
    /// The admin has called `close_market_to_deposits`. Deposits are blocked
    /// outright, and `update_position` calls that would increase locked
    /// collateral (opening new exposure) are also rejected. Trades that
    /// reduce or hold the lock flat (closing/reducing a position) and
    /// withdrawals continue to work normally.
    MarketClosedToDeposits = 6,

    /// Withdraw attempted before the cooldown period since the last deposit has elapsed.
    WithdrawCooldownActive = 7,

    // ========== Position Errors (10-19) ==========
    /// User does not have enough collateral locked to perform this operation.
    ///
    /// Ensure sufficient collateral is deposited before attempting trades.
    InsufficientCollateral = 10,

    /// Settlement was attempted on a position that has already been paid out.
    ///
    /// Each position can only be settled once.
    PositionAlreadySettled = 11,

    /// No position exists for this user in this market.
    ///
    /// The user must have an open position to perform this operation.
    NoPositionFound = 12,

    /// Share amount is invalid (e.g., negative or zero when positive required).
    ///
    /// Share amounts must be non-negative, and at least one side must be positive.
    InvalidShareAmount = 13,

    /// The batch supplied to `batch_settle_positions` was either empty or exceeded
    /// `MAX_BATCH_SETTLE_SIZE`. Empty batches are rejected to surface caller bugs
    /// early; oversized batches are rejected to prevent gas-griefing attacks.
    BatchTooLarge = 14,

    // ========== Oracle Errors (20-29) ==========
    /// Oracle signature verification failed.
    ///
    /// The provided signature does not match the oracle's public key or the market data.
    InvalidSignature = 20,

    /// Caller is not the authorized oracle for this market.
    ///
    /// Only the designated oracle can submit resolutions for this market.
    UnauthorizedOracle = 21,

    /// Resolution outcome value is invalid (must be true or false).
    ///
    /// Outcome must be a valid boolean value.
    InvalidOutcome = 22,

    /// Reflector oracle returned no price for the requested asset.
    ///
    /// Occurs when `lastprice(asset)` returns `None` — the asset may be
    /// unsupported, the oracle may not have a recent price, or the Reflector
    /// node network may be temporarily disconnected.
    OraclePriceUnavailable = 23,

    /// The oracle message has an `expires_at` deadline that has already
    /// passed. Signed outcomes cannot be replayed after their stated
    /// deadline — submit a freshly signed message instead.
    OracleMessageExpired = 24,

    /// The requested threshold is invalid (e.g., higher than signer count or zero).
    InvalidThresholdQuorum = 25,

    /// A Reflector/Pyth price observation is older than the adapter's
    /// maximum allowed staleness window (#682).
    ///
    /// Both adapters read a timestamp/publish_time alongside the price but
    /// previously never checked it — a disconnected or slow oracle feed
    /// could otherwise resolve a market against an arbitrarily old price.
    StalePrice = 26,

    // ========== Validation Errors (30-39) ==========
    /// Price is out of valid range (must be between 0 and 1).
    ///
    /// Prices represent probabilities and must be normalized.
    InvalidPrice = 30,

    /// Quantity is invalid (must be positive).
    ///
    /// Quantities, amounts, and counts must be greater than zero.
    InvalidQuantity = 31,

    /// Timestamp is invalid (e.g., end_time in the past or too far in future).
    ///
    /// Market end_time must be in the future and within one year.
    InvalidTimestamp = 32,

    /// Market question is invalid (e.g., empty string or exceeds 500 characters).
    ///
    /// Questions must be non-empty and reasonably sized (1-499 characters).
    InvalidQuestion = 33,

    /// Outcome count is not exactly 2.
    ///
    /// All markets on this protocol are binary (YES/NO). Any attempt to create
    /// or overwrite a market with an outcome_count other than 2 is rejected.
    InvalidOutcomeCount = 34,

    /// Admin address is invalid (e.g., contract address or zero address).
    ///
    /// The admin must be a valid user account address, not a contract address
    /// or any special/reserved address.
    InvalidAdmin = 35,

    /// Deposit amount is below the configured minimum deposit.
    BelowMinDeposit = 36,

    /// Market metadata URI is invalid (e.g. exceeds the maximum length).
    InvalidMetadataUri = 37,

    /// Fee rate is invalid (e.g. exceeds the configured fee cap or is out of range).
    InvalidFeeRate = 38,

    /// Fee waiver account is invalid (a contract address, or the admin itself).
    ///
    /// The admin-managed fee waiver list (#483) may only hold ordinary user
    /// accounts. Contract addresses are rejected the same way `InvalidAdmin`
    /// rejects them, and the admin's own address is rejected so the admin
    /// cannot quietly exempt itself from withdrawal fees it controls (#584).
    InvalidFeeWaiverAccount = 39,

    // ========== Authorization Errors (40-49) ==========
    /// Caller is not authorized to perform this action.
    ///
    /// The caller must be the market creator or have appropriate permissions.
    Unauthorized = 40,

    /// Caller is not the admin for this operation.
    ///
    /// Only the contract admin can perform this action.
    NotAdmin = 41,

    /// Contract has already been initialized.
    ///
    /// `initialize(admin)` may only be called once. Replaying it would allow
    /// an attacker to hijack the admin slot after initial deploy.
    AlreadyInitialized = 42,

    /// No pending admin transfer exists.
    ///
    /// `accept_admin` was called but `propose_admin` has not been issued yet,
    /// or the previous proposal was already accepted.
    NoPendingAdmin = 43,

    /// `confirm_renounce_admin` was called but no renounce proposal is pending.
    NoRenounceProposal = 44,

    /// A renounce proposal is already pending; cannot propose again until confirmed or canceled.
    RenounceAlreadyProposed = 45,

    /// The requested fee rate exceeds the configured fee cap.
    ///
    /// The admin must lower the fee rate to at or below the cap before
    /// calling `set_fee_rate`.
    FeeCapExceeded = 46,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed (insufficient balance, approval, etc.).
    ///
    /// Ensure the user has sufficient balance and has approved the contract.
    TokenTransferFailed = 50,

    /// A user's `Position` shares and their `OutcomeToken` balances have
    /// diverged for this market (dual-ledger reconciliation guard).
    ///
    /// Trading and settlement are blocked for this user/market until an
    /// admin repairs the divergence via `reconcile_position_tokens`. See
    /// `contracts/market/src/reconciliation.rs`.
    PositionTokenMismatch = 51,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic operation overflowed.
    ///
    /// The operation would exceed the maximum value for the data type.
    ArithmeticOverflow = 60,

    // ========== Upgrade Errors (70-79) ==========
    /// Storage layout version does not match the current contract version.
    ///
    /// A migration must be performed before the contract can be used.
    /// On testnet, redeploy and reinitialize the contract.
    UpgradeRequired = 70,

    // ========== Resolution Errors (80-89) ==========
    /// A resolution contract is registered but no finalized candidate exists
    /// for this market, or the candidate has been challenged.
    ///
    /// Call `ResolutionContract::finalize` first, then retry `resolve_market`.
    ResolutionNotFinalized = 80,

    // ========== Pause / Initialization Errors (90-99) ==========
    /// The contract has not been initialized yet.
    ///
    /// Admin operations are rejected until `initialize` is called.
    NotInitialized = 90,

    /// The contract is paused for emergency maintenance.
    ///
    /// All state-mutating operations are temporarily disabled.
    ContractPaused = 91,

    /// The requested operation is blocked by the current emergency mode
    /// (Issue #662). Check [`get_emergency_mode`] for the active mode.
    ///
    /// In `TradingHalted`: deposits, trades, and market creation are blocked.
    /// In `SettleOnly`: only settlement and withdrawal are allowed.
    /// In `GlobalFreeze`: all non-admin operations are blocked.
    EmergencyModeActive = 92,

    // ========== Security Errors (100-109) ==========
    /// A reentrant call was detected (e.g. a token contract calling back into
    /// `deposit_collateral` before the initial call has finished).
    ReentrantCall = 100,

    // ========== Timelock Errors (110-119) ==========
    /// `execute_fee_rate_change` was called but no fee rate change is pending.
    NoPendingFeeChange = 110,

    /// `execute_fee_rate_change` was called before the timelock delay elapsed.
    TimelockNotElapsed = 111,
}

#[cfg(test)]
mod tests {
    use super::ContractError;

    #[test]
    fn test_error_discriminants() {
        assert_eq!(ContractError::MarketNotFound as u32, 1);
        assert_eq!(ContractError::MarketAlreadyResolved as u32, 2);
        assert_eq!(ContractError::MarketNotResolved as u32, 3);
        assert_eq!(ContractError::MarketExpired as u32, 4);
        assert_eq!(ContractError::MarketNotActive as u32, 5);
        assert_eq!(ContractError::MarketClosedToDeposits as u32, 6);
        assert_eq!(ContractError::WithdrawCooldownActive as u32, 7);
        assert_eq!(ContractError::InsufficientCollateral as u32, 10);
        assert_eq!(ContractError::PositionAlreadySettled as u32, 11);
        assert_eq!(ContractError::NoPositionFound as u32, 12);
        assert_eq!(ContractError::InvalidShareAmount as u32, 13);
        assert_eq!(ContractError::BatchTooLarge as u32, 14);
        assert_eq!(ContractError::InvalidSignature as u32, 20);
        assert_eq!(ContractError::UnauthorizedOracle as u32, 21);
        assert_eq!(ContractError::InvalidOutcome as u32, 22);
        assert_eq!(ContractError::OraclePriceUnavailable as u32, 23);
        assert_eq!(ContractError::OracleMessageExpired as u32, 24);
        assert_eq!(ContractError::InvalidPrice as u32, 30);
        assert_eq!(ContractError::InvalidQuantity as u32, 31);
        assert_eq!(ContractError::InvalidTimestamp as u32, 32);
        assert_eq!(ContractError::InvalidQuestion as u32, 33);
        assert_eq!(ContractError::InvalidOutcomeCount as u32, 34);
        assert_eq!(ContractError::InvalidAdmin as u32, 35);
        assert_eq!(ContractError::BelowMinDeposit as u32, 36);
        assert_eq!(ContractError::InvalidMetadataUri as u32, 37);
        assert_eq!(ContractError::InvalidFeeRate as u32, 38);
        assert_eq!(ContractError::InvalidFeeWaiverAccount as u32, 39);
        assert_eq!(ContractError::Unauthorized as u32, 40);
        assert_eq!(ContractError::NotAdmin as u32, 41);
        assert_eq!(ContractError::AlreadyInitialized as u32, 42);
        assert_eq!(ContractError::NoPendingAdmin as u32, 43);
        assert_eq!(ContractError::NoRenounceProposal as u32, 44);
        assert_eq!(ContractError::RenounceAlreadyProposed as u32, 45);
        assert_eq!(ContractError::FeeCapExceeded as u32, 46);
        assert_eq!(ContractError::TokenTransferFailed as u32, 50);
        assert_eq!(ContractError::PositionTokenMismatch as u32, 51);
        assert_eq!(ContractError::ArithmeticOverflow as u32, 60);
        assert_eq!(ContractError::UpgradeRequired as u32, 70);
        assert_eq!(ContractError::ResolutionNotFinalized as u32, 80);
        assert_eq!(ContractError::NotInitialized as u32, 90);
        assert_eq!(ContractError::ContractPaused as u32, 91);
        assert_eq!(ContractError::ReentrantCall as u32, 100);
        assert_eq!(ContractError::NoPendingFeeChange as u32, 110);
        assert_eq!(ContractError::TimelockNotElapsed as u32, 111);
    }

    #[test]
    fn test_error_equality() {
        assert_eq!(ContractError::MarketNotFound, ContractError::MarketNotFound);
        assert_ne!(
            ContractError::MarketNotFound,
            ContractError::MarketNotActive
        );
    }

    #[test]
    fn test_error_ordering() {
        assert!(ContractError::MarketNotFound < ContractError::InsufficientCollateral);
        assert!(ContractError::InvalidSignature < ContractError::InvalidPrice);
        assert!(ContractError::Unauthorized < ContractError::TokenTransferFailed);
    }

    /// Ensure no two variants share the same discriminant value.
    ///
    /// This test exhaustively compares every variant pair so a future merge
    /// that accidentally reuses a discriminant is caught immediately rather
    /// than at runtime via undefined behaviour.
    #[test]
    fn test_no_duplicate_discriminants() {
        let all: &[(ContractError, u32)] = &[
            (ContractError::MarketNotFound, 1),
            (ContractError::MarketAlreadyResolved, 2),
            (ContractError::MarketNotResolved, 3),
            (ContractError::MarketExpired, 4),
            (ContractError::MarketNotActive, 5),
            (ContractError::MarketClosedToDeposits, 6),
            (ContractError::WithdrawCooldownActive, 7),
            (ContractError::InsufficientCollateral, 10),
            (ContractError::PositionAlreadySettled, 11),
            (ContractError::NoPositionFound, 12),
            (ContractError::InvalidShareAmount, 13),
            (ContractError::BatchTooLarge, 14),
            (ContractError::InvalidSignature, 20),
            (ContractError::UnauthorizedOracle, 21),
            (ContractError::InvalidOutcome, 22),
            (ContractError::OraclePriceUnavailable, 23),
            (ContractError::OracleMessageExpired, 24),
            (ContractError::InvalidThresholdQuorum, 25),
            (ContractError::StalePrice, 26),
            (ContractError::InvalidPrice, 30),
            (ContractError::InvalidQuantity, 31),
            (ContractError::InvalidTimestamp, 32),
            (ContractError::InvalidQuestion, 33),
            (ContractError::InvalidOutcomeCount, 34),
            (ContractError::InvalidAdmin, 35),
            (ContractError::BelowMinDeposit, 36),
            (ContractError::InvalidMetadataUri, 37),
            (ContractError::InvalidFeeRate, 38),
            (ContractError::InvalidFeeWaiverAccount, 39),
            (ContractError::Unauthorized, 40),
            (ContractError::NotAdmin, 41),
            (ContractError::AlreadyInitialized, 42),
            (ContractError::NoPendingAdmin, 43),
            (ContractError::NoRenounceProposal, 44),
            (ContractError::RenounceAlreadyProposed, 45),
            (ContractError::FeeCapExceeded, 46),
            (ContractError::TokenTransferFailed, 50),
            (ContractError::PositionTokenMismatch, 51),
            (ContractError::ArithmeticOverflow, 60),
            (ContractError::UpgradeRequired, 70),
            (ContractError::ResolutionNotFinalized, 80),
            (ContractError::NotInitialized, 90),
            (ContractError::ContractPaused, 91),
            (ContractError::EmergencyModeActive, 92),
            (ContractError::ReentrantCall, 100),
            (ContractError::NoPendingFeeChange, 110),
            (ContractError::TimelockNotElapsed, 111),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(
                    all[i].1,
                    all[j].1,
                    "duplicate discriminant {} between {:?} and {:?}",
                    all[i].1,
                    all[i].0,
                    all[j].0,
                );
            }
        }
    }
}
