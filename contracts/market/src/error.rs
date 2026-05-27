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
    /// The requested market does not exist in storage
    MarketNotFound = 1,

    /// Attempted to resolve a market that has already been resolved
    MarketAlreadyResolved = 2,

    /// Attempted to settle positions before market resolution
    MarketNotResolved = 3,

    /// Market has passed its end_time and is no longer active for trading
    MarketExpired = 4,

    /// Market is not in Active status (may be Resolved or Canceled)
    MarketNotActive = 5,

    // ========== Position Errors (10-19) ==========
    /// User does not have enough collateral locked to perform this operation
    InsufficientCollateral = 10,

    /// Attempted to settle a position that has already been settled
    PositionAlreadySettled = 11,

    /// No position exists for this user in this market
    NoPositionFound = 12,

    /// Share amount is invalid (e.g., negative or zero when positive required)
    InvalidShareAmount = 13,

    // ========== Oracle Errors (20-29) ==========
    /// Oracle signature verification failed
    InvalidSignature = 20,

    /// Caller is not the authorized oracle for this market
    UnauthorizedOracle = 21,

    /// Resolution outcome value is invalid (must be true or false)
    InvalidOutcome = 22,

    // ========== Validation Errors (30-39) ==========
    /// Price is out of valid range (must be between 0 and 1)
    InvalidPrice = 30,

    /// Quantity is invalid (must be positive)
    InvalidQuantity = 31,

    /// Timestamp is invalid (e.g., end_time in the past)
    InvalidTimestamp = 32,

    /// Market question is invalid (e.g., empty string)
    InvalidQuestion = 33,

    // ========== Authorization Errors (40-49) ==========
    /// Caller is not authorized to perform this action
    Unauthorized = 40,

    /// Caller is not the admin for this operation
    NotAdmin = 41,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed (insufficient balance, approval, etc.)
    TokenTransferFailed = 50,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic operation overflowed
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
