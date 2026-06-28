//! Withdraw unused collateral from a market.
//!
//! `Position::locked_collateral` is the single source of truth for how much
//! collateral backs the user's active YES/NO shares. It is computed and
//! persisted exclusively by `positions::update_position` at the real trade
//! price. Withdraw reads that stored value and never recomputes a lock from a
//! hardcoded price, so the two views can never diverge.
//!
//! ## Fee deduction (#377)
//! When a fee rate is configured the user must have `amount + fee` of unlocked
//! collateral available. The fee is routed to the treasury (if registered) and
//! both the withdrawal and the fee are deducted from `total_deposited` so the
//! invariant `available = total_deposited - locked_collateral` is preserved.

use crate::error::ContractError;
use crate::events::{emit_collateral_withdrawn, emit_fee_calculated, emit_withdraw_edge_case};
use crate::storage;
use crate::types::{MarketStatus, Position};
use crate::validation;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec};

/// Withdraw `amount` of unused (unlocked) collateral from a market.
///
/// # Locked-collateral enforcement (#376)
/// `available = total_deposited − locked_collateral`
/// The user may only withdraw up to `available − fee`. Any request that would
/// reduce the balance below the locked amount is rejected with
/// `InsufficientCollateral`.
///
/// # Fee deduction (#377)
/// The protocol fee is computed as `amount * fee_rate_bps / 10_000`. The check
/// is `amount + fee ≤ available`, so the user always receives exactly `amount`
/// and the fee is deducted on top — it is never silently subtracted from the
/// requested amount.
pub fn withdraw_unused_collateral(
    env: Env,
    user: Address,
    market_id: u32,
    amount: i128,
) -> Result<(), ContractError> {
    user.require_auth();

    // 1. Validate amount is positive and within safe range.
    validation::validate_collateral_amount(amount)?;

    // 2. Market must exist and be Active.
    let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
    }

    // 3. Load position; an absent or zero-deposited position cannot be withdrawn.
    let mut position = storage::get_position(&env, market_id, &user)?
        .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));

    if position.total_deposited == 0 {
        emit_withdraw_edge_case(&env, &user, market_id, amount);
        return Err(ContractError::InsufficientCollateral);
    }

    // 4. Compute the fee (single path, no duplication).
    let fee_rate_bps = storage::get_fee_rate_bps(&env);
    validation::validate_fee_rate_bps(fee_rate_bps)?;
    let fee_amount = if fee_rate_bps > 0 {
        validation::calculate_fee(amount, fee_rate_bps)?
    } else {
        0
    };

    // 5. Enforce locked collateral (#376).
    //    available = total_deposited - locked_collateral (floored at 0).
    //    The user must have `amount + fee_amount` of available (unlocked) collateral.
    let available = position
        .total_deposited
        .saturating_sub(position.locked_collateral);

    emit_fee_calculated(&env, market_id, &user, fee_amount, available);

    let total_required = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    if total_required > available {
        return Err(ContractError::InsufficientCollateral);
    }

    // 6. Route fee to treasury if one is registered.
    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);

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
        // When no treasury is registered the fee stays in the contract.
    }

    // 7. Deduct both withdrawal and fee from total_deposited.
    let total_deducted = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    position.total_deposited = position
        .total_deposited
        .checked_sub(total_deducted)
        .ok_or(ContractError::ArithmeticOverflow)?;

    storage::set_position(&env, market_id, &user, &position)?;

    // 8. Transfer the requested amount to the user.
    token_client.transfer(&contract_address, &user, &amount);

    emit_collateral_withdrawn(&env, &user, market_id, amount, position.total_deposited);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AdapterType, Market};
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
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
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
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || { storage::set_version(&env); });
        env.mock_all_auths();
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), 999, 1000)
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

    /// #376: withdrawing more than unlocked collateral must be rejected.
    #[test]
    fn test_withdraw_locked_collateral_enforced() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &collateral_token);
        // total_deposited=100, locked_collateral=60 → available=40
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 120,
            no_shares: 0,
            locked_collateral: 60,
            total_deposited: 100,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });
        env.mock_all_auths();
        // 41 > available(40) → InsufficientCollateral
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 41)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    /// #376: withdrawing exactly available collateral must succeed.
    #[test]
    fn test_withdraw_exactly_available_succeeds() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
        // locked=60, total=100 → available=40
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 120,
            no_shares: 0,
            locked_collateral: 60,
            total_deposited: 100,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().unwrap()
        });
        // total_deposited = 100 - 40 = 60 (still >= locked_collateral 60)
        assert_eq!(updated.total_deposited, 60);
        assert_eq!(updated.locked_collateral, 60);
    }

    /// #376: position with locked == total means available == 0 → any withdrawal rejected.
    #[test]
    fn test_withdraw_fully_locked_rejected() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 100,
            no_shares: 0,
            locked_collateral: 100,
            total_deposited: 100,
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

    /// #377: fee is deducted on top of the requested amount, user receives exact amount.
    #[test]
    fn test_withdraw_fee_deducted_on_top() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
        // No locked collateral, total=100 → available=100
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
            storage::set_fee_rate_bps(&env, 1_000); // 10%
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);
        let user_token = soroban_sdk::token::Client::new(&env, &token);
        // Withdraw 40: fee = 4, total_required = 44, available = 100 → ok
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());
        // User receives exactly 40
        assert_eq!(user_token.balance(&user), 40);
        // Position deducted by 44 (40 + 4 fee)
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().unwrap()
        });
        assert_eq!(updated.total_deposited, 56); // 100 - 44
    }

    /// #377: when amount + fee > available, reject with InsufficientCollateral.
    #[test]
    fn test_withdraw_fee_causes_insufficient_collateral() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &collateral_token);
        // locked=50, total=100 → available=50
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 50,
            total_deposited: 100,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 1_000); // 10%
        });
        env.mock_all_auths();
        // Withdraw 48: fee=4, total_required=52, available=50 → insufficient
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 48)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    #[test]
    fn test_withdraw_zero_fee_rate_no_deduction() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
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
            storage::set_fee_rate_bps(&env, 0);
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().unwrap()
        });
        assert_eq!(updated.total_deposited, 60); // 100 - 40, no fee
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
            market_id, user: user.clone(),
            yes_shares: 0, no_shares: 0,
            locked_collateral: 0, total_deposited: 0, is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 1000); // 10% fee
            storage::set_treasury(&env, &Some(treasury_id.clone()));
        });
        env.mock_all_auths();
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 1)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }
}
