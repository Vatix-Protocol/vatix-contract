use soroban_sdk::{Env, String, Symbol};

/// Emit a MarketCreated event
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the created market
/// * `question` - The market question
/// * `end_time` - Unix timestamp when market closes for trading
#[allow(deprecated)]
pub fn emit_market_created(env: &Env, market_id: &String, question: &String, end_time: u64) {
    // Create event topics
    let topics = (Symbol::new(env, "MarketCreated"), market_id.clone());

    // Publish the event with topics and data
    env.events().publish(topics, (question.clone(), end_time));
}
