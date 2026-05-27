use crate::types::{Market, Position};
use soroban_sdk::{contracterror, Address, Env};

const BASIS_POINTS: i128 = 10_000;
pub const STROOPS_PER_USDC: i128 = 10_000_000;

/// Errors returned by position validation and update operations.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PositionError {
    /// Proposed YES or NO share change would reduce that side below zero
    ShareBalanceBelowZero = 1,
}

/// Calculate required locked collateral based on net position.
///
/// # Arguments
/// * `yes_shares` - Number of YES shares held
/// * `no_shares` - Number of NO shares held
/// * `market_price` - Current market price in basis points (0–10_000)
///
/// # Returns
/// Collateral that must remain locked, in the same unit as the share values.
///
/// # Logic
/// - Net YES  => lock `net_yes * price / 10_000`
/// - Net NO   => lock `net_no * (10_000 - price) / 10_000`
/// - Hedged   => lock `0`
///
/// # Example
/// ```
/// // 100 YES shares at a 60% price => 60 units locked
/// let locked = calculate_locked_collateral(100, 0, 6_000);
/// assert_eq!(locked, 60);
/// ```
pub fn calculate_locked_collateral(yes_shares: i128, no_shares: i128, market_price: i128) -> i128 {
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

/// Validate whether a proposed position change is allowed.
///
/// # Errors
/// Returns [`PositionError::ShareBalanceBelowZero`] when `yes_delta` or
/// `no_delta` would leave either share balance negative.
pub fn validate_position_change(
    current_position: &Position,
    yes_delta: i128,
    no_delta: i128,
) -> Result<(), PositionError> {
    let new_yes = current_position.yes_shares + yes_delta;
    let new_no = current_position.no_shares + no_delta;

    if new_yes < 0 || new_no < 0 {
        return Err(PositionError::ShareBalanceBelowZero);
    }

    Ok(())
}

/// Calculate net position from YES and NO shares.
///
/// # Arguments
/// * `yes_shares` - Number of YES shares held
/// * `no_shares` - Number of NO shares held
///
/// # Returns
/// Positive value => net long YES, negative => net long NO, zero => hedged.
///
/// # Example
/// ```
/// assert_eq!(calculate_net_position(100, 30), 70);  // net long YES
/// assert_eq!(calculate_net_position(30, 100), -70); // net long NO
/// ```
pub fn calculate_net_position(yes_shares: i128, no_shares: i128) -> i128 {
    yes_shares - no_shares
}

/// Check if a position is eligible for settlement.
///
/// Returns `true` only when the market is `Resolved` and the position has not
/// already been settled.
///
/// # Arguments
/// * `position` - The user's position to check
/// * `market` - The market the position belongs to
///
/// # Example
/// ```
/// // Returns false if position.is_settled == true, even on a resolved market.
/// assert!(!can_settle(&settled_position, &resolved_market));
/// ```
pub fn can_settle(position: &Position, market: &Market) -> bool {
    use crate::types::MarketStatus;
    matches!(market.status, MarketStatus::Resolved) && !position.is_settled
}

/// Update a user's position with new share deltas
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `yes_delta` - Change in YES shares (can be negative)
/// * `no_delta` - Change in NO shares (can be negative)
/// * `market_price` - Current market price for collateral calculation
///
/// # Returns
/// Updated Position struct
///
/// # Errors
/// - [`PositionError::ShareBalanceBelowZero`] if deltas would make shares negative
pub fn update_position(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_delta: i128,
    no_delta: i128,
    market_price: i128,
) -> Result<Position, PositionError> {
    // 1. Load or initialize position
    let mut position =
        crate::storage::get_position(env, market_id, user).unwrap_or_else(|| Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        });

    // 2. Validate deltas
    validate_position_change(&position, yes_delta, no_delta)?;

    // 3. Apply deltas
    position.yes_shares += yes_delta;
    position.no_shares += no_delta;

    // 4. Recalculate locked collateral
    let new_locked =
        calculate_locked_collateral(position.yes_shares, position.no_shares, market_price);
    position.locked_collateral = new_locked;

    // 5. Persist
    crate::storage::set_position(env, market_id, user, &position);

    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types;
    use soroban_sdk::{testutils::Address as TestAddress, Address, BytesN, Env, String};

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
            id: 1,
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

        let locked =
            calculate_locked_collateral(100 * STROOPS_PER_USDC, 30 * STROOPS_PER_USDC, 5000);
        assert_eq!(locked, 35 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_net_no() {
        let locked = calculate_locked_collateral(0, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 40 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_hedged() {
        let locked =
            calculate_locked_collateral(100 * STROOPS_PER_USDC, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 0);
    }

    #[test]
    fn test_validate_position_change() {
        let env = setup_env();
        let position = Position {
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 50,
            no_shares: 50,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };

        assert!(validate_position_change(&position, 10, -20).is_ok());
        assert_eq!(
            validate_position_change(&position, -60, 0),
            Err(PositionError::ShareBalanceBelowZero)
        );
        assert_eq!(
            validate_position_change(&position, 0, -60),
            Err(PositionError::ShareBalanceBelowZero)
        );
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
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };

        assert!(can_settle(&position, &market));
    }

    #[test]
    fn test_can_settle_already_settled() {
        let env = setup_env();
        let market = sample_market(&env);
        let position = Position {
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: true,
        };

        assert!(!can_settle(&position, &market));
    }

    #[test]
    fn test_update_position_new_user() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 1);
        let market_id = 1;

        let pos = env.as_contract(&contract_id, || {
            update_position(&env, market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000)
                .expect("should update position")
        });

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 0);
        assert_eq!(pos.locked_collateral, 60 * STROOPS_PER_USDC);
        assert!(!pos.is_settled);
    }

    #[test]
    fn test_update_position_existing_user() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 2);
        let market_id = 2;

        // First update - buy YES
        let _ = env.as_contract(&contract_id, || {
            update_position(&env, market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000).unwrap()
        });

        // Second update - buy some NO
        let pos = env.as_contract(&contract_id, || {
            update_position(&env, market_id, &user, 0, 30 * STROOPS_PER_USDC, 6000).unwrap()
        });

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 30 * STROOPS_PER_USDC);
        assert_eq!(pos.locked_collateral, 42 * STROOPS_PER_USDC);
    }
}
