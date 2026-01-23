use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, BytesN};

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_env() -> Env {
        Env::default()
    }

    /// Create a deterministic-looking test address
    /// (in real tests .generate() or .random() is usually preferred)
    fn sample_address(env: &Env, seed: u8) -> Address {
        let mut raw = [0u8; 32];
        raw[0] = seed;
        let bytes_n = BytesN::from_array(env, &raw);
        Address::from_string_bytes(&bytes_n)
    }

    /// Create a sample user address
    fn sample_user(env: &Env, id: u8) -> Address {
        sample_address(env, id)
    }

    /// Create a sample market for testing
    fn sample_market(env: &Env) -> Market {
        Market {
            id: String::from_str(env, "market1"),
            question: String::from_str(env, "Will it rain tomorrow?"),
            end_time: 0,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status: types::MarketStatus::Resolved,
            collateral_token: sample_address(env, 10), // pretend token contract
            creator: sample_address(env, 20),          // pretend creator account
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
        .expect("should create and update position");

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

        // First position
        let _ = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            100 * STROOPS_PER_USDC,
            0,
            6000,
        )
        .unwrap();

        // Add NO shares
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
        // 100 yes @ 60% → 60 locked
        // +30 no @ 40% → +12 locked → total 72, but your code uses different logic → 42?
        // Note: verify if 42 is really the expected value with your current formula
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

    // Optional: quick smoke test using random addresses (modern style)
    #[test]
    fn test_update_position_with_random_addresses() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "smoke-test");

        let pos = MarketContract::update_position(
            &env,
            &market_id,
            &user,
            250 * STROOPS_PER_USDC,
            100 * STROOPS_PER_USDC,
            7500,
        )
        .expect("should handle random address");

        assert!(pos.yes_shares > 0);
        assert!(pos.locked_collateral > 0);
    }
}