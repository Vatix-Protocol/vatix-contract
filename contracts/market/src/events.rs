use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

// Event topic symbols (max 10 chars for symbol_short!)
const MARKET_CREATED: Symbol = symbol_short!("MKT_CREAT");
const MARKET_RESOLVED: Symbol = symbol_short!("MKT_RESOL");
const POSITION_UPDATED: Symbol = symbol_short!("POS_UPD");
const POSITION_SETTLED: Symbol = symbol_short!("POS_SETT");
const COLLATERAL_DEPOSITED: Symbol = symbol_short!("COLLAT_DEP");
const COLLATERAL_WITHDRAWN: Symbol = symbol_short!("COLLAT_WD");

/// Emit event when a new market is created
///
/// # Event Data
/// - Topic 1: "MKT_CREAT" (market created symbol)
/// - Topic 2: market_id
/// - Data: (question, end_time, creator)
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique market identifier
/// * `question` - Market question
/// * `end_time` - Market end timestamp
/// * `creator` - Address that created the market
pub fn emit_market_created(
    env: &Env,
    market_id: &String,
    question: &String,
    end_time: u64,
    creator: &Address,
) {
    env.events().publish(
        (MARKET_CREATED, market_id.clone()),
        (question.clone(), end_time, creator.clone()),
    );
}

/// Emit event when a market is resolved
///
/// # Event Data
/// - Topic 1: "MKT_RESOL" (market resolved symbol)
/// - Topic 2: market_id
/// - Data: (outcome, resolved_at)
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `outcome` - Resolution outcome (true = YES, false = NO)
/// * `resolved_at` - Resolution timestamp
pub fn emit_market_resolved(
    env: &Env,
    market_id: &String,
    outcome: bool,
    resolved_at: u64,
) {
    env.events().publish(
        (MARKET_RESOLVED, market_id.clone()),
        (outcome, resolved_at),
    );
}

/// Emit event when a user's position changes
///
/// # Event Data
/// - Topic 1: "POS_UPD" (position updated symbol)
/// - Topic 2: market_id
/// - Topic 3: user address
/// - Data: (yes_shares, no_shares, locked_collateral)
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `yes_shares` - New YES shares amount
/// * `no_shares` - New NO shares amount
/// * `locked_collateral` - New locked collateral amount
pub fn emit_position_updated(
    env: &Env,
    market_id: &String,
    user: &Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128,
) {
    env.events().publish(
        (POSITION_UPDATED, market_id.clone(), user.clone()),
        (yes_shares, no_shares, locked_collateral),
    );
}

/// Emit event when a position is settled
///
/// # Event Data
/// - Topic 1: "POS_SETT" (position settled symbol)
/// - Topic 2: market_id
/// - Topic 3: user address
/// - Data: (payout, settled_at)
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `payout` - Amount paid out in stroops
/// * `settled_at` - Settlement timestamp
pub fn emit_position_settled(
    env: &Env,
    market_id: &String,
    user: &Address,
    payout: i128,
    settled_at: u64,
) {
    env.events().publish(
        (POSITION_SETTLED, market_id.clone(), user.clone()),
        (payout, settled_at),
    );
}

/// Emit event when user deposits collateral
///
/// # Event Data
/// - Topic 1: "COLLAT_DEP" (collateral deposited symbol)
/// - Topic 2: market_id
/// - Topic 3: user address
/// - Data: amount
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `amount` - Amount deposited in stroops
pub fn emit_collateral_deposited(
    env: &Env,
    market_id: &String,
    user: &Address,
    amount: i128,
) {
    env.events().publish(
        (COLLATERAL_DEPOSITED, market_id.clone(), user.clone()),
        amount,
    );
}

/// Emit event when user withdraws collateral
///
/// # Event Data
/// - Topic 1: "COLLAT_WD" (collateral withdrawn symbol)
/// - Topic 2: market_id
/// - Topic 3: user address
/// - Data: amount
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `amount` - Amount withdrawn in stroops
pub fn emit_collateral_withdrawn(
    env: &Env,
    market_id: &String,
    user: &Address,
    amount: i128,
) {
    env.events().publish(
        (COLLATERAL_WITHDRAWN, market_id.clone(), user.clone()),
        amount,
    );
}
