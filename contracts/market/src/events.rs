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
/// * `env` - Soroban environment
/// * `user` - User's address
/// * `market_id` - Market identifier
/// * `amount` - Amount deposited in stroops
/// * `new_total` - User's total collateral in this market after deposit
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
/// * `env` - Soroban environment
/// * `user` - User's address
/// * `market_id` - Market identifier
/// * `amount` - Amount withdrawn in stroops
/// * `new_total` - User's total collateral in this market after withdrawal
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
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the created market
/// * `question` - The market question
/// * `end_time` - Unix timestamp when market closes for trading
pub fn emit_market_created(env: &Env, market_id: u32, question: &String, end_time: u64) {
    // Publish the event with topics and data
    MarketCreatedEvent {
        market_id,
        question: question.clone(),
        end_time,
    }
    .publish(env);
}
