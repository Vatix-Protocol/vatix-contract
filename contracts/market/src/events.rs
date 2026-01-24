//! Event emission functions for the Vatix prediction market contract

use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

const COLLATERAL_DEPOSITED: Symbol = symbol_short!("c_dep");
const COLLATERAL_WITHDRAWN: Symbol = symbol_short!("c_wdr");

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
    market_id: &String,
    amount: i128,
    new_total: i128,
) {
    env.events().publish(
        (COLLATERAL_DEPOSITED, user.clone()),
        (market_id.clone(), amount, new_total),
    );
}

/// Emit event when collateral is withdrawn
///
/// # Arguments
/// * `env` - Soroban environment
/// * `user` - User's address
/// * `market_id` - Market identifier
/// * `amount` - Amount withdrawn in stroops
/// * `new_total` - User's total collateral in this market after withdrawal
pub fn emit_collateral_withdrawn(
    env: &Env,
    user: &Address,
    market_id: &String,
    amount: i128,
    new_total: i128,
) {
    env.events().publish(
        (COLLATERAL_WITHDRAWN, user.clone()),
        (market_id.clone(), amount, new_total),
    );
}
