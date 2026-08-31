use crate::error::ContractError;
use crate::storage;
use crate::types::{AdapterType, Market, MarketStatus, Position};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env, Vec};
use vatix_outcome_token_contract::{types::TokenKind, OutcomeTokenContractClient};

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

/// Core settlement calculation shared by both the single-position
/// (`settle_position`) and batch (`batch_settle_positions`) paths.
///
/// Validates eligibility, computes the payout, and marks the position
/// settled — but does not emit any events. This lets callers choose
/// per-position event emission (`execute_settlement`, used by the
/// single-user path) or a single aggregated event for a whole batch
/// (`batch_settle_positions`, Issue #499) without duplicating the
/// settlement math.
fn compute_settlement(
    env: &Env,
    position: &mut Position,
    market: &Market,
) -> Result<i128, ContractError> {
    validate_settlement_eligibility(position, market)?;

    // Dual-ledger reconciliation guard (see `crate::reconciliation`): refuse
    // to settle a position whose Position shares and OutcomeToken balances
    // have diverged. An admin must repair via `reconcile_position_tokens`
    // first — there is no silent re-sync on this path.
    crate::reconciliation::assert_position_token_parity(env, position.market_id, &position.user)?;

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

    Ok(payout)
}

/// Burn a settled position's outcome tokens (both YES and NO sides), if an
/// outcome-token contract is wired up via [`crate::MarketContract::set_outcome_token_contract`].
///
/// Retires the position's tokens now that they have been redeemed for the
/// collateral payout, so a settled position's outcome tokens can never be
/// transferred or redeemed a second time. Shared by every settle path
/// (`settle_position`, `batch_settle_positions`, `settle_positions_page`) so
/// full-exit burning behaves identically regardless of which entrypoint a
/// caller uses.
fn burn_settled_outcome_tokens(env: &Env, market_id: u32, user: &Address, position: &Position) {
    if let Some(outcome_token_address) = storage::get_outcome_token_contract(env) {
        let token_client = OutcomeTokenContractClient::new(env, &outcome_token_address);
        if position.yes_shares > 0 {
            token_client.burn(&market_id, user, &TokenKind::Yes, &position.yes_shares);
        }
        if position.no_shares > 0 {
            token_client.burn(&market_id, user, &TokenKind::No, &position.no_shares);
        }
    }
}

/// Execute settlement for a position and return payout
///
/// This function:
/// 1. Validates settlement eligibility
/// 2. Calculates payout
/// 3. Validates payout amount
/// 4. Marks position as settled
/// 5. Emits `PositionUpdated` and `PositionSettled` for this single position
/// 6. Returns payout amount
pub fn execute_settlement(
    env: &Env,
    position: &mut Position,
    market: &Market,
) -> Result<i128, ContractError> {
    let payout = compute_settlement(env, position, market)?;

    // Emit PositionUpdated so indexers observe the share balance zeroing out
    // on settlement (yes_shares and no_shares are consumed; locked_collateral
    // drops to 0 because the position is fully settled).
    crate::events::emit_position_updated(
        env,
        position.market_id,
        &position.user,
        position.yes_shares,
        position.no_shares,
        0,
    );

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
    // Emergency mode: settlement is blocked only in GlobalFreeze;
    // allowed in Normal, TradingHalted, and SettleOnly.
    crate::validation::require_emergency_mode_allows(
        env,
        &[
            crate::types::EmergencyMode::Normal,
            crate::types::EmergencyMode::TradingHalted,
            crate::types::EmergencyMode::SettleOnly,
        ],
    )?;



    let market = storage::get_market(env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    let mut position =
        storage::get_position(env, market_id, user)?.ok_or(ContractError::NoPositionFound)?;

    // Validates eligibility (Resolved + not already settled), computes the
    // payout, marks the position settled, and emits the PositionSettled event.
    let payout = execute_settlement(env, &mut position, &market)?;

     storage::set_position(env, market_id, user, &position)?;

    burn_settled_outcome_tokens(env, market_id, user, &position);

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
/// Positions that are already settled, not found, or encounter any other
/// per-user error are skipped — the batch continues and the total payout
/// across all successfully settled positions is returned.
///
/// Unlike calling [`settle_position`] N times (which emits `PositionUpdated`
/// + `PositionSettled` per user, i.e. 2N events), this settles each position
/// via the event-free [`compute_settlement`] core and emits exactly one
/// aggregated `PositionsBatchSettled` event covering every user actually
/// settled in this call (Issue #499) — cutting per-position event-emission
/// overhead for bulk settlement.
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
    // Emergency mode: settlement is blocked only in GlobalFreeze
    crate::validation::require_emergency_mode_allows(
        env,
        &[
            crate::types::EmergencyMode::Normal,
            crate::types::EmergencyMode::TradingHalted,
            crate::types::EmergencyMode::SettleOnly,
        ],
    )?;

    // Guard: reject empty batches immediately to surface caller bugs early
    // rather than silently returning 0 with no indication anything was wrong.
    if users.is_empty() {
        return Err(ContractError::BatchTooLarge);
    }

    // Guard: cap batch size to prevent gas-griefing. Callers with more users
    // should use the paginated `settle_positions_page` endpoint instead.
    if users.len() > crate::MAX_BATCH_SETTLE_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    // Validate the market once before iterating users.
    let market = storage::get_market(env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    if market.status != MarketStatus::Resolved {
        return Err(ContractError::MarketNotResolved);
    }

    let mut total_payout: i128 = 0;
    let mut settled_users: Vec<Address> = Vec::new(env);
    let mut settled_payouts: Vec<i128> = Vec::new(env);

    for user in users.iter() {
        let Ok(Some(mut position)) = storage::get_position(env, market_id, &user) else {
            continue;
        };

        // Skip already-settled positions and any unexpected state. No event
        // is emitted per-user here; one aggregated event covers the batch.
        let Ok(payout) = compute_settlement(env, &mut position, &market) else {
            continue;
        };

        burn_settled_outcome_tokens(env, market_id, &user, &position);

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
        settled_users.push_back(user);
        settled_payouts.push_back(payout);
    }

    if !settled_users.is_empty() {
        let settled_at = env.ledger().timestamp();
        crate::events::emit_positions_batch_settled(
            env,
            market_id,
            &settled_users,
            &settled_payouts,
            settled_at,
        );
    }

    Ok(total_payout)
}

/// Settle a bounded page of a market's tracked participants (Issue #495).
///
/// Markets with more positions than fit comfortably in one transaction's
/// resource budget can be fully settled by repeatedly calling this with
/// `start_index` advanced to the returned `next_index`, until the returned
/// `is_complete` flag is `true`. Unlike [`batch_settle_positions`], the
/// caller does not need to know or supply the list of participant
/// addresses — it is read from the on-chain participant registry that
/// [`crate::MarketContract::update_position`] maintains.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier (must be resolved)
/// * `start_index` - Index into the market's participant list to resume from
/// * `limit` - Maximum number of participants to process in this call
///
/// # Returns
/// `(total_payout_this_page, next_index, is_complete)` — `is_complete` is
/// `true` once `next_index` has reached the end of the participant list.
///
/// # Errors
/// - [`ContractError::MarketNotFound`] – the market does not exist
/// - [`ContractError::MarketNotResolved`] – the market is not resolved
pub fn settle_positions_page(
    env: &Env,
    market_id: u32,
    start_index: u32,
    limit: u32,
) -> Result<(i128, u32, bool), ContractError> {
    // Emergency mode: settlement is blocked only in GlobalFreeze
    crate::validation::require_emergency_mode_allows(
        env,
        &[
            crate::types::EmergencyMode::Normal,
            crate::types::EmergencyMode::TradingHalted,
            crate::types::EmergencyMode::SettleOnly,
        ],
    )?;

    let market = storage::get_market(env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    if market.status != MarketStatus::Resolved {
        return Err(ContractError::MarketNotResolved);
    }

    let participants = storage::get_market_participants(env, market_id);
    let total = participants.len();

    if start_index >= total {
        return Ok((0, total, true));
    }

    let end_index = start_index.saturating_add(limit).min(total);

    let mut total_payout: i128 = 0;
    let mut settled_users: Vec<Address> = Vec::new(env);
    let mut settled_payouts: Vec<i128> = Vec::new(env);

    for i in start_index..end_index {
        let user = participants.get(i).expect("index within bounds");

        let Ok(Some(mut position)) = storage::get_position(env, market_id, &user) else {
            continue;
        };

        let Ok(payout) = compute_settlement(env, &mut position, &market) else {
            continue;
        };

        burn_settled_outcome_tokens(env, market_id, &user, &position);

        if storage::set_position(env, market_id, &user, &position).is_err() {
            continue;
        }

        if payout > 0 {
            let contract_address = env.current_contract_address();
            let token_client = TokenClient::new(env, &market.collateral_token);
            token_client.transfer(&contract_address, &user, &payout);
        }

        total_payout = total_payout.saturating_add(payout);
        settled_users.push_back(user);
        settled_payouts.push_back(payout);
    }

    if !settled_users.is_empty() {
        let settled_at = env.ledger().timestamp();
        crate::events::emit_positions_batch_settled(
            env,
            market_id,
            &settled_users,
            &settled_payouts,
            settled_at,
        );
    }

    Ok((total_payout, end_index, end_index >= total))
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
            outcome_count: 2,
            closed_to_deposits: false,
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

    #[test]
    fn test_settle_position_refunds_collateral_for_no_winner() {
        use crate::{MarketContract, MarketContractClient};
        use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

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
        let sac = StellarAssetClient::new(&env, &collateral_token);
        let token_client = TokenClient::new(&env, &collateral_token);

        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let question = String::from_str(&env, "No winner refund path");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        let deposit = 100 * STROOPS_PER_USDC;
        sac.mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        client.update_position(&user, &market_id, &deposit, &0i128, &5_000i128);

        env.as_contract(&contract_id, || {
            let mut market = storage::get_market(&env, market_id).unwrap().unwrap();
            market.status = MarketStatus::Resolved;
            market.result = None;
            storage::set_market(&env, market_id, &market).unwrap();
        });

        let payout = client.settle_position(&user, &market_id);
        assert_eq!(payout, deposit);
        assert_eq!(token_client.balance(&user), deposit);
        assert_eq!(token_client.balance(&contract_id), 0);
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

    /// #settle-idempotency: a second `settle_position` call for the same user
    /// must never transfer funds again. This asserts the *effect* (token
    /// balances and stored position are byte-for-byte unchanged), not just
    /// that the call errors — proving the second call is a true no-op/error
    /// rather than an error that still leaked a partial payout.
    #[test]
    fn test_second_settle_position_cannot_double_pay() {
        use crate::{MarketContract, MarketContractClient};
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

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
        let sac = StellarAssetClient::new(&env, &collateral_token);
        let token_client = TokenClient::new(&env, &collateral_token);

        let outcome = true;
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let question = String::from_str(&env, "Can settle drain funds twice?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        let deposit = 100 * STROOPS_PER_USDC;
        sac.mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        let yes_shares = 100 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes_shares, &0i128, &5_000i128);

        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        client.resolve_market(&String::from_str(&env, "1"), &outcome, &signature);

        // First settle succeeds and pays out exactly once.
        let first_payout = client.settle_position(&user, &market_id);
        assert_eq!(first_payout, yes_shares);

        let user_balance_after_first = token_client.balance(&user);
        let contract_balance_after_first = token_client.balance(&contract_id);
        let position_after_first = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert!(position_after_first.is_settled);

        // Attempt to settle the same position multiple times more: every
        // attempt must be rejected with PositionAlreadySettled and must not
        // move any additional funds.
        for _ in 0..3 {
            let repeat = client.try_settle_position(&user, &market_id);
            assert_eq!(repeat, Err(Ok(ContractError::PositionAlreadySettled)));

            assert_eq!(
                token_client.balance(&user),
                user_balance_after_first,
                "second settle must not pay the user again"
            );
            assert_eq!(
                token_client.balance(&contract_id),
                contract_balance_after_first,
                "second settle must not drain the contract again"
            );
        }

        // Stored position is unchanged by the rejected repeat attempts.
        let position_after_repeats = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().expect("position should exist")
        });
        assert_eq!(position_after_repeats, position_after_first);
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

        // Pass a non-empty list so the market-status guard (not the empty-batch guard)
        // is the first thing that fires.
        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(Address::generate(&env));
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

        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(Address::generate(&env));
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

    /// #578: `batch_settle_positions` and `settle_positions_page` must burn a
    /// settled position's outcome tokens when an outcome-token contract is
    /// wired, exactly like the single-user `settle_position` path already
    /// does — otherwise a fully-exited position's YES/NO balances would be
    /// left outstanding after settlement.
    #[test]
    fn test_batch_settle_burns_outcome_tokens_when_wired() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::String;
        use vatix_outcome_token_contract::{OutcomeTokenContract, OutcomeTokenContractClient};

        const DEPOSIT: i128 = 100_000_000;
        const SHARES: i128 = 100_000_000;

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

        let ot_contract_id = env.register(OutcomeTokenContract, ());
        let ot_client = OutcomeTokenContractClient::new(&env, &ot_contract_id);
        ot_client.initialize(
            &admin,
            &contract_id,
            &String::from_str(&env, "Vatix Outcome Token"),
            &String::from_str(&env, "VOT"),
        );

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let question = String::from_str(&env, "Batch settle burns outcome tokens?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin, &question, &end_time, &oracle_pubkey, &collateral_token,
        );
        client.set_outcome_token_contract(&admin, &ot_contract_id);

        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        for u in [&user1, &user2] {
            sac.mint(u, &DEPOSIT);
            client.deposit_collateral(u, &market_id, &DEPOSIT);
            client.update_position(u, &market_id, &SHARES, &0i128, &5_000i128);
        }

        // Outcome tokens were minted alongside the position updates above.
        assert_eq!(ot_client.total_supply(&market_id, &TokenKind::Yes), SHARES * 2);
        assert_eq!(ot_client.balance(&market_id, &user1, &TokenKind::Yes), SHARES);
        assert_eq!(ot_client.balance(&market_id, &user2, &TokenKind::Yes), SHARES);

        let outcome = true;
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        let mut users: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
        users.push_back(user1.clone());
        users.push_back(user2.clone());
        env.as_contract(&contract_id, || batch_settle_positions(&env, market_id, users))
            .expect("batch settle should succeed");

        // Full exit via batch settlement must burn every settled user's
        // outcome tokens, driving total supply back to zero.
        assert_eq!(ot_client.total_supply(&market_id, &TokenKind::Yes), 0);
        assert_eq!(ot_client.balance(&market_id, &user1, &TokenKind::Yes), 0);
        assert_eq!(ot_client.balance(&market_id, &user2, &TokenKind::Yes), 0);
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

    // --- Settlement guard hardening: reject non-Resolved markets ---
    //
    // `settle_position`, `batch_settle_positions`, and `settle_positions_page`
    // must all refuse to pay out unless MarketStatus::Resolved. Beyond the
    // existing Active-market coverage above, these cover the Canceled case
    // explicitly (there is no separate "Closed" status in this protocol —
    // Active and Canceled are the only non-Resolved states) plus the
    // previously-untested paginated settlement path.

    #[test]
    fn test_validate_settlement_rejects_canceled_market() {
        let env = Env::default();
        let market = create_test_market(&env, MarketStatus::Canceled, None);
        let pos = create_test_position(&env, 100, 0, false);

        let result = validate_settlement_eligibility(&pos, &market);
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_batch_settle_rejects_canceled_market() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Canceled, None);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market.id, &market).unwrap();
        });

        let mut users: Vec<Address> = Vec::new(&env);
        users.push_back(Address::generate(&env));
        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market.id, users)
        });
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_settle_positions_page_rejects_active_market() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Active, None);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market.id, &market).unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            settle_positions_page(&env, market.id, 0, 10)
        });
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_settle_positions_page_rejects_canceled_market() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, MarketStatus::Canceled, None);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market.id, &market).unwrap();
        });

        let result = env.as_contract(&contract_id, || {
            settle_positions_page(&env, market.id, 0, 10)
        });
        assert_eq!(result, Err(ContractError::MarketNotResolved));
    }

    #[test]
    fn test_settle_positions_page_pays_out_resolved_market() {
        let (env, contract_id, market_id, collateral_token) = setup_resolved_market();

        // Fund the contract so the page-settle payout transfer succeeds.
        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
            .mint(&contract_id, &(1_000_000_000i128));

        let (total_payout, next_index, is_complete) = env.as_contract(&contract_id, || {
            settle_positions_page(&env, market_id, 0, 10)
        })
        .expect("resolved market should settle successfully");

        assert!(total_payout > 0, "resolved market page-settle must pay out");
        assert!(is_complete);
        assert_eq!(next_index, 2); // two participants were set up by setup_resolved_market
    }

    /// #578: `settle_positions_page` must also burn outcome tokens on full
    /// exit, matching `settle_position` and `batch_settle_positions`.
    #[test]
    fn test_settle_positions_page_burns_outcome_tokens_when_wired() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::String;
        use vatix_outcome_token_contract::{OutcomeTokenContract, OutcomeTokenContractClient};

        const DEPOSIT: i128 = 100_000_000;
        const SHARES: i128 = 100_000_000;

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

        let ot_contract_id = env.register(OutcomeTokenContract, ());
        let ot_client = OutcomeTokenContractClient::new(&env, &ot_contract_id);
        ot_client.initialize(
            &admin,
            &contract_id,
            &String::from_str(&env, "Vatix Outcome Token"),
            &String::from_str(&env, "VOT"),
        );

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let client = crate::MarketContractClient::new(&env, &contract_id);
        let question = String::from_str(&env, "Page settle burns outcome tokens?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin, &question, &end_time, &oracle_pubkey, &collateral_token,
        );
        client.set_outcome_token_contract(&admin, &ot_contract_id);

        let user = Address::generate(&env);
        sac.mint(&user, &DEPOSIT);
        client.deposit_collateral(&user, &market_id, &DEPOSIT);
        client.update_position(&user, &market_id, &SHARES, &0i128, &5_000i128);
        assert_eq!(ot_client.balance(&market_id, &user, &TokenKind::Yes), SHARES);

        let outcome = true;
        let message = crate::oracle::construct_oracle_message(&env, market_id, outcome);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        // Fund the contract so the page-settle payout transfer succeeds.
        StellarAssetClient::new(&env, &collateral_token).mint(&contract_id, &(1_000_000_000i128));

        env.as_contract(&contract_id, || settle_positions_page(&env, market_id, 0, 10))
            .expect("resolved market should settle successfully");

        assert_eq!(ot_client.total_supply(&market_id, &TokenKind::Yes), 0);
        assert_eq!(ot_client.balance(&market_id, &user, &TokenKind::Yes), 0);
    }

    // --- Issue #551: batch size hardening ---

    /// An empty user list must be rejected immediately with `BatchTooLarge`.
    /// This surfaces caller bugs early rather than silently returning 0.
    #[test]
    fn test_batch_settle_rejects_empty_batch() {
        let (env, contract_id, market_id, _) = setup_resolved_market();

        let users: Vec<Address> = Vec::new(&env);
        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        assert_eq!(result, Err(ContractError::BatchTooLarge));
    }

    /// A user list whose length exceeds `MAX_BATCH_SETTLE_SIZE` must be
    /// rejected with `BatchTooLarge` to prevent gas-griefing.
    #[test]
    fn test_batch_settle_rejects_oversized_batch() {
        let (env, contract_id, market_id, _) = setup_resolved_market();

        // Build a list that is exactly one over the limit.
        let mut users: Vec<Address> = Vec::new(&env);
        for _ in 0..=crate::MAX_BATCH_SETTLE_SIZE {
            users.push_back(Address::generate(&env));
        }

        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        assert_eq!(result, Err(ContractError::BatchTooLarge));
    }

    /// A list that is exactly `MAX_BATCH_SETTLE_SIZE` entries long must be
    /// accepted — the guard must be `>`, not `>=`.
    /// Users with no position are skipped, so total payout is 0 here (all
    /// addresses are freshly generated ghosts), but the call must not error.
    #[test]
    fn test_batch_settle_accepts_batch_at_max_size() {
        let (env, contract_id, market_id, _) = setup_resolved_market();

        let mut users: Vec<Address> = Vec::new(&env);
        for _ in 0..crate::MAX_BATCH_SETTLE_SIZE {
            users.push_back(Address::generate(&env));
        }

        let result = env.as_contract(&contract_id, || {
            batch_settle_positions(&env, market_id, users)
        });
        // No positions exist for any ghost address — payout is 0, not an error.
        assert_eq!(result, Ok(0));
    }

    /// Issue #706: the gas-griefing cap is exactly 100. Pinned as an explicit
    /// value so a change to the constant is a deliberate, reviewed edit (the
    /// AUTH_TABLE / griefing analysis references this number).
    #[test]
    fn test_max_batch_settle_size_constant_is_100() {
        assert_eq!(crate::MAX_BATCH_SETTLE_SIZE, 100);
    }

    /// Issue #706: the empty-batch and oversize-batch guards fire before the
    /// market is even loaded, so they reject regardless of market state — i.e.
    /// a griefing caller cannot burn gas iterating an unbounded (or spoofed
    /// empty) list. Complements the resolved-market coverage above by pinning
    /// the guard ordering: `BatchTooLarge` wins over `MarketNotFound`.
    #[test]
    fn test_batch_settle_size_guards_precede_market_load() {
        use crate::MarketContract;
        let env = soroban_sdk::Env::default();
        let contract_id = env.register(MarketContract, ());
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
        });

        // Empty list → BatchTooLarge, even though market 12345 does not exist.
        let empty: Vec<Address> = Vec::new(&env);
        assert_eq!(
            env.as_contract(&contract_id, || batch_settle_positions(&env, 12345, empty)),
            Err(ContractError::BatchTooLarge),
        );

        // 101 entries → BatchTooLarge, again before any market lookup.
        let mut oversized: Vec<Address> = Vec::new(&env);
        for _ in 0..=crate::MAX_BATCH_SETTLE_SIZE {
            oversized.push_back(Address::generate(&env));
        }
        assert_eq!(
            env.as_contract(&contract_id, || batch_settle_positions(&env, 12345, oversized)),
            Err(ContractError::BatchTooLarge),
        );
    }
}
