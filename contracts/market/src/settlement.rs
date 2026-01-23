use crate::error::ContractError;
use crate::types::{Market, MarketStatus, Position};

/// Calculate payout for a position based on market outcome
///
/// # Arguments
/// * `position` - User's position
/// * `outcome` - Market outcome (true = YES won, false = NO won)
///
/// # Returns
/// Payout amount in stroops (1 USDC = 10^7 stroops)
pub fn calculate_payout(position: &Position, outcome: bool) -> i128 {
    if outcome {
        position.yes_shares
    } else {
        position.no_shares
    }
}

/// Check if a position is eligible for settlement
///
/// # Arguments
/// * `position` - Position to check
/// * `market` - Associated market
pub fn validate_settlement_eligibility(
    position: &Position,
    market: &Market,
) -> Result<(), ContractError> {
    if market.status != MarketStatus::Resolved {
        return Err(ContractError::MarketNotResolved);
    }

    if position.is_settled {
        return Err(ContractError::PositionAlreadySettled);
    }

    Ok(())
}

/// Execute settlement for a position and return payout
///
/// This function:
/// 1. Validates settlement eligibility
/// 2. Calculates payout
/// 3. Marks position as settled
/// 4. Returns payout amount
pub fn execute_settlement(position: &mut Position, market: &Market) -> Result<i128, ContractError> {
    validate_settlement_eligibility(position, market)?;

    let outcome = market.result.ok_or(ContractError::MarketNotResolved)?;
    let payout = calculate_payout(position, outcome);

    position.is_settled = true;

    Ok(payout)
}

/// Calculate what a user would receive if they settled now
///
/// # Arguments
/// * `position` - User's position
/// * `market` - Market (may or may not be resolved)
pub fn calculate_potential_payout(position: &Position, market: &Market) -> Option<i128> {
    market
        .result
        .map(|outcome| calculate_payout(position, outcome))
}

/// Calculate statistics about settlements
///
/// # Returns
/// (winning_shares, losing_shares, total_payout)
pub fn calculate_market_settlement_stats(
    total_yes_shares: i128,
    total_no_shares: i128,
    outcome: bool,
) -> (i128, i128, i128) {
    if outcome {
        (total_yes_shares, total_no_shares, total_yes_shares)
    } else {
        (total_no_shares, total_yes_shares, total_no_shares)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

    fn create_test_market(env: &Env, status: MarketStatus, result: Option<bool>) -> Market {
        Market {
            id: String::from_str(env, "market-1"),
            question: String::from_str(env, "Test?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status,
            result,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: Address::generate(env),
        }
    }

    fn create_test_position(env: &Env, yes: i128, no: i128, settled: bool) -> Position {
        Position {
            market_id: String::from_str(env, "market-1"),
            user: Address::generate(env),
            yes_shares: yes,
            no_shares: no,
            locked_collateral: yes + no, // simplified
            is_settled: settled,
        }
    }

    #[test]
    fn test_calculate_payout_yes_wins() {
        let env = Env::default();
        let pos = create_test_position(&env, 100, 30, false);
        assert_eq!(calculate_payout(&pos, true), 100);
    }

    #[test]
    fn test_calculate_payout_no_wins() {
        let env = Env::default();
        let pos = create_test_position(&env, 100, 30, false);
        assert_eq!(calculate_payout(&pos, false), 30);
    }

    #[test]
    fn test_calculate_payout_hedged_position() {
        let env = Env::default();
        let pos = create_test_position(&env, 50, 50, false);
        assert_eq!(calculate_payout(&pos, true), 50);
        assert_eq!(calculate_payout(&pos, false), 50);
    }

    #[test]
    fn test_calculate_payout_zero_shares() {
        let env = Env::default();
        let pos = create_test_position(&env, 0, 0, false);
        assert_eq!(calculate_payout(&pos, true), 0);
    }

    #[test]
    fn test_validate_settlement_not_resolved() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Active, None);
        let pos = create_test_position(&env, 100, 0, false);

        let result = validate_settlement_eligibility(&pos, &market);
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_validate_settlement_already_settled() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let pos = create_test_position(&env, 100, 0, true);

        let result = validate_settlement_eligibility(&pos, &market);
        assert_eq!(result, Err(ContractError::PositionAlreadySettled));
    }

    #[test]
    fn test_execute_settlement_marks_as_settled() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let mut pos = create_test_position(&env, 100, 0, false);

        let payout = execute_settlement(&mut pos, &market).unwrap();
        assert_eq!(payout, 100);
        assert!(pos.is_settled);
    }

    #[test]
    fn test_execute_settlement_returns_correct_amount() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(false));
        let mut pos = create_test_position(&env, 100, 30, false);

        let payout = execute_settlement(&mut pos, &market).unwrap();
        assert_eq!(payout, 30);
    }

    #[test]
    fn test_potential_payout_unresolved_market() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Active, None);
        let pos = create_test_position(&env, 100, 0, false);

        assert_eq!(calculate_potential_payout(&pos, &market), None);
    }

    #[test]
    fn test_potential_payout_resolved_market() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let pos = create_test_position(&env, 100, 30, false);

        assert_eq!(calculate_potential_payout(&pos, &market), Some(100));
    }

    #[test]
    fn test_market_settlement_stats() {
        let (winning, losing, payout) = calculate_market_settlement_stats(1000, 500, true);
        assert_eq!(winning, 1000);
        assert_eq!(losing, 500);
        assert_eq!(payout, 1000);

        let (winning, losing, payout) = calculate_market_settlement_stats(1000, 500, false);
        assert_eq!(winning, 500);
        assert_eq!(losing, 1000);
        assert_eq!(payout, 500);
    }
}
