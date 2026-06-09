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

    // ========== Authorization Errors (40-49) ==========
    /// Caller is not authorized to perform this action.
    ///
    /// The caller must be the market creator or have appropriate permissions.
    Unauthorized = 40,

    /// Caller is not the admin for this operation.
    ///
    /// Only the contract admin can perform this action.
    NotAdmin = 41,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed (insufficient balance, approval, etc.).
    ///
    /// Ensure the user has sufficient balance and has approved the contract.
    TokenTransferFailed = 50,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic operation overflowed.
    ///
    /// The operation would exceed the maximum value for the data type.
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
}
