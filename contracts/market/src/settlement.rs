use crate::error::ContractError;
use crate::storage;
use crate::types::{AdapterType, Market, MarketStatus, Position};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env, Vec};

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

    // Support a "no-winner" refund path: when a market is marked as
    // `Resolved` but `result` is `None` we treat the settlement as a full
    // refund of the user's deposited collateral. This allows resolution
    // flows (or external governance) to indicate that no outcome could be
    // determined and users should be made whole.
    let payout = match market.result {
        Some(outcome) => calculate_payout(position, outcome),
        None => position.total_deposited,
    };

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

    let market = storage::get_market(env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    let mut position =
        storage::get_position(env, market_id, user)?.ok_or(ContractError::NoPositionFound)?;

    // Validates eligibility (Resolved + not already settled), computes the
    // payout, marks the position settled, and emits the PositionSettled event.
    let payout = execute_settlement(env, &mut position, &market)?;

    // Persist the settled position before paying out.
    storage::set_position(env, market_id, user, &position)?;

    // Transfer the payout in collateral tokens from the contract to the user.
    if payout > 0 {
        let contract_address = env.current_contract_address();
        let token_client = TokenClient::new(env, &market.collateral_token);
        token_client.transfer(&contract_address, user, &payout);
    }

    Ok(payout)
}

/// Settle multiple users' positions in a single call for a resolved market.
///
/// Iterates over `users`, calling [`settle_position`] for each. Positions that
/// are already settled, not found, or encounter any other per-user error are
/// skipped — the batch continues and the total payout across all successfully
/// settled positions is returned.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier (must be resolved)
/// * `users` - List of user addresses to settle
///
/// # Returns
/// Total payout transferred across all settled positions, in stroops.
///
/// # Errors
/// - [`ContractError::MarketNotFound`] – the market does not exist
/// - [`ContractError::MarketNotResolved`] – the market is not resolved; in this
///   case no individual settlements are attempted
pub fn batch_settle_positions(
    env: &Env,
    market_id: u32,
    users: Vec<Address>,
) -> Result<i128, ContractError> {
    // Validate the market once before iterating users.
    let market = storage::get_market(env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    if market.status != MarketStatus::Resolved {
        return Err(ContractError::MarketNotResolved);
    }

    let mut total_payout: i128 = 0;

    for user in users.iter() {
        let Ok(Some(mut position)) = storage::get_position(env, market_id, &user) else {
            continue;
        };

        // Skip already-settled positions and any unexpected state.
        let Ok(payout) = execute_settlement(env, &mut position, &market) else {
            continue;
        };

        // Persist the settled flag; skip if storage fails.
        if storage::set_position(env, market_id, &user, &position).is_err() {
            continue;
        }

        if payout > 0 {
            let contract_address = env.current_contract_address();
            let token_client = TokenClient::new(env, &market.collateral_token);
            token_client.transfer(&contract_address, &user, &payout);
        }

        total_payout = total_payout.saturating_add(payout);
    }

    Ok(total_payout)
}

/// Calculate what a user would receive if they settled now
///
/// # Arguments
/// * `position` - User's position
/// * `market` - Market (may or may not be resolved)
pub fn calculate_potential_payout(position: &Position, market: &Market) -> Option<i128> {
    // If the market is resolved but has no winning outcome (result == None)
    // then the potential payout is the full deposited collateral (refund).
    if market.status == MarketStatus::Resolved {
        match market.result {
            Some(outcome) => Some(calculate_payout(position, outcome)),
            None => Some(position.total_deposited),
        }
    } else {
        None
    }
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
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
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

    #[test]
    fn test_execute_settlement_no_winner_refunds_deposited() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        // Market is resolved but has no winning outcome (None) -> refund path
        let market = create_test_market(&env, MarketStatus::Resolved, None);
        let mut pos = create_test_position(&env, 100, 30, false);

        let payout = env.as_contract(&contract_id, || {
            execute_settlement(&env, &mut pos, &market).unwrap()
        });

        // Full deposited amount should be returned
        assert_eq!(payout, pos.total_deposited);
        assert!(pos.is_settled);
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
            storage::set_version(&env);
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
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
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
            storage::set_version(&env);
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

    // --- #372: batch_settle_positions tests ---

    /// Helper: full setup returning env, contract_id, client, market_id, and a
    /// collateral token client — the market is resolved YES.
    fn setup_resolved_market() -> (
        soroban_sdk::Env,
        soroban_sdk::Address, // contract_id
        u32,                  // market_id
        soroban_sdk::Address, // collateral_token
    ) {
        use crate::MarketContract;
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        use soroban_sdk::{String, token::StellarAssetClient};

        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let sac = StellarAssetClient::new(&env, &collateral_token);

        let outcome = true;
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let question = String::from_str(&env, "Batch settle test?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id =
            client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);

        // Mint and deposit for two users with YES shares
        for _ in 0..2u8 {
            let u = Address::generate(&env);
            sac.mint(&u, &(100_000_000i128));
            client.deposit_collateral(&u, &market_id, &(100_000_000i128));
            client.update_position(&u, &market_id, &(100_000_000i128), &0i128, &5_000i128);
        }

        // Resolve YES
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        (env, contract_id, market_id, collateral_token)
    }

    #[test]
    fn test_batch_settle_rejects_unresolved_market() {
        use crate::MarketContract;
        use soroban_sdk::String;

        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let question = String::from_str(&env, "Still active?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id =
            client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);

        let users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_batch_settle_returns_market_not_found_for_missing_market() {
        use crate::MarketContract;
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(MarketContract, ());
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
        });

        let users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, 999, users)
        });
        assert_eq!(result, Err(ContractError::MarketNotFound));
    }

    #[test]
    fn test_batch_settle_skips_missing_positions() {
        let (env, contract_id, market_id, _) = setup_resolved_market();
        let ghost = Address::generate(&env);
        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(ghost);

        let total = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        // Ghost has no position — batch returns 0, not an error.
        assert_eq!(total, Ok(0));
    }

    #[test]
    fn test_batch_settle_settles_multiple_users() {
        use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
        use soroban_sdk::String;
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        const DEPOSIT: i128 = 100_000_000;
        const SHARES: i128 = 100_000_000;
        const STROOPS_PER_USDC: i128 = 10_000_000;

        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::MarketContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let sac = StellarAssetClient::new(&env, &collateral_token);
        let token_client = TokenClient::new(&env, &collateral_token);

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let question = String::from_str(&env, "Batch settle multi user?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin, &question, &end_time, &oracle_pubkey, &collateral_token,
        );

        // Create two users, both buy YES shares.
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        for u in [&user1, &user2] {
            sac.mint(u, &DEPOSIT);
            client.deposit_collateral(u, &market_id, &DEPOSIT);
            client.update_position(u, &market_id, &SHARES, &0i128, &5_000i128);
        }

        // Resolve YES.
        let outcome = true;
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        // Batch settle both users.
        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(user1.clone());
        users.push_back(user2.clone());

        let total_payout = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        })
        .expect("batch settle should succeed");

        // Both users should receive SHARES each.
        assert_eq!(total_payout, SHARES * 2);
        assert_eq!(token_client.balance(&user1), SHARES);
        assert_eq!(token_client.balance(&user2), SHARES);

        // Both positions are now marked settled.
        for u in [&user1, &user2] {
            let pos = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, u)
                    .unwrap()
                    .expect("position should exist")
            });
            assert!(pos.is_settled);
        }
    }

    #[test]
    fn test_batch_settle_skips_already_settled() {
        use soroban_sdk::String;
        use soroban_sdk::token::StellarAssetClient;
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        const DEPOSIT: i128 = 50_000_000;
        const SHARES: i128 = 50_000_000;

        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let contract_id = env.register(crate::MarketContract, ());
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        StellarAssetClient::new(&env, &collateral_token).mint(&Address::generate(&env), &0);

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &String::from_str(&env, "Skip settled?"),
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        StellarAssetClient::new(&env, &collateral_token).mint(&user, &DEPOSIT);
        client.deposit_collateral(&user, &market_id, &DEPOSIT);
        client.update_position(&user, &market_id, &SHARES, &0i128, &5_000i128);

        let outcome = true;
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        client.resolve_market(&String::from_str(&env, "1"), &outcome, &signature);

        // Settle once through the normal path.
        client.settle_position(&user, &market_id);

        // Batch settling the same user a second time must produce 0 payout,
        // not an error.
        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(user.clone());
        let second = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        assert_eq!(second, Ok(0));
    }
}
