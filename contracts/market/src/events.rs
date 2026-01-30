//! Event emission functions for the Vatix prediction market contract

use soroban_sdk::{contractevent, Address, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCreatedEvent {
    #[topic]
    pub market_id: u32,
    pub question: String,
    pub end_time: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralDepositedEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralWithdrawnEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

/// Emit event when collateral is deposited
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount deposited in stroops
/// * new_total - User's total collateral in this market after deposit
pub fn emit_collateral_deposited(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralDepositedEvent {
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Emit event when collateral is withdrawn
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount withdrawn in stroops
/// * new_total - User's total collateral in this market after withdrawal
#[allow(dead_code)]
pub fn emit_collateral_withdrawn(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralWithdrawnEvent {
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Emit a MarketCreated event
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the created market
/// * question - The market question
/// * end_time - Unix timestamp when market closes for trading
pub fn emit_market_created(env: &Env, market_id: u32, question: &String, end_time: u64) {
    // Publish the event with topics and data
    MarketCreatedEvent {
        market_id,
        question: question.clone(),
        end_time,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketResolvedEvent {
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub resolved_at: u64,
}

/// Emit a MarketResolved event
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the resolved market
/// * outcome - Market outcome (true = YES won, false = NO won)
/// * resolved_at - Unix timestamp when market was resolved
pub fn emit_market_resolved(env: &Env, market_id: u32, outcome: bool, resolved_at: u64) {
    MarketResolvedEvent {
        market_id,
        outcome,
        resolved_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionUpdatedEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    pub locked_collateral: i128,
}

#[allow(dead_code)]
pub fn emit_position_updated(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128,
) {
    PositionUpdatedEvent {
        market_id,
        user: user.clone(),
        yes_shares,
        no_shares,
        locked_collateral,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionSettledEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub payout: i128,
    pub settled_at: u64,
}

#[allow(dead_code)]
pub fn emit_position_settled(
    env: &Env,
    market_id: u32,
    user: &Address,
    payout: i128,
    settled_at: u64,
) {
    PositionSettledEvent {
        market_id,
        user: user.clone(),
        payout,
        settled_at,
    }
    .publish(env);
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarketContract;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Env, IntoVal, Map, String, Symbol, TryIntoVal, Val,
    };

    #[test]
    fn test_emit_market_created() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let question = String::from_str(&env, "Will BTC hit $100k?");
        let end_time = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_created(&env, market_id, &question, end_time);
        });

        // Verify event was published
        let events = env.events().all();
        assert_eq!(events.len(), 1);

        // Verify event content
        let event = events.first().unwrap();
        // event is (Contract Address, Topics, Data)

        // Topics
        let topics = &event.1;
        assert_eq!(topics.len(), 2);

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_created_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        // Data
        // Data
        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let question_val: String = data
            .get(Symbol::new(&env, "question"))
            .unwrap()
            .into_val(&env);
        let end_time_val: u64 = data
            .get(Symbol::new(&env, "end_time"))
            .unwrap()
            .into_val(&env);
        assert_eq!(question_val, question);
        assert_eq!(end_time_val, end_time);
    }

    #[test]
    fn test_emit_market_resolved() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let outcome = true;
        let resolved_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_resolved(&env, market_id, outcome, resolved_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_resolved_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let outcome_val: bool = data
            .get(Symbol::new(&env, "outcome"))
            .unwrap()
            .into_val(&env);
        let resolved_at_val: u64 = data
            .get(Symbol::new(&env, "resolved_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(outcome_val, outcome);
        assert_eq!(resolved_at_val, resolved_at);
    }

    #[test]
    fn test_emit_position_updated() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let yes_shares = 100i128;
        let no_shares = 50i128;
        let locked_collateral = 150i128;

        env.as_contract(&contract_id, || {
            emit_position_updated(
                &env,
                market_id,
                &user,
                yes_shares,
                no_shares,
                locked_collateral,
            );
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_updated_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let yes_shares_val: i128 = data
            .get(Symbol::new(&env, "yes_shares"))
            .unwrap()
            .into_val(&env);
        let no_shares_val: i128 = data
            .get(Symbol::new(&env, "no_shares"))
            .unwrap()
            .into_val(&env);
        let locked_collateral_val: i128 = data
            .get(Symbol::new(&env, "locked_collateral"))
            .unwrap()
            .into_val(&env);

        assert_eq!(yes_shares_val, yes_shares);
        assert_eq!(no_shares_val, no_shares);
        assert_eq!(locked_collateral_val, locked_collateral);
    }

    #[test]
    fn test_emit_position_settled() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let payout = 100i128;
        let settled_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_position_settled(&env, market_id, &user, payout, settled_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_settled_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let payout_val: i128 = data
            .get(Symbol::new(&env, "payout"))
            .unwrap()
            .into_val(&env);
        let settled_at_val: u64 = data
            .get(Symbol::new(&env, "settled_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(payout_val, payout);
        assert_eq!(settled_at_val, settled_at);
    }

    #[test]
    fn test_emit_collateral_deposited() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 1000i128;
        let new_total = 1000i128;

        env.as_contract(&contract_id, || {
            emit_collateral_deposited(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_deposited_event"));

        let topic1: Address = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, user);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }

    #[test]
    fn test_emit_collateral_withdrawn() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 500i128;
        let new_total = 500i128;

        env.as_contract(&contract_id, || {
            emit_collateral_withdrawn(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_withdrawn_event"));

        let topic1: Address = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, user);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }
}