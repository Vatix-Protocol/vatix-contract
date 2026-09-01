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

/// RAII-style reentrancy guard for the deposit path (Issue #501).
///
/// `deposit_collateral` performs an external token transfer (a cross-contract
/// call) before it finishes updating this contract's own state. Without a
/// guard, a malicious or upgraded token contract could call back into
/// `deposit_collateral` from within that transfer and re-enter before the
/// first call's state update has been persisted. The lock is released
/// automatically on `Drop`, so it clears on every exit path (success or
/// error) without needing to touch each `return`/`?` site individually.
struct DepositReentrancyGuard<'a> {
    env: &'a Env,
}

impl<'a> DepositReentrancyGuard<'a> {
    fn acquire(env: &'a Env) -> Result<Self, ContractError> {
        if storage::is_deposit_locked(env) {
            return Err(ContractError::ReentrantCall);
        }
        storage::set_deposit_locked(env, true);
        Ok(Self { env })
    }
}

impl<'a> Drop for DepositReentrancyGuard<'a> {
    fn drop(&mut self) {
        storage::set_deposit_locked(self.env, false);
    }
}

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

    // Emergency mode: deposits are blocked unless mode is Normal
    crate::validation::require_emergency_mode_allows(
        &env,
        &[crate::types::EmergencyMode::Normal],
    )?;

    // Reentrancy guard: held for the remainder of this call, released
    // automatically when it goes out of scope.
    let _guard = DepositReentrancyGuard::acquire(&env)?;

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

    // Reject deposits at or after end_time (Issue #549). A market expires the
    // moment the ledger timestamp reaches end_time, so a deposit submitted in
    // the same ledger second as expiry must be rejected (>= not just >).
    //
    // Using strict greater-than (>) would admit a deposit at exactly end_time,
    // which is inside the expired window from the market's perspective — the
    // same ledger second is simultaneously the last valid trading instant and
    // the first expired one, so we treat it as expired and reject it.
    if env.ledger().timestamp() >= market.end_time {
        return Err(ContractError::MarketExpired);
    }

    // TODO: Refactor collateral management
    // Current design requires separate deposits per market. Users cannot use
    // Market A collateral for Market B trades. refactor will introduce:
    // - Global user balance (deposit once, trade anywhere)
    // - Better capital efficiency
    //
    // Every deposit credits `storage::CollateralBalance(user)`, a balance
    // scoped by *user only* (not by market — see `StorageKey::CollateralBalance`
    // in `storage.rs`). `MarketContract::update_position` checks a trade's
    // prospective lock against this shared balance (net of whatever is
    // already locked in the user's *other* markets, tracked in
    // `StorageKey::TotalLockedCollateral`), which is what lets a user deposit
    // once and trade in any market without a second deposit.
    //
    // The legacy per-market `Position.total_deposited` field below is kept
    // as-is for backward compatibility with `withdraw_unused_collateral` and
    // settlement, which still refund/settle per market; migrating those to
    // draw from the protocol-wide balance is tracked as follow-up work in
    // the ADR.
    let new_collateral_balance = storage::get_collateral_balance(&env, &user)
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    storage::set_collateral_balance(&env, &user, new_collateral_balance);

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

    // Persist updated position — done BEFORE the external token transfer
    // below (Checks-Effects-Interactions, Issue #695). Previously the
    // transfer ran first and every state write happened after it; the
    // `DepositReentrancyGuard` above (#501) already blocks a reentrant
    // second call into `deposit_collateral` from inside that transfer, but
    // ordering state writes before the external call is defense in depth
    // that costs nothing here — nothing below depends on the transfer's
    // return value, and a failed/panicking transfer aborts the whole
    // transaction (including these writes) atomically regardless of order.
    storage::set_position(&env, market_id, &user, &position)?;

    // Track first-time depositors in the MarketParticipants list (Issue #546 / #495).
    //
    // `add_market_participant` is idempotent — it performs a linear search
    // before appending, so repeated deposits by the same user produce no
    // duplicate entries. Calling it here (in addition to the call in
    // `update_position`) ensures that users who deposit collateral but never
    // execute a trade are still present in the list and will be reached by
    // the paginated `settle_positions_page` settlement path.
    storage::add_market_participant(&env, market_id, &user);

    // Record deposit timestamp for cooldown enforcement on withdrawals (issue #413).
    storage::set_last_deposit_time(&env, market_id, &user, env.ledger().timestamp());

    // Transfer USDC from user to contract — the external (Interactions) call,
    // now ordered after every state write above (Checks-Effects-Interactions,
    // Issue #695).
    let contract_address = env.current_contract_address();
    let token_client = TokenClient::new(&env, &market.collateral_token);
    token_client.transfer(&user, &contract_address, &amount);

    // TODO(#issue): consider batching deposit events for gas efficiency
    // Emit event
    emit_collateral_deposited(&env, &user, market_id, amount, position.total_deposited);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AdapterType, Market};
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
            closed_to_deposits: false,
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

    // --- #586: i128 boundary deposits — overflow must never panic ---

    /// Depositing `i128::MAX` must be rejected by `validate_collateral_amount`
    /// (the `validate_amount_reasonable` guard catches amounts above
    /// `i128::MAX / 2`) before any arithmetic is attempted, so no overflow
    /// or panic can occur.
    #[test]
    fn test_deposit_i128_max_rejected_no_panic() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });
        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, i128::MAX)
        });
        assert_eq!(
            result,
            Err(ContractError::InvalidQuantity),
            "i128::MAX deposit must be rejected before any arithmetic"
        );
    }

    /// Depositing `i128::MAX / 2 + 1` (one above the reasonable-amount cap)
    /// must be rejected. Tests the exact boundary of `validate_amount_reasonable`.
    #[test]
    fn test_deposit_just_above_reasonable_cap_rejected() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });
        env.mock_all_auths();

        // i128::MAX / 2 + 1 is above the MAX_REASONABLE_AMOUNT cap.
        let above_cap = i128::MAX / 2 + 1;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, above_cap)
        });
        assert_eq!(
            result,
            Err(ContractError::InvalidQuantity),
            "amount just above the reasonable cap must be rejected"
        );
    }

    /// Depositing exactly `i128::MAX / 2` (the boundary value itself) must
    /// also be rejected: the cap is exclusive (`> MAX_REASONABLE_AMOUNT`
    /// means the cap equals `i128::MAX / 2`, so amounts *equal* to the cap
    /// are treated as reasonable). Confirm both directions around the boundary.
    #[test]
    fn test_deposit_at_reasonable_cap_boundary() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
        });
        env.mock_all_auths();

        // MAX / 2 is the cap — amounts strictly above it are rejected.
        // This value passes the quantity check but the token transfer will
        // fail (no real balance). The key is no panic from overflow.
        let at_cap = i128::MAX / 2;
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, at_cap)
        });
        // We expect either success (token transfer would fail for other reasons)
        // or a non-panic error, but never ArithmeticOverflow from checked_add.
        assert_ne!(
            result,
            Err(ContractError::ArithmeticOverflow),
            "boundary amount must not cause ArithmeticOverflow inside deposit"
        );
    }

    /// Two near-maximum deposits that would together overflow `total_deposited`
    /// must be caught by `checked_add` and returned as `ArithmeticOverflow`,
    /// never as a panic.
    #[test]
    fn test_deposit_second_deposit_accumulation_overflow_caught() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);

        // Seed the position with a very large total_deposited directly in
        // storage (bypassing the amount validation that would reject it), then
        // attempt to add a second deposit that would overflow the running sum.
        let large = i128::MAX - 1_000i128;
        let position = crate::types::Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: large,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });
        env.mock_all_auths();
        // Mint a small amount so the token transfer itself won't fail first.
        StellarAssetClient::new(&env, &collateral_token).mint(&user, &2_000i128);

        // A deposit of 2_000 would push total_deposited past i128::MAX → overflow.
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 2_000i128)
        });
        assert_eq!(
            result,
            Err(ContractError::ArithmeticOverflow),
            "overflow in total_deposited accumulation must return ArithmeticOverflow, not panic"
        );
    }

    // --- #501 / #709: deposit reentrancy guard ---

    /// Regression test for the deposit reentrancy guard (#501, #709).
    ///
    /// The lock is held for the whole `deposit_collateral` call, wrapping the
    /// external collateral-token `transfer`. A malicious or upgraded token
    /// contract that calls back into `deposit_collateral` from inside its
    /// `transfer` implementation lands on exactly this lock. Simulating the
    /// lock being already held (as it would be during that reentrant call)
    /// must make `deposit_collateral` fail closed with `ReentrantCall`. If
    /// `DepositReentrancyGuard::acquire` is ever removed, this test fails.
    #[test]
    fn test_deposit_rejected_while_reentrancy_lock_held() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        let market = create_test_market(&env, market_id, &collateral_token);
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            // Simulate being mid-transfer inside an outer deposit call.
            storage::set_deposit_locked(&env, true);
        });
        env.mock_all_auths();

        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 5_000)
        });
        assert_eq!(result, Err(ContractError::ReentrantCall));
    }

    /// The lock must be released once the deposit returns (RAII `Drop`), so a
    /// legitimate follow-up deposit still works. Guards against a regression
    /// that leaves the lock stuck set.
    #[test]
    fn test_deposit_lock_released_after_successful_deposit() {
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
        StellarAssetClient::new(&env, &collateral_token).mint(&user, &20_000);

        env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 5_000).unwrap();
        });

        let locked = env.as_contract(&contract_id, || storage::is_deposit_locked(&env));
        assert!(!locked, "deposit lock must be cleared after the call returns");

        // Second deposit still succeeds now that the lock is clear.
        let result = env.as_contract(&contract_id, || {
            deposit_collateral(env.clone(), user.clone(), market_id, 1_000)
        });
        assert!(result.is_ok());
    }
}
