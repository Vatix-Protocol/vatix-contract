#[cfg(test)]
mod events_test {
    use crate::events::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Env, String,
    };

    #[test]
    fn test_emit_market_created() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let question = String::from_str(&env, "Will BTC reach $100k?");
        let end_time = 1234567890u64;
        let creator = Address::generate(&env);

        emit_market_created(&env, &market_id, &question, end_time, &creator);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_emit_market_resolved() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let outcome = true;
        let resolved_at = 1234567890u64;

        emit_market_resolved(&env, &market_id, outcome, resolved_at);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_emit_position_updated() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let user = Address::generate(&env);
        let yes_shares = 1000i128;
        let no_shares = 500i128;
        let locked_collateral = 1500i128;

        emit_position_updated(&env, &market_id, &user, yes_shares, no_shares, locked_collateral);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_emit_position_settled() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let user = Address::generate(&env);
        let payout = 2000i128;
        let settled_at = 1234567890u64;

        emit_position_settled(&env, &market_id, &user, payout, settled_at);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_emit_collateral_deposited() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let user = Address::generate(&env);
        let amount = 1000i128;

        emit_collateral_deposited(&env, &market_id, &user, amount);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_emit_collateral_withdrawn() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let user = Address::generate(&env);
        let amount = 500i128;

        emit_collateral_withdrawn(&env, &market_id, &user, amount);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_multiple_events() {
        let env = Env::default();
        let market_id = String::from_str(&env, "1");
        let user = Address::generate(&env);
        let creator = Address::generate(&env);
        let question = String::from_str(&env, "Test market");

        // Emit multiple events
        emit_market_created(&env, &market_id, &question, 1234567890, &creator);
        emit_collateral_deposited(&env, &market_id, &user, 1000);
        emit_position_updated(&env, &market_id, &user, 500, 500, 1000);

        let events = env.events().all();
        assert_eq!(events.len(), 3);
    }
}