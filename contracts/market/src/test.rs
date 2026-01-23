use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_env() -> Env {
        Env::default()
    }

    fn sample_user(env: &Env, id: u8) -> Address {
        Address::from_account_id(&env, &[id; 32])
    }

    fn sample_market() -> Market {
        Market {
            id: "market1".into(),
            question: "Will it rain tomorrow?".into(),
            end_time: 0,
            oracle_pubkey: [0; 32].into(),
            status: types::MarketStatus::Resolved,
        }
    }

    #[test]
    fn test_calculate_locked_collateral_net_yes() {
        let locked = MarketContract::calculate_locked_collateral(100 * STROOPS_PER_USDC, 0, 6000);
        assert_eq!(locked, 60 * STROOPS_PER_USDC);

        let locked = MarketContract::calculate_locked_collateral(100 * STROOPS_PER_USDC, 30 * STROOPS_PER_USDC, 5000);
        // Net YES = 70, price = 50% → 35 USDC
        assert_eq!(locked, 35 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_net_no() {
        let locked = MarketContract::calculate_locked_collateral(0, 100 * STROOPS_PER_USDC, 6000);
        // Net NO = 100, price = 60% → lock 100 * (100-60)/100 = 40 USDC
        assert_eq!(locked, 40 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_hedged() {
        let locked = MarketContract::calculate_locked_collateral(100 * STROOPS_PER_USDC, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 0);
    }

    #[test]
    fn test_validate_position_change() {
        let position = Position {
            market_id: "m1".into(),
            user: sample_user(&Env::default(), 1),
            yes_shares: 50,
            no_shares: 50,
            locked_collateral: 0,
            is_settled: false,
        };

        // Valid change
        assert!(MarketContract::validate_position_change(&position, 10, -20).is_ok());

        // Invalid: would make YES negative
        assert!(MarketContract::validate_position_change(&position, -60, 0).is_err());

        // Invalid: would make NO negative
        assert!(MarketContract::validate_position_change(&position, 0, -60).is_err());
    }

    #[test]
    fn test_update_position_new_user() {
        let env = setup_env();
        let user = sample_user(&env, 1);
        let market_id = "market1".to_string();

        let pos = MarketContract::update_position(&env, &market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000)
            .expect("should update position");

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 0);
        assert_eq!(pos.locked_collateral, 60 * STROOPS_PER_USDC);
        assert_eq!(pos.is_settled, false);
    }

    #[test]
    fn test_update_position_existing_user() {
        let env = setup_env();
        let user = sample_user(&env, 2);
        let market_id = "market2".to_string();

        // First update
        let _ = MarketContract::update_position(&env, &market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000).unwrap();

        // Second update: add NO shares
        let pos = MarketContract::update_position(&env, &market_id, &user, 0, 30 * STROOPS_PER_USDC, 6000).unwrap();

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 30 * STROOPS_PER_USDC);
        // Net YES = 70 → 70 * 60% = 42 USDC
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
        let market = sample_market();
        let position = Position {
            market_id: "m1".into(),
            user: sample_user(&Env::default(), 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: false,
        };

        assert!(MarketContract::can_settle(&position, &market));
    }

    #[test]
    fn test_can_settle_already_settled() {
        let market = sample_market();
        let mut position = Position {
            market_id: "m1".into(),
            user: sample_user(&Env::default(), 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: true,
        };

        assert!(!MarketContract::can_settle(&position, &market));
    }
}
