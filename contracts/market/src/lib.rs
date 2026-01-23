#![no_std]

mod error;
mod events;
mod oracle;
mod positions;
#[allow(dead_code)]
mod settlement;

#[allow(dead_code)]
mod storage;
mod test;
mod types;
#[allow(dead_code)]
mod validation;

use soroban_sdk::{contract, contractimpl, Address, Env, String};

use crate::{
    error::ContractError,
    types::{Position, Market},
};

const BASIS_POINTS: i128 = 10_000;
const STROOPS_PER_USDC: i128 = 10_000_000;

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Calculate required locked collateral based on net position
    ///
    /// Logic:
    /// - Net YES  => lock net_yes * price
    /// - Net NO   => lock net_no * (1 - price)
    /// - Hedged   => lock 0
    pub fn calculate_locked_collateral(
        yes_shares: i128,
        no_shares: i128,
        market_price: i128,
    ) -> i128 {
        if yes_shares == no_shares {
            return 0;
        }

        if yes_shares > no_shares {
            let net_yes = yes_shares - no_shares;
            net_yes
                .checked_mul(market_price)
                .unwrap()
                .checked_div(BASIS_POINTS)
                .unwrap()
        } else {
            let net_no = no_shares - yes_shares;
            let inverse_price = BASIS_POINTS - market_price;
            net_no
                .checked_mul(inverse_price)
                .unwrap()
                .checked_div(BASIS_POINTS)
                .unwrap()
        }
    }

    /// Validate whether a proposed position change is allowed
    pub fn validate_position_change(
        current_position: &Position,
        yes_delta: i128,
        no_delta: i128,
    ) -> Result<(), ContractError> {
        let new_yes = current_position.yes_shares + yes_delta;
        let new_no = current_position.no_shares + no_delta;

        if new_yes < 0 || new_no < 0 {
            return Err(ContractError::InvalidShareAmount);
        }

        Ok(())
    }

    /// Update a user's position with new share deltas
  pub fn update_position(
    env: &Env,
    market_id: &String,
    user: &Address,
    yes_delta: i128,
    no_delta: i128,
    market_price: i128,
) -> Result<Position, ContractError> {
    // 1. Load or initialize position safely
    let mut position = crate::storage::get_position(env, market_id, user)
        .unwrap_or_else(|| Position {
            market_id: market_id.clone(),
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: false,
        });

    // 2. Validate deltas
    Self::validate_position_change(&position, yes_delta, no_delta)?;

    // 3. Apply deltas
    position.yes_shares += yes_delta;
    position.no_shares += no_delta;

    // 4. Recalculate locked collateral
    let new_locked = Self::calculate_locked_collateral(
        position.yes_shares,
        position.no_shares,
        market_price,
    );

    position.locked_collateral = new_locked;

    // 5. Persist
    crate::storage::set_position(env, market_id, user, &position);

    Ok(position)
}

    /// Calculate net position from YES and NO shares
    ///
    /// Positive  => net long YES
    /// Negative  => net long NO
    /// Zero      => hedged
    pub fn calculate_net_position(yes_shares: i128, no_shares: i128) -> i128 {
        yes_shares - no_shares
    }

    /// Check if a position is eligible for settlement
    pub fn can_settle(position: &Position, market: &Market) -> bool {
        use crate::types::MarketStatus;
        matches!(market.status, MarketStatus::Resolved) && !position.is_settled
    }
}
