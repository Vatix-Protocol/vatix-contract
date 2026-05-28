use soroban_sdk::contracterror;

/// Error codes
///
/// Errors are grouped by category with reserved number ranges:
/// - Market Errors: 1-9
/// - Position Errors: 10-19
/// - Oracle Errors: 20-29
/// - Validation Errors: 30-39
/// - Authorization Errors: 40-49
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    // ========== Market Errors (1-9) ==========
    /// The requested market does not exist in storage; verify the market_id is
    /// correct and that the market has been successfully initialized
    MarketNotFound = 1,

    /// Attempted to resolve a market that has already been resolved; each
    /// market can only be resolved once — check market status before resolving
    MarketAlreadyResolved = 2,

    /// Settlement was attempted but the market has not been resolved yet;
    /// wait for the oracle to submit a valid signed resolution before settling
    MarketNotResolved = 3,

    /// Market end_time has passed and it is no longer active for trading;
    /// no new positions may be opened or modified after the deadline
    MarketExpired = 4,

    /// Market is not in Active status and cannot accept this operation;
    /// the market may be Resolved or Canceled — check status before acting
    MarketNotActive = 5,

    // ========== Position Errors (10-19) ==========
    /// User does not have enough unlocked collateral for this operation;
    /// deposit more collateral or close open positions to free locked funds
    InsufficientCollateral = 10,

    /// Settlement was attempted on a position that has already been paid out;
    /// each position can only be settled once — check is_settled before calling
    PositionAlreadySettled = 11,

    /// No position record exists for this (market_id, user) pair;
    /// the user must deposit collateral and open a position before this call
    NoPositionFound = 12,

    /// Share amount is invalid; values must be non-negative integers with at
    /// least one side (yes or no) greater than zero
    InvalidShareAmount = 13,

    // ========== Oracle Errors (20-29) ==========
    /// Oracle Ed25519 signature verification failed; the signature does not
    /// match the expected message digest for this market_id and outcome
    InvalidSignature = 20,

    /// The signing key does not match the authorized oracle for this market;
    /// ensure the correct oracle keypair is used to sign the resolution
    UnauthorizedOracle = 21,

    /// Resolution outcome is not a valid boolean; must be true (YES wins) or
    /// false (NO wins) — no other values are accepted
    InvalidOutcome = 22,

    // ========== Validation Errors (30-39) ==========
    /// Price is outside the valid range; binary market prices must be strictly
    /// between 0 and 1 expressed as a fixed-point fraction
    InvalidPrice = 30,

    /// Quantity is invalid; the value must be a positive integer and must not
    /// exceed the contract's maximum allowed collateral amount
    InvalidQuantity = 31,

    /// Timestamp is out of range; end_time must be strictly in the future and
    /// no more than one year (31 536 000 s) from the current ledger time
    InvalidTimestamp = 32,

    /// Market question is invalid; it must be a non-empty UTF-8 string of
    /// fewer than 500 characters
    InvalidQuestion = 33,

    // ========== Authorization Errors (40-49) ==========
    /// Caller is not authorized to perform this action; only the stored admin
    /// address may invoke admin-only functions
    Unauthorized = 40,

    /// Caller is not the contract admin; use the admin address that was set
    /// during contract initialization
    NotAdmin = 41,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed; verify the caller holds sufficient balance and
    /// has granted the necessary allowance to this contract address
    TokenTransferFailed = 50,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic overflow detected; the computed value exceeds the maximum
    /// representable integer — reduce input magnitudes and retry
    ArithmeticOverflow = 60,
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
        assert_eq!(ContractError::InsufficientCollateral as u32, 10);
        assert_eq!(ContractError::PositionAlreadySettled as u32, 11);
        assert_eq!(ContractError::NoPositionFound as u32, 12);
        assert_eq!(ContractError::InvalidShareAmount as u32, 13);
        assert_eq!(ContractError::InvalidSignature as u32, 20);
        assert_eq!(ContractError::UnauthorizedOracle as u32, 21);
        assert_eq!(ContractError::InvalidOutcome as u32, 22);
        assert_eq!(ContractError::InvalidPrice as u32, 30);
        assert_eq!(ContractError::InvalidQuantity as u32, 31);
        assert_eq!(ContractError::InvalidTimestamp as u32, 32);
        assert_eq!(ContractError::InvalidQuestion as u32, 33);
        assert_eq!(ContractError::Unauthorized as u32, 40);
        assert_eq!(ContractError::NotAdmin as u32, 41);
        assert_eq!(ContractError::TokenTransferFailed as u32, 50);
        assert_eq!(ContractError::ArithmeticOverflow as u32, 60);
    }

    #[test]
    fn test_error_equality() {
        assert_eq!(ContractError::MarketNotFound, ContractError::MarketNotFound);
        assert_ne!(ContractError::MarketNotFound, ContractError::MarketNotActive);
    }

    #[test]
    fn test_error_ordering() {
        assert!(ContractError::MarketNotFound < ContractError::InsufficientCollateral);
        assert!(ContractError::InvalidSignature < ContractError::InvalidPrice);
        assert!(ContractError::Unauthorized < ContractError::TokenTransferFailed);
    }
}
