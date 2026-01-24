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

    // Validation
    validation::validate_collateral_amount(amount)?;

    let market = storage::get_market(&env, market_id).ok_or(ContractError::MarketNotFound)?;

    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
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
    let mut position = storage::get_position(&env, market_id, &user).unwrap_or_else(|| Position {
        market_id,
        user: user.clone(),
        yes_shares: 0,
        no_shares: 0,
        locked_collateral: 0,
        is_settled: false,
    });

    // Add to locked_collateral (represents total deposited)
    // This is available for buying shares
    position.locked_collateral = position
        .locked_collateral
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    // Persist updated position
    storage::set_position(&env, market_id, &user, &position);

    // Emit event
    emit_collateral_deposited(&env, &user, market_id, amount, position.locked_collateral);

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
    fn test_deposit_validates_zero_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_market(&env, market_id, &market);
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
            storage::set_market(&env, market_id, &market);
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
            storage::set_market(&env, market_id, &market);
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
            storage::set_market(&env, market_id, &market);
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
            storage::set_market(&env, market_id, &market);
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
}
