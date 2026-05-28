use crate::error::ContractError;
use soroban_sdk::String;

/// Validates that a question is non-empty and within length limits
fn validate_question_format(question: &String) -> Result<(), ContractError> {
    if question.is_empty() {
        return Err(ContractError::InvalidQuestion);
    }

    if question.len() >= 500 {
        return Err(ContractError::InvalidQuestion);
    }

    Ok(())
}

/// Validates that end_time is in the future and within reasonable bounds
fn validate_end_time(end_time: u64, current_time: u64) -> Result<(), ContractError> {
    if end_time <= current_time {
        return Err(ContractError::InvalidTimestamp);
    }

    const ONE_YEAR_SECONDS: u64 = 31_536_000;
    if end_time > current_time + ONE_YEAR_SECONDS {
        return Err(ContractError::InvalidTimestamp);
    }

    Ok(())
}

/// Validates market creation parameters
pub fn validate_market_creation(
    question: &String,
    end_time: u64,
    current_time: u64,
) -> Result<(), ContractError> {
    validate_question_format(question)?;
    validate_end_time(end_time, current_time)?;
    Ok(())
}

/// Validates that amount is positive
fn validate_amount_positive(amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidQuantity);
    }
    Ok(())
}

/// Validates that amount does not exceed reasonable limits
fn validate_amount_reasonable(amount: i128) -> Result<(), ContractError> {
    const MAX_REASONABLE_AMOUNT: i128 = i128::MAX / 2;
    if amount > MAX_REASONABLE_AMOUNT {
        return Err(ContractError::InvalidQuantity);
    }
    Ok(())
}

/// Validates collateral amount
pub fn validate_collateral_amount(amount: i128) -> Result<(), ContractError> {
    validate_amount_positive(amount)?;
    validate_amount_reasonable(amount)?;
    Ok(())
}

/// Validates that shares are non-negative
fn validate_shares_non_negative(yes_shares: i128, no_shares: i128) -> Result<(), ContractError> {
    if yes_shares < 0 || no_shares < 0 {
        return Err(ContractError::InvalidShareAmount);
    }
    Ok(())
}

/// Validates that at least one share amount is positive
fn validate_shares_not_empty(yes_shares: i128, no_shares: i128) -> Result<(), ContractError> {
    if yes_shares == 0 && no_shares == 0 {
        return Err(ContractError::InvalidShareAmount);
    }
    Ok(())
}

/// Validates share amounts
pub fn validate_shares(yes_shares: i128, no_shares: i128) -> Result<(), ContractError> {
    validate_shares_non_negative(yes_shares, no_shares)?;
    validate_shares_not_empty(yes_shares, no_shares)?;
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

/// Parse a decimal market_id string to u32 (e.g. "1", "42").
/// Returns InvalidQuantity if empty, non-digit, or overflow.
pub fn parse_market_id(market_id: &String) -> Result<u32, ContractError> {
    let len = market_id.len();
    if len == 0 || len > 10 {
        return Err(ContractError::InvalidQuantity);
    }
    let mut buf = [0u8; 10];
    let slice = &mut buf[..len as usize];
    market_id.copy_into_slice(slice);
    let mut n: u32 = 0;
    for b in slice.iter() {
        if *b < b'0' || *b > b'9' {
            return Err(ContractError::InvalidQuantity);
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add((*b - b'0') as u32))
            .ok_or(ContractError::InvalidQuantity)?;
    }
    Ok(n)
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

    #[test]
    fn test_parse_market_id_valid() {
        let env = soroban_sdk::Env::default();
        assert_eq!(parse_market_id(&String::from_str(&env, "1")).unwrap(), 1);
        assert_eq!(parse_market_id(&String::from_str(&env, "42")).unwrap(), 42);
        assert_eq!(
            parse_market_id(&String::from_str(&env, "999")).unwrap(),
            999
        );
    }

    #[test]
    fn test_parse_market_id_invalid() {
        let env = soroban_sdk::Env::default();
        assert_eq!(
            parse_market_id(&String::from_str(&env, "")),
            Err(ContractError::InvalidQuantity)
        );
        assert_eq!(
            parse_market_id(&String::from_str(&env, "abc")),
            Err(ContractError::InvalidQuantity)
        );
        assert_eq!(
            parse_market_id(&String::from_str(&env, "12a")),
            Err(ContractError::InvalidQuantity)
        );
    }
}
