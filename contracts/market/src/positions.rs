use soroban_sdk::{contract, contractimpl, Address, Env, String};

use crate::{
    error::ContractError,
    types::{Market, Position},
};

const BASIS_POINTS: i128 = 10_000;
const STROOPS_PER_USDC: i128 = 10_000_000;
/// Calculate required locked collateral based on net position
    ///
    /// Logic:
    /// - Net YES  => lock net_yes * price
    /// - Net NO   => lock net_no * (1 - price)
    /// - Hedged   => lock 0
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

    /// Update a user's position with new share deltas
    pub fn update_position(
        env: &Env,
        market_id: &String,
        user: &Address,
        yes_delta: i128,
        no_delta: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        // 1. Load or initialize position safely
        let mut position =
            crate::storage::get_position(env, market_id, user).unwrap_or_else(|| Position {
                market_id: market_id.clone(),
                user: user.clone(),
                yes_shares: 0,
                no_shares: 0,
                locked_collateral: 0,
                is_settled: false,
            });

        // 2. Validate deltas
        Self::validate_position_change(&position, yes_delta, no_delta)?;

        // 3. Apply deltas
        position.yes_shares += yes_delta;
        position.no_shares += no_delta;

        // 4. Recalculate locked collateral
        let new_locked = Self::calculate_locked_collateral(
            position.yes_shares,
            position.no_shares,
            market_price,
        );

        position.locked_collateral = new_locked;

        // 5. Persist
        crate::storage::set_position(env, market_id, user, &position);

        Ok(position)
    }

    /// Calculate net position from YES and NO shares
    ///
    /// Positive  => net long YES
    /// Negative  => net long NO
    /// Zero      => hedged
    pub fn calculate_net_position(yes_shares: i128, no_shares: i128) -> i128 {
        yes_shares - no_shares
    }

    /// Check if a position is eligible for settlement
    pub fn can_settle(position: &Position, market: &Market) -> bool {
        use crate::types::MarketStatus;
        matches!(market.status, MarketStatus::Resolved) && !position.is_settled
    }

    #[cfg(test)]
mod tests {
    use super::*;

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
        let locked = MarketContract::calculate_locked_collateral(100 * STROOPS_PER_USDC, 0, 6000);
        assert_eq!(locked, 60 * STROOPS_PER_USDC);

        let locked = MarketContract::calculate_locked_collateral(
            100 * STROOPS_PER_USDC,
            30 * STROOPS_PER_USDC,
            5000,
        );
        assert_eq!(locked, 35 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_net_no() {
        let locked = MarketContract::calculate_locked_collateral(0, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 40 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_hedged() {
        let locked = MarketContract::calculate_locked_collateral(
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

        assert!(MarketContract::validate_position_change(&position, 10, -20).is_ok());
        assert!(MarketContract::validate_position_change(&position, -60, 0).is_err());
        assert!(MarketContract::validate_position_change(&position, 0, -60).is_err());
    }

    #[test]
    fn test_update_position_new_user() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 1);
        let market_id = String::from_str(&env, "market1");

        let pos = env.as_contract(&contract_id, || {
            MarketContract::update_position(
                &env,
                &market_id,
                &user,
                100 * STROOPS_PER_USDC,
                0,
                6000,
            )
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
        let market_id = String::from_str(&env, "market2");

        // First update - buy YES
        let _ = env.as_contract(&contract_id, || {
            MarketContract::update_position(
                &env,
                &market_id,
                &user,
                100 * STROOPS_PER_USDC,
                0,
                6000,
            )
            .unwrap()
        });

        // Second update - buy some NO
        let pos = env.as_contract(&contract_id, || {
            MarketContract::update_position(&env, &market_id, &user, 0, 30 * STROOPS_PER_USDC, 6000)
                .unwrap()
        });

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 30 * STROOPS_PER_USDC);
        assert_eq!(pos.locked_collateral, 42 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_net_position() {
        assert_eq!(MarketContract::calculate_net_position(100, 30), 70);
        assert_eq!(MarketContract::calculate_net_position(30, 100), -70);
        assert_eq!(MarketContract::calculate_net_position(50, 50), 0);
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

        assert!(MarketContract::can_settle(&position, &market));
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

        assert!(!MarketContract::can_settle(&position, &market));
    }

    // Optional smoke test
    #[test]
    fn test_update_position_smoke() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = <Address as TestAddress>::generate(&env);
        let market_id = String::from_str(&env, "smoke-market");

        let pos = env.as_contract(&contract_id, || {
            MarketContract::update_position(
                &env,
                &market_id,
                &user,
                250 * STROOPS_PER_USDC,
                80 * STROOPS_PER_USDC,
                7200,
            )
            .expect("smoke test should succeed")
        });

        assert!(pos.yes_shares > 0);
        assert!(pos.locked_collateral > 0);
    }
}
