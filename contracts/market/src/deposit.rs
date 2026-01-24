//! Deposit collateral implementation for Vatix prediction markets
//!
//! This module handles USDC deposits into prediction markets.
//! Users deposit collateral which can then be used to buy YES/NO shares.

use crate::error::ContractError;
use crate::events::emit_collateral_deposited;
use crate::storage::{extend_ttl, get_market, get_position, set_market, set_position};
use crate::types::{MarketStatus, Position};

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env, String};

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
    market_id: String,
    amount: i128,
) -> Result<(), ContractError> {
    // ====================================================================
    // STEP 1: AUTHORIZATION & INITIAL SETUP
    // ====================================================================

    // 1.1 Extend storage TTL before any operations
    // CRITICAL: Soroban data expires; must refresh TTL regularly
    extend_ttl(&env);

    // 1.2 Enforce user authorization
    // Proves user signed the transaction authorizing this call
    // Soroban's equivalent of Ethereum's msg.sender + signature verification
    user.require_auth();

    // ====================================================================
    // STEP 2: INPUT VALIDATION
    // ====================================================================

    // 2.1 Validate amount is positive
    if amount <= 0 {
        return Err(ContractError::InvalidQuantity);
    }

    // 2.2 Cap amount at 10^18 stroops (100 billion USDC) to prevent overflow
    const MAX_DEPOSIT_AMOUNT: i128 = 1_000_000_000_000_000_000i128;
    if amount > MAX_DEPOSIT_AMOUNT {
        return Err(ContractError::InvalidQuantity);
    }

    // 2.3 Fetch market from storage
    let mut market = get_market(&env, &market_id)
        .ok_or(ContractError::MarketNotFound)?;

    // 2.4 Validate market status - only Active markets accept deposits
    // Note: MarketStatus only has Active, Resolved, Canceled in current codebase
    match market.status {
        MarketStatus::Active => {
            // Market is valid for deposits
        }
        MarketStatus::Resolved | MarketStatus::Canceled => {
            return Err(ContractError::MarketNotActive);
        }
    }

    // ====================================================================
    // STEP 3: TOKEN TRANSFER (ATOMIC POINT)
    // ====================================================================
    // CRITICAL: If transfer fails, entire transaction reverts

    // 3.1 Get contract's own address (where we'll hold the collateral)
    let contract_address = env.current_contract_address();

    // 3.2 Initialize Soroban token client for the collateral token (USDC)
    let token_client = TokenClient::new(&env, &market.collateral_token);

    // 3.3 Perform the actual token transfer: user → contract
    // User has already called require_auth(), so TokenClient knows user authorized this
    token_client.transfer(&user, &contract_address, &amount);

    // ====================================================================
    // STEP 4: UPDATE STORAGE (NO EXTERNAL CALLS AFTER THIS)
    // ====================================================================
    // Safe to update storage because token transfer succeeded

    // 4.1 Fetch existing position or create new one
    let mut position = get_position(&env, &market_id, &user)
        .unwrap_or_else(|| Position {
            market_id: market_id.clone(),
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            is_settled: false,
        });

    // 4.2 Add amount to position's locked collateral
    // Using checked_add to detect overflow
    position.locked_collateral = position.locked_collateral
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    // 4.3 Persist updated position to storage
    set_position(&env, &market_id, &user, &position);

    // 4.4 Update market's total collateral tracking
    market.total_collateral = market.total_collateral
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    // 4.5 Persist updated market to storage
    set_market(&env, &market_id, &market);

    // ====================================================================
    // STEP 5: EMIT EVENTS
    // ====================================================================

    emit_collateral_deposited(
        &env,
        &user,
        &market_id,
        amount,
        position.locked_collateral,
    );

    // ====================================================================
    // SUCCESS
    // ====================================================================
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

    fn create_test_market(env: &Env, market_id: &String, collateral_token: &Address) -> Market {
        Market {
            id: market_id.clone(),
            question: String::from_str(env, "Will it rain tomorrow?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: collateral_token.clone(),
            total_collateral: 0,
        }
    }

    #[test]
    fn test_deposit_validates_zero_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "market-1");
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, &market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            set_market(&env, &market_id, &market);
        });

        // Mock auth
        env.mock_all_auths();

        // Test zero amount - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), 0)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_deposit_validates_negative_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "market-1");
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, &market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            set_market(&env, &market_id, &market);
        });

        // Mock auth
        env.mock_all_auths();

        // Test negative amount - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), -100)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_deposit_validates_market_not_found() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "nonexistent");
        let contract_id = env.register(crate::MarketContract, ());

        // Mock auth
        env.mock_all_auths();

        // Test nonexistent market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotFound));
    }

    #[test]
    fn test_deposit_validates_resolved_market() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "market-1");
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup resolved market
        let mut market = create_test_market(&env, &market_id, &collateral_token);
        market.status = MarketStatus::Resolved;
        market.result = Some(true);

        env.as_contract(&contract_id, || {
            set_market(&env, &market_id, &market);
        });

        // Mock auth
        env.mock_all_auths();

        // Test resolved market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotActive));
    }

    #[test]
    fn test_deposit_validates_canceled_market() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "market-1");
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup canceled market
        let mut market = create_test_market(&env, &market_id, &collateral_token);
        market.status = MarketStatus::Canceled;

        env.as_contract(&contract_id, || {
            set_market(&env, &market_id, &market);
        });

        // Mock auth
        env.mock_all_auths();

        // Test canceled market - should fail
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), 1000)
        });

        assert_eq!(result, Err(ContractError::MarketNotActive));
    }

    #[test]
    fn test_deposit_validates_excessive_amount() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = String::from_str(&env, "market-1");
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        // Setup market
        let market = create_test_market(&env, &market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            set_market(&env, &market_id, &market);
        });

        // Mock auth
        env.mock_all_auths();

        // Test excessive amount - should fail
        let excessive = i128::MAX;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id.clone(), excessive)
        });

        assert_eq!(result, Err(ContractError::InvalidQuantity));
    }
}
