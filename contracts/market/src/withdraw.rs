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
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec};

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

    let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;

    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
    }

    let mut position = storage::get_position(&env, market_id, &user)?
        .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));

    if position.total_deposited == 0 {
        emit_withdraw_edge_case(&env, &user, market_id, amount);
        return Err(ContractError::InsufficientCollateral);
    }

    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);

    let fee_rate_bps = storage::get_fee_rate_bps(&env);
    let fee_amount: i128 = if fee_rate_bps > 0 {
        amount
            .checked_mul(fee_rate_bps)
            .ok_or(ContractError::ArithmeticOverflow)?
            / 10_000
    } else {
        0
    };

    let required_lock = position.locked_collateral;
    let total_deposited_after_fee = position
        .total_deposited
        .checked_sub(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    // Compute the protocol fee on the requested withdrawal using the configured
    // fee rate (basis points). A zero (or unset) rate yields a zero fee.
    let fee_rate_bps = storage::get_fee_rate_bps(&env);
    validation::validate_fee_rate_bps(fee_rate_bps)?;
    let fee_amount = if fee_rate_bps > 0 {
        validation::calculate_fee(amount, fee_rate_bps)?
    } else {
        0
    };

    // Collateral available to withdraw is what remains after reserving the fee
    // and the amount locked to back the user's current YES/NO shares.
    let total_deposited_after_fee = position
        .total_deposited
        .checked_sub(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    let available = if total_deposited_after_fee > required_lock {
        total_deposited_after_fee - required_lock
    } else {
        0
    };

    emit_fee_calculated(&env, market_id, &user, fee_amount, available);

    if amount > available {
        return Err(ContractError::InsufficientCollateral);
    }

    // Route non-zero fees to the treasury contract if one has been registered.
    if fee_amount > 0 {
        if let Some(treasury_addr) = storage::get_treasury(&env) {
            token_client.transfer(&contract_address, &treasury_addr, &fee_amount);
    // Record the fee calculation for off-chain indexers. The emitted amount is
    // now non-zero whenever a fee rate is configured.
    emit_fee_calculated(&env, market_id, &user, fee_amount, available);

    // The user must have `amount + fee_amount` of unlocked collateral so that
    // the net transfer to them remains exactly `amount`.
    let total_required = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    if total_required > available {
        return Err(ContractError::InsufficientCollateral);
    }

    // Route a non-zero fee to the treasury contract when one is registered:
    // transfer the fee tokens, then record the deposit via the treasury's
    // `collect_fee` entry point. `invoke_contract` is used (rather than a
    // compile-time client) to avoid a hard crate dependency on the treasury.
    if fee_amount > 0 {
        if let Some(treasury_addr) = storage::get_treasury(&env) {
            token_client.transfer(&contract_address, &treasury_addr, &fee_amount);

            let args: Vec<Val> = soroban_sdk::vec![
                &env,
                contract_address.into_val(&env),
                market.collateral_token.clone().into_val(&env),
                market_id.into_val(&env),
                fee_amount.into_val(&env),
            ];
            let _: () = env.invoke_contract(
                &treasury_addr,
                &Symbol::new(&env, "collect_fee"),
                args,
            );
        }
    }

    let total_deducted = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    // Deduct the full amount (including fee) from the user's deposited balance.
    position.total_deposited = position
        .total_deposited
        .checked_sub(total_deducted)
        .ok_or(ContractError::ArithmeticOverflow)?;

    storage::set_position(&env, market_id, &user, &position)?;

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
            price_bps: 5_000,
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
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

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
        });

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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
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
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
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
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });

        env.mock_all_auths();

        env.as_contract(&contract_id, || {
            let result = withdraw_unused_collateral(env.clone(), user.clone(), market_id, 100);
            assert_eq!(result, Err(ContractError::InsufficientCollateral));

            let events = env.events().all();
            assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());
        });
    }

    #[test]
    fn test_withdraw_fee_zero_bps() {
        use soroban_sdk::token::StellarAssetClient;

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
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
            storage::set_fee_rate_bps(&env, 0); // 0 bps
        });

        env.mock_all_auths();

        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &200);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(updated.total_deposited, 60); // 100 - 40 - 0 = 60
    }

    #[test]
    fn test_withdraw_fee_max_bps() {
        use soroban_sdk::token::StellarAssetClient;

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
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
            storage::set_fee_rate_bps(&env, 10000); // 10000 bps (100% fee)
        });

        env.mock_all_auths();

        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &200);

        // Try to withdraw 50. Fee will be calculate_fee(50, 10000) = 50.
        // Total deduction will be 100.
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 50)
        });
        assert!(result.is_ok());

        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(updated.total_deposited, 0); // 100 - 50 - 50 = 0
    }

    #[test]
    fn test_withdraw_fee_insufficient_after_fee() {
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
            locked_collateral: 50,
            total_deposited: 100, // available before fee = 50
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
            storage::set_position(&env, market_id, &user, &position);
            storage::set_fee_rate_bps(&env, 1000); // 10% fee
        });

        env.mock_all_auths();

        // Withdraw 48. Fee is calculate_fee(48, 1000) = 4.
        // total_deposited_after_fee = 96.
        // available = 96 - 50 = 46.
        // 48 > 46, so should fail with InsufficientCollateral.
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 48)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_fee_holds_in_contract_when_no_treasury() {
        use soroban_sdk::token::StellarAssetClient;

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
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
            storage::set_fee_rate_bps(&env, 1000); // 10% fee
            // no treasury address is set
        });

        env.mock_all_auths();

        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &100);

        let user_token_client = soroban_sdk::token::Client::new(&env, &collateral_token);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        // Position reduced by 44 (40 withdrawal + 4 fee)
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(updated.total_deposited, 56);

        // User receives 40
        assert_eq!(user_token_client.balance(&user), 40);

        // Contract retains the fee of 4 (original 100 mint - 40 user withdraw = 60 contract balance)
        assert_eq!(user_token_client.balance(&contract_id), 60);
    }

    #[test]
    fn test_withdraw_fee_routes_to_treasury() {
        use soroban_sdk::token::StellarAssetClient;

        let env = setup_env();
        let user = Address::generate(&env);
        let treasury = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
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
            storage::set_fee_rate_bps(&env, 1000); // 10% fee
            storage::set_treasury(&env, &Some(treasury.clone()));
        });

        env.mock_all_auths();

        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&contract_id, &100);

        let user_token_client = soroban_sdk::token::Client::new(&env, &collateral_token);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        // Position reduced by 44 (40 withdrawal + 4 fee)
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(updated.total_deposited, 56);

        // User receives 40
        assert_eq!(user_token_client.balance(&user), 40);

        // Treasury receives 4
        assert_eq!(user_token_client.balance(&treasury), 4);

        // Contract balance decreases by 44 (original 100 - 44 = 56)
        assert_eq!(user_token_client.balance(&contract_id), 56);
    }
}
