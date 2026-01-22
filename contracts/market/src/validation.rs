use crate::error::ContractError;
use soroban_sdk::String;

/// Validates market creation parameters
pub fn validate_market_creation(
    question: &String,
    end_time: u64,
    current_time: u64,
) -> Result<(), ContractError> {
    // Question must not be empty
    if question.is_empty() {
        return Err(ContractError::InvalidQuestion);
    }

    // Question length must be < 500 characters
    if question.len() >= 500 {
        return Err(ContractError::InvalidQuestion);
    }

    // End time must be in future (> current_time)
    if end_time <= current_time {
        return Err(ContractError::InvalidTimestamp);
    }

    // End time not too far in future (< 1 year)
    // 1 year = 365 * 24 * 60 * 60 = 31,536,000 seconds
    const ONE_YEAR_SECONDS: u64 = 31_536_000;
    if end_time > current_time + ONE_YEAR_SECONDS {
        return Err(ContractError::InvalidTimestamp);
    }

    Ok(())
}

/// Validates collateral amount
pub fn validate_collateral_amount(amount: i128) -> Result<(), ContractError> {
    // Amount must be positive
    if amount <= 0 {
        return Err(ContractError::InvalidQuantity);
    }

    // Amount must be reasonable (not overflow i128)
    // Check against a reasonable maximum to prevent overflow issues
    const MAX_REASONABLE_AMOUNT: i128 = i128::MAX / 2;
    if amount > MAX_REASONABLE_AMOUNT {
        return Err(ContractError::InvalidQuantity);
    }

    Ok(())
}

/// Validates share amounts
pub fn validate_shares(yes_shares: i128, no_shares: i128) -> Result<(), ContractError> {
    // Both must be non-negative
    if yes_shares < 0 || no_shares < 0 {
        return Err(ContractError::InvalidShareAmount);
    }

    // At least one must be > 0
    if yes_shares == 0 && no_shares == 0 {
        return Err(ContractError::InvalidShareAmount);
    }

    Ok(())
}

/// Validates outcome value
pub fn validate_outcome(outcome: bool) -> Result<(), ContractError> {
    // Simple bool check (for consistency)
    // In Rust, bool is always valid, but we keep this for API consistency
    // and potential future validation logic
    let _ = outcome; // Acknowledge the parameter
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_market_creation() {
        let question = String::from_str(&soroban_sdk::Env::default(), "Will it rain tomorrow?");
        let current_time = 1000;
        let end_time = current_time + 86400; // 1 day later

        assert!(validate_market_creation(&question, end_time, current_time).is_ok());
    }

    #[test]
    fn test_empty_question_fails() {
        let question = String::from_str(&soroban_sdk::Env::default(), "");
        let current_time = 1000;
        let end_time = current_time + 86400;

        assert_eq!(
            validate_market_creation(&question, end_time, current_time),
            Err(ContractError::InvalidQuestion)
        );
    }

    #[test]
    fn test_long_question_fails() {
        let long_str = "a".repeat(500);
        let question = String::from_str(&soroban_sdk::Env::default(), &long_str);
        let current_time = 1000;
        let end_time = current_time + 86400;

        assert_eq!(
            validate_market_creation(&question, end_time, current_time),
            Err(ContractError::InvalidQuestion)
        );
    }

    #[test]
    fn test_past_end_time_fails() {
        let question = String::from_str(&soroban_sdk::Env::default(), "Valid question?");
        let current_time = 1000;
        let end_time = current_time - 1; // In the past

        assert_eq!(
            validate_market_creation(&question, end_time, current_time),
            Err(ContractError::InvalidTimestamp)
        );
    }

    #[test]
    fn test_far_future_end_time_fails() {
        let question = String::from_str(&soroban_sdk::Env::default(), "Valid question?");
        let current_time = 1000;
        let end_time = current_time + 31_536_001; // More than 1 year

        assert_eq!(
            validate_market_creation(&question, end_time, current_time),
            Err(ContractError::InvalidTimestamp)
        );
    }

    #[test]
    fn test_valid_collateral_amount() {
        assert!(validate_collateral_amount(100).is_ok());
        assert!(validate_collateral_amount(1).is_ok());
    }

    #[test]
    fn test_zero_collateral_fails() {
        assert_eq!(
            validate_collateral_amount(0),
            Err(ContractError::InvalidQuantity)
        );
    }

    #[test]
    fn test_negative_collateral_fails() {
        assert_eq!(
            validate_collateral_amount(-1),
            Err(ContractError::InvalidQuantity)
        );
    }

    #[test]
    fn test_excessive_collateral_fails() {
        let excessive_amount = i128::MAX;
        assert_eq!(
            validate_collateral_amount(excessive_amount),
            Err(ContractError::InvalidQuantity)
        );
    }

    #[test]
    fn test_valid_shares() {
        assert!(validate_shares(100, 0).is_ok());
        assert!(validate_shares(0, 100).is_ok());
        assert!(validate_shares(50, 50).is_ok());
    }

    #[test]
    fn test_both_zero_shares_fails() {
        assert_eq!(
            validate_shares(0, 0),
            Err(ContractError::InvalidShareAmount)
        );
    }

    #[test]
    fn test_negative_shares_fail() {
        assert_eq!(
            validate_shares(-1, 100),
            Err(ContractError::InvalidShareAmount)
        );
        assert_eq!(
            validate_shares(100, -1),
            Err(ContractError::InvalidShareAmount)
        );
    }

    #[test]
    fn test_valid_outcome() {
        assert!(validate_outcome(true).is_ok());
        assert!(validate_outcome(false).is_ok());
    }
}
