use crate::error::ContractError;
use crate::storage;
use crate::types::{Market, MarketStatus, Position};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env};

/// Calculate payout for a position based on market outcome
///
/// # Arguments
/// * `position` - User's position
/// * `outcome` - Market outcome (true = YES won, false = NO won)
///
/// # Returns
/// Payout amount in stroops (1 USDC = 10^7 stroops)
pub fn calculate_payout(position: &Position, outcome: bool) -> i128 {
    if outcome {
        position.yes_shares
    } else {
        position.no_shares
    }
}

/// Check if a position is eligible for settlement
///
/// # Arguments
/// * `position` - Position to check
/// * `market` - Associated market
pub fn validate_settlement_eligibility(
    position: &Position,
    market: &Market,
) -> Result<(), ContractError> {
    if market.status != MarketStatus::Resolved {
        return Err(ContractError::MarketNotResolved);
    }

    if position.is_settled {
        return Err(ContractError::PositionAlreadySettled);
    }

    Ok(())
}

/// Validate that payout amount is non-negative
///
/// # Arguments
/// * `payout` - Payout amount to validate
///
/// # Returns
/// Ok if payout is valid, error otherwise
fn validate_payout(payout: i128) -> Result<(), ContractError> {
    if payout < 0 {
        return Err(ContractError::InvalidQuantity);
    }
    Ok(())
}

/// Execute settlement for a position and return payout
///
/// This function:
/// 1. Validates settlement eligibility
/// 2. Calculates payout
/// 3. Validates payout amount
/// 4. Marks position as settled
/// 5. Returns payout amount
pub fn execute_settlement(
    env: &Env,
    position: &mut Position,
    market: &Market,
) -> Result<i128, ContractError> {
    validate_settlement_eligibility(position, market)?;

    let outcome = market.result.ok_or(ContractError::MarketNotResolved)?;
    let payout = calculate_payout(position, outcome);

    validate_payout(payout)?;

    position.is_settled = true;

    // Emit event
    let settled_at = env.ledger().timestamp();
    crate::events::emit_position_settled(
        env,
        position.market_id,
        &position.user,
        payout,
        settled_at,
    );

    Ok(payout)
}

/// Settle a user's position in a resolved market and transfer their payout.
///
/// This is the full settlement entry point that completes the
/// deposit -> resolve -> settle -> receive-funds loop:
/// 1. Loads the market and the user's position
/// 2. Validates eligibility, calculates the payout, and marks the position
///    settled (via [`execute_settlement`], which also emits `PositionSettled`)
/// 3. Persists the updated position
/// 4. Transfers the payout in collateral (SAC) tokens from the contract to the
///    user
///
/// # Arguments
/// * `env` - Contract environment
/// * `user` - User settling their position (must authorize the call)
/// * `market_id` - Market identifier
///
/// # Returns
/// The payout amount transferred to the user, in stroops.
///
/// # Errors
/// - [`ContractError::MarketNotFound`] - the market does not exist
/// - [`ContractError::NoPositionFound`] - the user has no position in the market
/// - [`ContractError::MarketNotResolved`] - the market has not been resolved
/// - [`ContractError::PositionAlreadySettled`] - the position was already settled
///
/// # Events
/// Emits `PositionSettled` with the payout amount.
pub fn settle_position(env: &Env, user: &Address, market_id: u32) -> Result<i128, ContractError> {
    user.require_auth();

    let market = storage::get_market(env, market_id).ok_or(ContractError::MarketNotFound)?;
    let mut position =
        storage::get_position(env, market_id, user).ok_or(ContractError::NoPositionFound)?;

    // Validates eligibility (Resolved + not already settled), computes the
    // payout, marks the position settled, and emits the PositionSettled event.
    let payout = execute_settlement(env, &mut position, &market)?;

    // Persist the settled position before paying out.
    storage::set_position(env, market_id, user, &position);

    // Transfer the payout in collateral tokens from the contract to the user.
    if payout > 0 {
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(env, &market.collateral_token);
        token_client.transfer(&contract_address, user, &payout);
    }

    Ok(payout)
}

/// Calculate what a user would receive if they settled now
///
/// # Arguments
/// * `position` - User's position
/// * `market` - Market (may or may not be resolved)
pub fn calculate_potential_payout(position: &Position, market: &Market) -> Option<i128> {
    market
        .result
        .map(|outcome| calculate_payout(position, outcome))
}

/// Calculate statistics about settlements
///
/// # Returns
/// (winning_shares, losing_shares, total_payout)
pub fn calculate_market_settlement_stats(
    total_yes_shares: i128,
    total_no_shares: i128,
    outcome: bool,
) -> (i128, i128, i128) {
    if outcome {
        (total_yes_shares, total_no_shares, total_yes_shares)
    } else {
        (total_no_shares, total_yes_shares, total_no_shares)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, BytesN, Env, String,
    };

    fn create_test_market(env: &Env, status: MarketStatus, result: Option<bool>) -> Market {
        Market {
            id: 1,
            question: String::from_str(env, "Test?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status,
            result,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: Address::generate(env),
        }
    }

    fn create_test_position(env: &Env, yes: i128, no: i128, settled: bool) -> Position {
        Position {
            market_id: 1,
            user: Address::generate(env),
            yes_shares: yes,
            no_shares: no,
            locked_collateral: yes + no, // simplified
            total_deposited: yes + no,
            is_settled: settled,
        }
    }

    #[test]
    fn test_calculate_payout_yes_wins() {
        let env = Env::default();
        let pos = create_test_position(&env, 100, 30, false);
        assert_eq!(calculate_payout(&pos, true), 100);
    }

    #[test]
    fn test_calculate_payout_no_wins() {
        let env = Env::default();
        let pos = create_test_position(&env, 100, 30, false);
        assert_eq!(calculate_payout(&pos, false), 30);
    }

    #[test]
    fn test_calculate_payout_hedged_position() {
        let env = Env::default();
        let pos = create_test_position(&env, 50, 50, false);
        assert_eq!(calculate_payout(&pos, true), 50);
        assert_eq!(calculate_payout(&pos, false), 50);
    }

    #[test]
    fn test_calculate_payout_zero_shares() {
        let env = Env::default();
        let pos = create_test_position(&env, 0, 0, false);
        assert_eq!(calculate_payout(&pos, true), 0);
    }

    #[test]
    fn test_validate_settlement_not_resolved() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Active, None);
        let pos = create_test_position(&env, 100, 0, false);

        let result = validate_settlement_eligibility(&pos, &market);
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_validate_settlement_already_settled() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let pos = create_test_position(&env, 100, 0, true);

        let result = validate_settlement_eligibility(&pos, &market);
        assert_eq!(result, Err(ContractError::PositionAlreadySettled));
    }

    #[test]
    fn test_execute_settlement_marks_as_settled() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let mut pos = create_test_position(&env, 100, 0, false);

        let payout = env.as_contract(&contract_id, || {
            execute_settlement(&env, &mut pos, &market).unwrap()
        });
        assert_eq!(payout, 100);
        assert!(pos.is_settled);
    }

    #[test]
    fn test_execute_settlement_returns_correct_amount() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Resolved, Some(false));
        let mut pos = create_test_position(&env, 100, 30, false);

        let payout = env.as_contract(&contract_id, || {
            execute_settlement(&env, &mut pos, &market).unwrap()
        });
        assert_eq!(payout, 30);
    }

    #[test]
    fn test_execute_settlement_emits_event() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let mut pos = create_test_position(&env, 100, 0, false);

        env.as_contract(&contract_id, || {
            execute_settlement(&env, &mut pos, &market).unwrap();
        });

        let events = env.events().all();
        assert!(events.len() > 0);
    }

    #[test]
    fn test_potential_payout_unresolved_market() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Active, None);
        let pos = create_test_position(&env, 100, 0, false);

        assert_eq!(calculate_potential_payout(&pos, &market), None);
    }

    #[test]
    fn test_potential_payout_resolved_market() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Resolved, Some(true));
        let pos = create_test_position(&env, 100, 30, false);

        assert_eq!(calculate_potential_payout(&pos, &market), Some(100));
    }

    #[test]
    fn test_market_settlement_stats() {
        let (winning, losing, payout) = calculate_market_settlement_stats(1000, 500, true);
        assert_eq!(winning, 1000);
        assert_eq!(losing, 500);
        assert_eq!(payout, 1000);

        let (winning, losing, payout) = calculate_market_settlement_stats(1000, 500, false);
        assert_eq!(winning, 500);
        assert_eq!(losing, 1000);
        assert_eq!(payout, 500);
    }

    #[test]
    fn test_validate_payout_valid() {
        assert!(validate_payout(0).is_ok());
        assert!(validate_payout(100).is_ok());
        assert!(validate_payout(i128::MAX).is_ok());
    }

    #[test]
    fn test_validate_payout_invalid() {
        assert_eq!(validate_payout(-1), Err(ContractError::InvalidQuantity));
        assert_eq!(validate_payout(-100), Err(ContractError::InvalidQuantity));
    }

    /// End-to-end settlement through the contract client, asserting that the
    /// SAC token payout actually reaches the user:
    /// init -> create market -> deposit -> buy -> resolve -> settle.
    #[test]
    fn test_settle_position_transfers_payout_full_flow() {
        use crate::{MarketContract, MarketContractClient};
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

        const STROOPS_PER_USDC: i128 = 10_000_000;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        // Admin is required before a market can be created.
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
        });

        // Real SAC collateral token.
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let sac = StellarAssetClient::new(&env, &collateral_token);
        let token_client = TokenClient::new(&env, &collateral_token);

        // Oracle keypair used to sign the resolution of market id 1.
        let outcome = true;
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        // Create the market.
        let question = String::from_str(&env, "Will the payout land?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Deposit collateral.
        let user = Address::generate(&env);
        let deposit = 100 * STROOPS_PER_USDC;
        sac.mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        // Buy YES shares so the resolved position has a payout.
        let yes_shares = 100 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);

        // Resolve the market (YES wins) with a valid oracle signature.
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        // Before settling, the contract holds the deposit and the user holds nothing.
        assert_eq!(token_client.balance(&user), 0);
        assert_eq!(token_client.balance(&contract_id), deposit);

        // Settle: the payout equals the winning YES shares.
        let payout = client.settle_position(&user, &market_id);
        assert_eq!(payout, yes_shares);

        // The SAC tokens moved from the contract to the user.
        assert_eq!(token_client.balance(&user), payout);
        assert_eq!(token_client.balance(&contract_id), deposit - payout);

        // The position is now marked settled.
        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).expect("position should exist")
        });
        assert!(position.is_settled);

        // Settling a second time is rejected.
        let second = client.try_settle_position(&user, &market_id);
        assert!(second.is_err());
    }

    #[test]
    fn test_settle_position_rejects_unresolved_market() {
        use crate::{MarketContract, MarketContractClient};
        use soroban_sdk::token::StellarAssetClient;

        const STROOPS_PER_USDC: i128 = 10_000_000;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();

        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let question = String::from_str(&env, "Still active?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        let deposit = 50 * STROOPS_PER_USDC;
        StellarAssetClient::new(&env, &collateral_token).mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        // The market is still Active, so settlement must be rejected (#3).
        let result = client.try_settle_position(&user, &market_id);
        assert_eq!(result, Err(Ok(ContractError::MarketNotResolved)));
    }
}
