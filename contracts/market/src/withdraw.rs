//! Withdraw unused collateral from a market.
//!
//! Users can withdraw collateral that is not locked by their active positions.
//! Locked collateral is computed from YES/NO shares using a 50/50 market price.

use crate::error::ContractError;
use crate::events::{emit_collateral_withdrawn, emit_withdraw_edge_case};
use crate::positions::calculate_locked_collateral;
use crate::storage;
use crate::types::{MarketStatus, Position};
use crate::validation;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env};

/// Market price used for locked collateral calculation (50/50 = 5000 basis points).
const MARKET_PRICE_BPS: i128 = 5_000;

/// Withdraw unused collateral from a market.
///
/// Computes how much collateral is locked by the user's YES/NO shares at the
/// current 50/50 market price, then allows withdrawing any remaining balance.
///
/// # Arguments
/// * `env` - Contract environment
/// * `user` - User withdrawing (must authorize the call)
/// * `market_id` - Market to withdraw from
/// * `amount` - Amount to withdraw in stroops (1 USDC = 10^7 stroops)
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// - `MarketNotFound`: The market does not exist
/// - `MarketNotActive`: The market is resolved or canceled
/// - `InsufficientCollateral`: `amount` exceeds unlocked collateral
/// - `InvalidQuantity`: `amount` is zero or negative
/// - `ArithmeticOverflow`: Subtracting `amount` would overflow
///
/// # Events
/// Emits `CollateralWithdrawn` with the user, market, amount, and new total.
///
/// # Examples
/// ```
/// // Withdraw 0.1 USDC (1_000_000 stroops) from an active market when the user
/// // has at least that much unlocked collateral:
/// withdraw_unused_collateral(env, user, market_id, 1_000_000)?;
/// ```
pub fn withdraw_unused_collateral(
    env: Env,
    user: Address,
    market_id: u32,
    amount: i128,
) -> Result<(), ContractError> {
    user.require_auth();

    validation::validate_collateral_amount(amount)?;

    let market = storage::get_market(&env, market_id).ok_or(ContractError::MarketNotFound)?;

    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
    }

    let mut position = storage::get_position(&env, market_id, &user).unwrap_or_else(|| Position {
        market_id,
        user: user.clone(),
        yes_shares: 0,
        no_shares: 0,
        locked_collateral: 0,
        total_deposited: 0,
        is_settled: false,
    });

    if position.total_deposited == 0 {
        emit_withdraw_edge_case(&env, &user, market_id, amount);
        return Err(ContractError::InsufficientCollateral);
    }

    // TODO(#85): fee deduction should be applied here before computing available collateral
    // See: https://github.com/Vatix-Protocol/vatix-contract/issues/85
    let required_lock =
        calculate_locked_collateral(position.yes_shares, position.no_shares, MARKET_PRICE_BPS);
    let available = position
        .total_deposited
        .checked_sub(required_lock)
        .unwrap_or(0)
        .max(0);

    if amount > available {
        return Err(ContractError::InsufficientCollateral);
    }

    position.total_deposited = position
        .total_deposited
        .checked_sub(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    storage::set_position(&env, market_id, &user, &position);

    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);
    token_client.transfer(&contract_address, &user, &amount);

    // TODO(#issue): consider batching withdrawal events for gas efficiency
    emit_collateral_withdrawn(&env, &user, market_id, amount, position.total_deposited);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Market;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

    fn setup_env() -> Env {
        Env::default()
    }

    fn create_test_market(env: &Env, market_id: u32, collateral_token: &Address) -> Market {
        Market {
            id: market_id,
            question: String::from_str(env, "Will it rain tomorrow?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: collateral_token.clone(),
        }
    }

    #[test]
    fn test_withdraw_validates_zero_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 0)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_withdraw_validates_negative_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, -100)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_withdraw_validates_market_not_found() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 999u32;
        let contract_id = env.register(crate::MarketContract, ());

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotFound));
    }

    #[test]
    fn test_withdraw_validates_market_not_active_resolved() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let mut market = create_test_market(&env, market_id, &collateral_token);
        market.status = MarketStatus::Resolved;
        market.result = Some(true);
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 100)
        });

        assert_eq!(result, Err(ContractError::MarketNotActive));
    }

    #[test]
    fn test_withdraw_validates_insufficient_collateral() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 100, // net YES
            no_shares: 0,
            locked_collateral: 50, // required at 50/50 = 50
            total_deposited: 100,  // available = 100 - 50 = 50
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
        });

        env.mock_all_auths();

        // Try to withdraw 60 when only 50 is available
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 60)
        });

        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_zero_deposited_rejected() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 1)
        });

        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_no_position_insufficient_collateral() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            // No position - available = 0
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 1)
        });

        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_available_vs_locked() {
        // total_deposited 100, required_lock 60 (e.g. net YES 120 at 50%) -> available 40
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 120,
            no_shares: 0,
            locked_collateral: 60, // 120 * 5000/10000 = 60
            total_deposited: 100,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
        });

        env.mock_all_auths();

        // Withdraw 41 > 40 available -> InsufficientCollateral
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 41)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_success_updates_position_and_transfers() {
        use soroban_sdk::token::StellarAssetClient;

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        // Fund the contract with collateral (simulates prior deposit)
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &200);

        let market = create_test_market(&env, market_id, &collateral_token);
        // No shares held -> required_lock = 0, available = total_deposited = 100
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 100,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        // Position total_deposited reduced by withdrawn amount
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).expect("position should exist")
        });
        assert_eq!(updated.total_deposited, 60);
    }

    #[test]
    fn test_withdraw_edge_case_emits_event() {
        use soroban_sdk::testutils::Events;

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
        });

        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 100)
        });

        assert_eq!(result, Err(ContractError::InsufficientCollateral));

        let events = env.events().all();
        let withdraw_edge_case_events: Vec<_> = events
            .iter()
            .filter(|(event, _)| {
                // Check for WithdrawEdgeCaseEvent
                event.topics.len() == 2
                    && event.contract_id == contract_id
            })
            .collect();

        assert_eq!(withdraw_edge_case_events.len(), 1);
    }
}

