//! Deposit collateral implementation for Vatix prediction markets
//!
//! This module handles USDC deposits into prediction markets.
//! Users deposit collateral which can then be used to buy YES/NO shares.

use crate::error::ContractError;
use crate::events::emit_collateral_deposited;
use crate::storage;
use crate::types::{MarketStatus, Position};
use crate::validation;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env};

/// Deposit USDC collateral into a prediction market
///
/// # Detailed Flow
/// 1. **Authorization**: Verify user signed this transaction
/// 2. **Validation**: Check amount, market existence, market status
/// 3. **Token Transfer**: Move USDC from user to contract (ATOMIC POINT)
/// 4. **State Update**: Persist position and collateral data
/// 5. **Event Emission**: Publish CollateralDeposited event
///
/// # Arguments
/// * `env` - Soroban environment (provides ledger, storage, events access)
/// * `user` - User's Stellar address (must authorize this call)
/// * `market_id` - Market identifier (e.g., "market_usd_2025")
/// * `amount` - Amount in stroops (1 USDC = 10^7 stroops = 10,000,000)
///
/// # Return
/// `Result<(), ContractError>` - Ok(()) on success, error on failure
///
/// # Errors
/// - `MarketNotFound`: market_id doesn't exist in storage
/// - `MarketNotActive`: Market is resolved or cancelled
/// - `InvalidQuantity`: amount <= 0 or exceeds max
/// - `TokenTransferFailed`: USDC transfer failed (insufficient balance, etc.)
/// - `ArithmeticOverflow`: Collateral amount would exceed i128 max
///
/// # Events
/// Emits `CollateralDeposited` event with:
/// - user: User's address
/// - market_id: Target market
/// - amount: Amount deposited in stroops
/// - new_total: User's total collateral in this market after deposit
pub fn deposit_collateral(
    env: Env,
    user: Address,
    market_id: u32,
    amount: i128,
) -> Result<(), ContractError> {
    // Authorization
    user.require_auth();

    // Validation: reject zero or negative deposits explicitly
    if amount <= 0 {
        return Err(ContractError::InvalidQuantity);
    }
    validation::validate_collateral_amount(amount)?;

    let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;

    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
    }

    if market.closed_to_deposits {
        return Err(ContractError::MarketClosedToDeposits);
    }

    if env.ledger().timestamp() > market.end_time {
        return Err(ContractError::MarketExpired);
    }

    // Transfer USDC from user to contract
    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);
    token_client.transfer(&user, &contract_address, &amount);

    // TODO: Refactor collateral management
    // Current design requires separate deposits per market. Users cannot use
    // Market A collateral for Market B trades. refactor will introduce:
    // - Global user balance (deposit once, trade anywhere)
    // - Better capital efficiency
    //
    // # Current Flow
    // 1. User deposits USDC into specific market
    // 2. Collateral locked to this market only
    // 3. User must deposit separately for each market they want to trade
    let mut position = storage::get_position(&env, market_id, &user)?.unwrap_or_else(|| Position {
        market_id,
        user: user.clone(),
        yes_shares: 0,
        no_shares: 0,
        locked_collateral: 0,
        total_deposited: 0,
        is_settled: false,
    });

    // Add to total_deposited (total collateral user has in this market).
    //
    // `locked_collateral` is NOT touched here. It represents collateral
    // required to back the user's current YES/NO shares and is the single
    // source of truth maintained exclusively by `positions::update_position`
    // (see `calculate_locked_collateral`). A deposit with no shares held
    // must leave `locked_collateral` at 0, otherwise `withdraw` (which now
    // trusts this field directly) would see deposited-but-unused collateral
    // as locked.
    position.total_deposited = position
        .total_deposited
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    // Persist updated position
    storage::set_position(&env, market_id, &user, &position)?;

    // Record deposit timestamp for cooldown enforcement on withdrawals (issue #413).
    storage::set_last_deposit_time(&env, market_id, &user, env.ledger().timestamp());

    // TODO(#issue): consider batching deposit events for gas efficiency
    // Emit event
    emit_collateral_deposited(&env, &user, market_id, amount, position.total_deposited);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Market;
    use soroban_sdk::token::StellarAssetClient;
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
    fn test_deposit_validates_zero_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Mock auth
        env.mock_all_auths();

        // Test zero amount - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 0)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_deposit_validates_negative_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Mock auth
        env.mock_all_auths();

        // Test negative amount - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, -100)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_deposit_validates_market_not_found() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 999; // Nonexistent market ID
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
        });

        // Mock auth
        env.mock_all_auths();

        // Test nonexistent market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotFound));
    }

    #[test]
    fn test_deposit_validates_resolved_market() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup resolved market
        let mut market = create_test_market(&env, market_id, &collateral_token);
        market.status = MarketStatus::Resolved;
        market.result = Some(true);

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Mock auth
        env.mock_all_auths();

        // Test resolved market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotActive));
    }

    #[test]
    fn test_deposit_validates_canceled_market() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup canceled market
        let mut market = create_test_market(&env, market_id, &collateral_token);
        market.status = MarketStatus::Canceled;

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Mock auth
        env.mock_all_auths();

        // Test canceled market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotActive));
    }

    #[test]
    fn test_deposit_validates_excessive_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Mock auth
        env.mock_all_auths();

        // Test excessive amount - should fail
        let excessive = i128::MAX;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, excessive)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_deposit_updates_position_collateral() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        env.mock_all_auths();
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &10_000);

        let deposit_amount = 5_000i128;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, deposit_amount)
        });
        assert!(result.is_ok());

        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(position.total_deposited, deposit_amount);
        // locked_collateral is share-based and is untouched by deposit; see
        // `test_deposit_with_zero_shares_keeps_locked_collateral_zero` below.
        assert_eq!(position.locked_collateral, 0);
    }

    /// Regression test for #262: a deposit with zero shares held must never
    /// show any collateral as locked. Before the fix, `deposit_collateral`
    /// incremented `locked_collateral` by the deposit amount directly,
    /// making freshly deposited (and entirely unused) collateral look
    /// "locked" even though the user had not bought any YES/NO shares.
    #[test]
    fn test_deposit_with_zero_shares_keeps_locked_collateral_zero() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        env.mock_all_auths();
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &10_000);

        let deposit_amount = 5_000i128;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, deposit_amount)
        });
        assert!(result.is_ok());

        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(position.yes_shares, 0);
        assert_eq!(position.no_shares, 0);
        assert_eq!(position.total_deposited, deposit_amount);
        assert_eq!(position.locked_collateral, 0);

        // A second deposit must keep locked_collateral at 0 while
        // total_deposited keeps growing.
        let second_deposit = 1_000i128;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, second_deposit)
        });
        assert!(result.is_ok());

        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(position.total_deposited, deposit_amount + second_deposit);
        assert_eq!(position.locked_collateral, 0);
    }

    // --- #375: collateral_deposited event contains correct amount and new_total ---

    #[test]
    fn test_deposit_event_contains_amount_and_new_total() {
        use soroban_sdk::{
            testutils::{Events as _, Address as _},
            IntoVal, Map, Symbol, TryIntoVal, Val,
        };

        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        env.mock_all_auths();
        let sac = soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token);
        sac.mint(&user, &20_000);

        // First deposit
        let first = 7_000i128;
        env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, first).unwrap();
        });

        let events = env.events().all();
        let last = events.last().unwrap();

        // Topic 0 = event name symbol
        let topic0: soroban_sdk::Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, soroban_sdk::Symbol::new(&env, "collateral_deposited"));

        // Topic 1 = user
        let topic1: Address = last.1.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, user);

        // Topic 2 = market_id
        let topic2: u32 = last.1.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        // Data: amount and new_total
        let data: Map<Symbol, Val> = last.2.clone().try_into_val(&env).unwrap();
        let amount_val: i128 = data.get(Symbol::new(&env, "amount")).unwrap().into_val(&env);
        let new_total_val: i128 = data.get(Symbol::new(&env, "new_total")).unwrap().into_val(&env);
        assert_eq!(amount_val, first);
        assert_eq!(new_total_val, first); // first deposit, new_total == amount

        // Second deposit: new_total must reflect the running sum
        let second = 3_000i128;
        env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, second).unwrap();
        });

        let events2 = env.events().all();
        let last2 = events2.last().unwrap();
        let data2: Map<Symbol, Val> = last2.2.clone().try_into_val(&env).unwrap();
        let amount2: i128 = data2.get(Symbol::new(&env, "amount")).unwrap().into_val(&env);
        let new_total2: i128 = data2.get(Symbol::new(&env, "new_total")).unwrap().into_val(&env);
        assert_eq!(amount2, second);
        assert_eq!(new_total2, first + second);
    }

    // --- #344: market expiry enforcement on deposit ---

    #[test]
    fn test_deposit_rejected_after_market_expiry() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        // Create market with end_time in the past
        let mut market = create_test_market(&env, market_id, &collateral_token);
        market.end_time = 0; // expired
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        env.mock_all_auths();
        let sac = soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token);
        sac.mint(&user, &20_000);

        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 5_000)
        });
        assert_eq!(result, Err(ContractError::MarketExpired));
    }

    // --- #374: total_deposited accumulates correctly across multiple deposits ---

    #[test]
    fn test_total_deposited_accumulates_across_multiple_deposits() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        env.mock_all_auths();
        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token).mint(&user, &100_000);

        let deposits = [10_000i128, 5_000, 15_000, 20_000];
        let mut running = 0i128;
        for amount in deposits {
            env.as_contract(&contract_id, || {
                deposit_collateral(env.clone(), user.clone(), market_id, amount).unwrap();
            });
            running += amount;

            let position = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
            });
            assert_eq!(position.total_deposited, running, "after deposit of {amount}");
        }

        // Final total must equal sum of all deposits
        assert_eq!(running, deposits.iter().sum::<i128>());
    }
}
