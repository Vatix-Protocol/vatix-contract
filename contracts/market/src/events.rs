use soroban_sdk::{contractevent, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCreatedEvent {
    #[topic]
    pub market_id: u32,
    pub question: String,
    pub end_time: u64,
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
