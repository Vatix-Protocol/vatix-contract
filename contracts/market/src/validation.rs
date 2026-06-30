use crate::error::ContractError;
use crate::types::MarketStatus;
use soroban_sdk::{Env, String};

/// Minimum collateral deposit in stroops (1 USDC = 10_000_000 stroops).
pub const MIN_DEPOSIT_AMOUNT: i128 = 10_000_000;

/// Guard function to validate input before processing.
///
/// This is a general-purpose validation guard that can be used in integration tests
/// and contract entry points to ensure invalid inputs are rejected early.
///
/// # Arguments
/// * `input` - The input value to validate
///
/// # Returns
/// `Ok(())` if input is valid, `Err(ContractError::InvalidQuantity)` if invalid
///
/// # Example
/// ```ignore
/// validation::validate_input_guard(amount)?;
/// ```
pub fn validate_input_guard(input: i128) -> Result<(), ContractError> {
    if input <= 0 {
        return Err(ContractError::InvalidQuantity);
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

/// Validates question format: must be non-empty and fewer than 500 characters
fn validate_question_format(question: &String) -> Result<(), ContractError> {
    let len = question.len();
    if len == 0 || len >= 500 {
        return Err(ContractError::InvalidQuestion);
    }
    Ok(())
}

/// Validates metadata URI format if provided
///
/// If metadata_uri is Some, it must:
/// - Be non-empty
/// - Be at most 2048 characters
///
/// If metadata_uri is None, validation passes (optional field).
///
/// # Arguments
/// * `metadata_uri` - The metadata URI to validate
///
/// # Returns
/// `Ok(())` if valid, `Err(ContractError::InvalidMetadataUri)` if invalid
///
/// # Example
/// ```ignore
/// let uri = Some(String::from_str(&env, "ipfs://QmXxx..."));
/// validation::validate_metadata_uri(&uri)?;
/// ```
pub fn validate_metadata_uri(metadata_uri: &Option<String>) -> Result<(), ContractError> {
    if let Some(uri) = metadata_uri {
        let len = uri.len();
        // Check non-empty
        if len == 0 {
            return Err(ContractError::InvalidMetadataUri);
        }
        // Check max length (2048 is standard URI limit)
        if len > 2048 {
            return Err(ContractError::InvalidMetadataUri);
        }
    }
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

/// Validates collateral amount (used for withdrawals — no minimum enforced).
pub fn validate_collateral_amount(amount: i128) -> Result<(), ContractError> {
    validate_amount_positive(amount)?;
    validate_amount_reasonable(amount)?;
    Ok(())
}

/// Validates a deposit amount, enforcing the protocol minimum of MIN_DEPOSIT_AMOUNT.
pub fn validate_deposit_amount(amount: i128) -> Result<(), ContractError> {
    validate_collateral_amount(amount)?;
    if amount < MIN_DEPOSIT_AMOUNT {
        return Err(ContractError::BelowMinDeposit);
    }
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

/// Validates market price is within valid basis-point range (0–10_000)
pub fn validate_market_price(price: i128) -> Result<(), ContractError> {
    if !(0..=10_000).contains(&price) {
        return Err(ContractError::InvalidPrice);
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

/// Validates that a market may be administratively canceled.
///
/// Cancellation is only permitted while a market is still open, i.e. before it
/// has been resolved by the oracle. This encodes the cancel policy in one place
/// so the contract entry point stays declarative.
///
/// # Arguments
/// * `status` - Current [`MarketStatus`] of the market being canceled
///
/// # Returns
/// `Ok(())` when the market is [`MarketStatus::Active`] and can be canceled.
///
/// # Errors
/// - [`ContractError::MarketAlreadyResolved`] – the market is already resolved
///   and its outcome is final, so it can no longer be canceled.
/// - [`ContractError::MarketNotActive`] – the market is already canceled; the
///   operation is a no-op and is rejected to surface the redundant call.
pub fn validate_cancelable(status: &MarketStatus) -> Result<(), ContractError> {
    match status {
        MarketStatus::Active => Ok(()),
        MarketStatus::Resolved => Err(ContractError::MarketAlreadyResolved),
        MarketStatus::Canceled => Err(ContractError::MarketNotActive),
    }
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

/// Validates that a configured withdrawal fee rate (in basis points).
///
/// The fee rate must lie within the inclusive 0–10_000 bps range (0%–100%).
///
/// # Errors
/// - `InvalidPrice`: `fee_rate_bps` is outside the 0–10_000 range.
pub fn validate_fee_rate_bps(fee_rate_bps: i128) -> Result<(), ContractError> {
    validate_market_price(fee_rate_bps)
}

/// Validates that outcome_count is exactly 2 (binary YES/NO market).
///
/// All Vatix markets are binary. This is enforced at creation and re-checked
/// on every write so the field cannot be silently mutated by callers.
///
/// # Errors
/// - [`ContractError::InvalidOutcomeCount`] – `outcome_count` is not 2.
pub fn validate_outcome_count(outcome_count: u32) -> Result<(), ContractError> {
    if outcome_count != 2 {
        return Err(ContractError::InvalidOutcomeCount);
    }
    Ok(())
}

/// Calculate fee with validation guard
///
/// # Arguments
/// * `amount` - Base amount to calculate fee on
/// * `fee_rate_bps` - Fee rate in basis points (0-10000)
///
/// # Returns
/// Fee amount in same units as input amount
///
/// # Errors
/// - `InvalidQuantity`: amount <= 0
/// - `InvalidPrice`: fee_rate_bps outside 0-10000 range
/// - `ArithmeticOverflow`: calculation would overflow
pub fn calculate_fee(amount: i128, fee_rate_bps: i128) -> Result<i128, ContractError> {
    validate_amount_positive(amount)?;
    validate_market_price(fee_rate_bps)?;

    amount
        .checked_mul(fee_rate_bps)
        .and_then(|result| result.checked_div(10_000))
        .ok_or(ContractError::ArithmeticOverflow)
}

/// Guard: reject operations when the contract has not been initialized.
///
/// # Errors
/// - [`ContractError::NotInitialized`] – the contract has no admin set.
pub fn require_initialized(env: &Env) -> Result<(), ContractError> {
    if !crate::storage::has_admin(env) {
        return Err(ContractError::NotInitialized);
    }
    Ok(())
}

/// Guard: reject state-mutating operations when the contract is paused.
///
/// # Errors
/// - [`ContractError::ContractPaused`] – the contract is in emergency halt.
pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if crate::storage::is_paused(env) {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

#[cfg(test)]
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

    #[test]
    fn test_validate_market_price_valid() {
        assert!(validate_market_price(0).is_ok());
        assert!(validate_market_price(5_000).is_ok());
        assert!(validate_market_price(10_000).is_ok());
    }

    #[test]
    fn test_validate_market_price_invalid() {
        assert_eq!(validate_market_price(-1), Err(ContractError::InvalidPrice));
        assert_eq!(
            validate_market_price(10_001),
            Err(ContractError::InvalidPrice)
        );
    }

    #[test]
    fn test_validate_input_guard_valid() {
        assert!(validate_input_guard(1).is_ok());
        assert!(validate_input_guard(100).is_ok());
        assert!(validate_input_guard(i128::MAX / 2).is_ok());
    }

    #[test]
    fn test_validate_input_guard_invalid() {
        assert_eq!(validate_input_guard(0), Err(ContractError::InvalidQuantity));
        assert_eq!(
            validate_input_guard(-1),
            Err(ContractError::InvalidQuantity)
        );
        assert_eq!(
            validate_input_guard(-100),
            Err(ContractError::InvalidQuantity)
        );
    }

    #[test]
    fn test_calculate_fee_valid() {
        assert_eq!(calculate_fee(1000, 100).unwrap(), 10); // 1% of 1000
        assert_eq!(calculate_fee(10000, 500).unwrap(), 500); // 5% of 10000
        assert_eq!(calculate_fee(100, 0).unwrap(), 0); // 0% fee
    }

    #[test]
    fn test_calculate_fee_invalid_amount() {
        assert_eq!(calculate_fee(0, 100), Err(ContractError::InvalidQuantity));
        assert_eq!(
            calculate_fee(-100, 100),
            Err(ContractError::InvalidQuantity)
        );
    }

    #[test]
    fn test_calculate_fee_invalid_rate() {
        assert_eq!(calculate_fee(1000, -1), Err(ContractError::InvalidPrice));
        assert_eq!(calculate_fee(1000, 10001), Err(ContractError::InvalidPrice));
    }

    #[test]
    fn test_calculate_fee_overflow() {
        assert_eq!(
            calculate_fee(i128::MAX, 10000),
            Err(ContractError::ArithmeticOverflow)
        );
    }

    #[test]
    fn test_validate_cancelable_active_ok() {
        assert!(validate_cancelable(&MarketStatus::Active).is_ok());
    }

    #[test]
    fn test_validate_cancelable_resolved_fails() {
        assert_eq!(
            validate_cancelable(&MarketStatus::Resolved),
            Err(ContractError::MarketAlreadyResolved)
        );
    }

    #[test]
    fn test_validate_cancelable_already_canceled_fails() {
        assert_eq!(
            validate_cancelable(&MarketStatus::Canceled),
            Err(ContractError::MarketNotActive)
        );
    }

    #[test]
    fn test_validate_admin_address_account_ok() {
        let env = soroban_sdk::Env::default();
        // Generate a user account address (starts with 'G')
        let admin = Address::generate(&env);
        assert!(validate_admin_address(&admin).is_ok());
    }

    #[test]
    fn test_validate_admin_address_contract_fails() {
        let env = soroban_sdk::Env::default();
        // Register a contract to get a contract address (starts with 'C')
        let contract_id = env.register(crate::MarketContract, ());
        assert_eq!(
            validate_admin_address(&contract_id),
            Err(ContractError::InvalidAdmin)
        );
    }
}

    #[test]
    fn test_validate_cancelable_already_canceled_fails() {
        assert_eq!(
            validate_cancelable(&MarketStatus::Canceled),
            Err(ContractError::MarketNotActive)
        );
    }

    #[test]
    fn test_require_initialized_when_has_admin() {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = soroban_sdk::Address::generate(&env);
        env.as_contract(&contract_id, || {
            crate::storage::set_admin(&env, &admin);
            assert!(require_initialized(&env).is_ok());
        });
    }

    #[test]
    fn test_require_initialized_returns_not_initialized() {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(require_initialized(&env), Err(ContractError::NotInitialized));
        });
    }

    #[test]
    fn test_require_not_paused_returns_ok_when_not_paused() {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            assert!(require_not_paused(&env).is_ok());
        });
    }

    #[test]
    fn test_require_not_paused_returns_contract_paused_when_paused() {
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            crate::storage::set_paused(&env, true);
            assert_eq!(require_not_paused(&env), Err(ContractError::ContractPaused));
        });
    }
}