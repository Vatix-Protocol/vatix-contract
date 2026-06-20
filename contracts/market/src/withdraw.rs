//! Withdraw unused collateral from a market.
//!
//! Users can withdraw collateral that is not locked by their active positions.
//! `Position::locked_collateral` is the single source of truth for how much
//! collateral is currently required to back a user's YES/NO shares: it is
//! computed and persisted exclusively by `positions::update_position` (via
//! `calculate_locked_collateral`) at the real trade price. Withdraw must
//! trust that stored value rather than recompute its own lock from a
//! hardcoded price, otherwise its view of "locked" can diverge from what was
//! actually locked when shares were bought at a price other than 50/50.

use crate::error::ContractError;
use crate::events::{emit_collateral_withdrawn, emit_fee_calculated, emit_withdraw_edge_case};
use crate::storage;
use crate::types::{MarketStatus, Position};
use crate::validation;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{vec, Address, Env, IntoVal, Symbol, Val};

/// Protocol fee rate in basis points (50 bps = 0.5%).
///
/// Applied to every withdrawal when a treasury contract is registered.
/// Fee is rounded down; zero-fee withdrawals still succeed.
pub(crate) const FEE_BPS: i128 = 50;

/// Denominator for basis-point arithmetic.
const BPS_DENOM: i128 = 10_000;

/// Withdraw unused collateral from a market.
///
/// Reads the collateral already locked by the user's YES/NO shares from
/// `Position::locked_collateral` (kept up to date by `update_position`), then
/// allows withdrawing any remaining balance.
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

    let mut position = storage::get_position(&env, market_id, &user)
        .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));

    if position.total_deposited == 0 {
        emit_withdraw_edge_case(&env, &user, market_id, amount);
        return Err(ContractError::InsufficientCollateral);
    }

    let required_lock = position.locked_collateral;

    // Available collateral is total deposited minus the amount required to back shares.
    let available = if position.total_deposited > required_lock {
        position.total_deposited - required_lock
    } else {
        0
    };

    // Compute the protocol fee.  When a treasury is registered, fee_amount is
    // deducted from the user's withdrawal; when no treasury is set, fee is 0.
    let treasury_opt = storage::get_treasury_contract(&env);
    let fee_amount = if treasury_opt.is_some() {
        // floor division — user always receives at least amount - fee_amount
        amount
            .checked_mul(FEE_BPS)
            .and_then(|v| v.checked_div(BPS_DENOM))
            .ok_or(ContractError::ArithmeticOverflow)?
    } else {
        0
    };

    // The user must have `amount + fee_amount` of unlocked collateral so that
    // the net transfer to them remains exactly `amount`.
    let total_required = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    emit_fee_calculated(&env, market_id, &user, fee_amount, available);

    if total_required > available {
        return Err(ContractError::InsufficientCollateral);
    }

    // Deduct the full amount (including fee) from the user's deposited balance.
    position.total_deposited = position
        .total_deposited
        .checked_sub(total_required)
        .ok_or(ContractError::ArithmeticOverflow)?;

    storage::set_position(&env, market_id, &user, &position);

    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);

    // Forward fee to treasury via cross-contract call (only when configured).
    if let Some(ref treasury) = treasury_opt {
        if fee_amount > 0 {
            // Grant treasury an allowance so collect_fee can use transfer_from
            // to pull the fee without requiring market's auth from a sub-invocation.
            token_client.approve(
                &contract_address,
                treasury,
                &fee_amount,
                &(env.ledger().sequence() + 1),
            );

            let fn_name = Symbol::new(&env, "collect_fee");
            let args: soroban_sdk::Vec<Val> = vec![
                &env,
                contract_address.clone().into_val(&env),
                market.collateral_token.clone().into_val(&env),
                fee_amount.into_val(&env),
            ];
            env.invoke_contract::<()>(treasury, &fn_name, args);
        }
    }

    // Transfer the net withdrawal amount to the user.
    token_client.transfer(&contract_address, &user, &amount);

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

        // Fund the contract with collateral (simulates prior deposit)
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &200);

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

        env.as_contract(&contract_id, || {
            let result = withdraw_unused_collateral(env.clone(), user.clone(), market_id, 100);
            assert_eq!(result, Err(ContractError::InsufficientCollateral));

            let events = env.events().all();
            assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());
        });
    }
}
