use super::*;
use soroban_sdk::{testutils::Address as TestAddress, Address, Env, String, BytesN};

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
        let locked =
            MarketContract::calculate_locked_collateral(100 * STROOPS_PER_USDC, 100 * STROOPS_PER_USDC, 6000);
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
        let user = sample_user(&env, 1);
        let market_id = String::from_str(&env, "market1");

        let pos = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            100 * STROOPS_PER_USDC,
            0,
            6000,
        )
        .expect("should update position");

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 0);
        assert_eq!(pos.locked_collateral, 60 * STROOPS_PER_USDC);
        assert!(!pos.is_settled);
    }

    #[test]
    fn test_update_position_existing_user() {
        let env = setup_env();
        let user = sample_user(&env, 2);
        let market_id = String::from_str(&env, "market2");

        // First update - buy YES
        let _ = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            100 * STROOPS_PER_USDC,
            0,
            6000,
        )
        .unwrap();

        // Second update - buy some NO
        let pos = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            0,
            30 * STROOPS_PER_USDC,
            6000,
        )
        .unwrap();

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
        let user = <Address as TestAddress>::generate(&env);
        let market_id = String::from_str(&env, "smoke-market");

        let pos = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            250 * STROOPS_PER_USDC,
            80 * STROOPS_PER_USDC,
            7200,
        )
        .expect("smoke test should succeed");

        assert!(pos.yes_shares > 0);
        assert!(pos.locked_collateral > 0);
    }
}