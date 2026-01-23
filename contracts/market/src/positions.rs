use crate::error::ContractError;
use crate::types::{Market, Position};

#[allow(dead_code)]
const BASIS_POINTS: i128 = 10_000;
#[allow(dead_code)]
pub const STROOPS_PER_USDC: i128 = 10_000_000;

/// Calculate required locked collateral based on net position
///
/// Logic:
/// - Net YES  => lock net_yes * price
/// - Net NO   => lock net_no * (1 - price)
/// - Hedged   => lock 0
#[allow(dead_code)]
pub fn calculate_locked_collateral(
    yes_shares: i128,
    no_shares: i128,
    market_price: i128,
) -> i128 {
    if yes_shares == no_shares {
        return 0;
    }

    if yes_shares > no_shares {
        let net_yes = yes_shares - no_shares;
        net_yes
            .checked_mul(market_price)
            .unwrap()
            .checked_div(BASIS_POINTS)
            .unwrap()
    } else {
        let net_no = no_shares - yes_shares;
        let inverse_price = BASIS_POINTS - market_price;
        net_no
            .checked_mul(inverse_price)
            .unwrap()
            .checked_div(BASIS_POINTS)
            .unwrap()
    }
}

/// Validate whether a proposed position change is allowed
#[allow(dead_code)]
pub fn validate_position_change(
    current_position: &Position,
    yes_delta: i128,
    no_delta: i128,
) -> Result<(), ContractError> {
    let new_yes = current_position.yes_shares + yes_delta;
    let new_no = current_position.no_shares + no_delta;

    if new_yes < 0 || new_no < 0 {
        return Err(ContractError::InvalidShareAmount);
    }

    Ok(())
}

/// Calculate net position from YES and NO shares
///
/// Positive  => net long YES
/// Negative  => net long NO
/// Zero      => hedged
#[allow(dead_code)]
pub fn calculate_net_position(yes_shares: i128, no_shares: i128) -> i128 {
    yes_shares - no_shares
}

/// Check if a position is eligible for settlement
#[allow(dead_code)]
pub fn can_settle(position: &Position, market: &Market) -> bool {
    use crate::types::MarketStatus;
    matches!(market.status, MarketStatus::Resolved) && !position.is_settled
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as TestAddress, Address, BytesN, Env, String};
    use crate::types;

    fn setup_env() -> Env {
        Env::default()
    }

    /// Create a sample user address for testing
    fn sample_user(env: &Env, _id: u8) -> Address {
        <Address as TestAddress>::generate(env)
    }

    /// Create a sample market for testing
    fn sample_market(env: &Env) -> Market {
        Market {
            id: String::from_str(env, "market1"),
            question: String::from_str(env, "Will it rain tomorrow?"),
            end_time: 0,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status: types::MarketStatus::Resolved,
            collateral_token: <Address as TestAddress>::generate(env),
            creator: <Address as TestAddress>::generate(env),
            created_at: 0,
            result: None,
        }
    }

    #[test]
    fn test_calculate_locked_collateral_net_yes() {
        let locked = calculate_locked_collateral(100 * STROOPS_PER_USDC, 0, 6000);
        assert_eq!(locked, 60 * STROOPS_PER_USDC);

        let locked = calculate_locked_collateral(
            100 * STROOPS_PER_USDC,
            30 * STROOPS_PER_USDC,
            5000,
        );
        assert_eq!(locked, 35 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_net_no() {
        let locked = calculate_locked_collateral(0, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 40 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_hedged() {
        let locked = calculate_locked_collateral(
            100 * STROOPS_PER_USDC,
            100 * STROOPS_PER_USDC,
            6000,
        );
        assert_eq!(locked, 0);
    }

    #[test]
    fn test_validate_position_change() {
        let env = setup_env();
        let position = Position {
            market_id: String::from_str(&env, "m1"),
            user: sample_user(&env, 1),
            yes_shares: 50,
            no_shares: 50,
            locked_collateral: 0,
            is_settled: false,
        };

        assert!(validate_position_change(&position, 10, -20).is_ok());
        assert!(validate_position_change(&position, -60, 0).is_err());
        assert!(validate_position_change(&position, 0, -60).is_err());
    }

    #[test]
    fn test_calculate_net_position() {
        assert_eq!(calculate_net_position(100, 30), 70);
        assert_eq!(calculate_net_position(30, 100), -70);
        assert_eq!(calculate_net_position(50, 50), 0);
    }

    #[test]
    fn test_can_settle_resolved_market() {
        let env = setup_env();
        let market = sample_market(&env);
        let position = Position {
            market_id: String::from_str(&env, "m1"),
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: false,
        };

        assert!(can_settle(&position, &market));
    }

    #[test]
    fn test_can_settle_already_settled() {
        let env = setup_env();
        let market = sample_market(&env);
        let position = Position {
            market_id: String::from_str(&env, "m1"),
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: true,
        };

        assert!(!can_settle(&position, &market));
    }
}

