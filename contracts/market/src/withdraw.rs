//! Withdraw unused collateral from a market.
//!
//! `Position::locked_collateral` is the single source of truth for how much
//! collateral backs the user's active YES/NO shares. It is computed and
//! persisted exclusively by `positions::update_position` at the real trade
//! price. Withdraw reads that stored value and never recomputes a lock from a
//! hardcoded price, so the two views can never diverge.
//!
//! ## Withdrawal cooldown (#413 / #565)
//! To deter deposit–withdraw round-trips used to game oracle snapshots or
//! liquidity metrics, a user must wait [`WITHDRAW_COOLDOWN_SECONDS`]
//! (3 600 s / 1 hour) after their most-recent deposit before they can
//! withdraw from the same market. The timestamp is persisted by
//! `deposit_collateral` via [`storage::set_last_deposit_time`]. If no deposit
//! has ever been recorded for a `(market_id, user)` pair, the cooldown is not
//! applied — the guard only kicks in after the first deposit.
//!
//! ```text
//! deposit_collateral   →  storage::set_last_deposit_time(market_id, user, now)
//! withdraw_unused_collateral
//!     elapsed = now - last_deposit_time
//!     if elapsed < 3_600  →  Err(ContractError::WithdrawCooldownActive)
//!     else                →  proceed with withdrawal
//! ```
//!
//! ## Fee deduction (#377)
//! When a fee rate is configured the user must have `amount + fee` of unlocked
//! collateral available. The fee is routed to the treasury (if registered) and
//! both the withdrawal and the fee are deducted from `total_deposited` so the
//! invariant `available = total_deposited - locked_collateral` is preserved.
//!
//! ## Treasury-optional fee behavior (treasury-unset)
//! The treasury address is an *optional* piece of admin configuration
//! (`storage::set_treasury_contract` / `storage::get_treasury`). The chosen,
//! documented behavior when it is unset is:
//!
//! - **Fees are never dropped.** `total_deposited` is always reduced by
//!   `amount + fee_amount`, regardless of whether a treasury is registered.
//!   The user always receives exactly `amount`.
//! - **Skip transfer, don't revert.** When `fee_rate_bps > 0` but
//!   `storage::get_treasury` returns `None`, the withdrawal still succeeds:
//!   the fee portion simply remains in the market contract's own collateral
//!   token balance instead of being forwarded anywhere. The withdrawal is
//!   *not* reverted just because a treasury has not been configured yet.
//! - **Explicit signal.** The `FeeRetainedNoTreasury` event
//!   (see [`crate::events::emit_fee_retained_no_treasury`]) is emitted
//!   whenever a non-zero fee is retained this way, so off-chain indexers and
//!   admins can see the fee is sitting in the contract balance rather than
//!   assuming it reached the treasury. Once a treasury is registered via
//!   `set_treasury_contract`, subsequent withdrawals route the fee there and
//!   `FeeRetainedNoTreasury` is no longer emitted for that market.
//! - Both configurations (`treasury` set and unset) are covered by tests in
//!   this module and in `tests/treasury_unset_fee_test.rs`.

use crate::error::ContractError;
use crate::events::{
    emit_collateral_withdrawn, emit_fee_calculated, emit_fee_retained_no_treasury,
    emit_large_withdraw, emit_withdraw_edge_case,
};
use crate::storage;
use crate::types::{MarketStatus, Position};
use crate::validation;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val, Vec};

/// Seconds a user must wait after their last deposit before withdrawing (issue #413).
const WITHDRAW_COOLDOWN_SECONDS: u64 = 3_600;

/// Withdrawn amount (in stroops) at or above which a dedicated audit event is
/// emitted so operators/indexers can flag unusual outflows (#502).
pub const LARGE_WITHDRAW_THRESHOLD: i128 = 1_000_000_0000000;

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

    // Emergency mode: withdrawals are blocked only in GlobalFreeze;
    // allowed in Normal, TradingHalted, and SettleOnly.
    validation::require_emergency_mode_allows(
        &env,
        &[
            crate::types::EmergencyMode::Normal,
            crate::types::EmergencyMode::TradingHalted,
            crate::types::EmergencyMode::SettleOnly,
        ],
    )?;

    // 1. Validate amount is positive and within safe range.
    validation::validate_collateral_amount(amount)?;

    // 2. Market must exist and be Active.
    let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
    if market.status != MarketStatus::Active {
        return Err(ContractError::MarketNotActive);
    }

    // 3. Enforce cooldown: user must wait WITHDRAW_COOLDOWN_SECONDS after their last deposit.
    if let Some(last_deposit_time) = storage::get_last_deposit_time(&env, market_id, &user) {
        let elapsed = env.ledger().timestamp().saturating_sub(last_deposit_time);
        if elapsed < WITHDRAW_COOLDOWN_SECONDS {
            return Err(ContractError::WithdrawCooldownActive);
        }
    }

    // 4. Load position; an absent or zero-deposited position cannot be withdrawn.
    let mut position = storage::get_position(&env, market_id, &user)?
        .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));

    if position.total_deposited == 0 {
        emit_withdraw_edge_case(&env, &user, market_id, amount);
        return Err(ContractError::InsufficientCollateral);
    }

    // 5. Compute the fee (single path, no duplication).
    // Addresses on the admin-managed fee waiver list pay no withdrawal fee (#483).
    let fee_rate_bps = storage::get_fee_rate_bps(&env);
    validation::validate_fee_rate_bps(fee_rate_bps)?;
    let fee_amount = if fee_rate_bps > 0 && !storage::is_fee_waived(&env, &user) {
        validation::calculate_fee(amount, fee_rate_bps)?
    } else {
        0
    };

    // 6. Enforce locked collateral (#376).
    //    available = total_deposited - locked_collateral (floored at 0).
    //    The user must have `amount + fee_amount` of available (unlocked) collateral.
    let available = position
        .total_deposited
        .saturating_sub(position.locked_collateral);

    // Only emit the fee event when a non-zero fee is actually deducted (#345).
    if fee_amount > 0 {
        emit_fee_calculated(&env, market_id, &user, fee_amount, available);
    }

    let total_required = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    if total_required > available {
        return Err(ContractError::InsufficientCollateral);
    }

  // 7. Update state & persist position FIRST (CEI Pattern)
    let total_deducted = amount
        .checked_add(fee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;

    position.total_deposited = position
        .total_deposited
        .checked_sub(total_deducted)
        .ok_or(ContractError::ArithmeticOverflow)?;

    storage::set_position(&env, market_id, &user, &position)?;

    // 8. Route fee to treasury if one is registered (External Calls)
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
        } else {
            emit_fee_retained_no_treasury(&env, market_id, &user, fee_amount);
        }
    }

    // 9. Transfer the requested amount to the user (single external payout —
    //    a prior bad merge left this `transfer` call duplicated, which would
    //    double-pay the user and drain the contract's collateral balance).
    token_client.transfer(&contract_address, &user, &amount);

    emit_collateral_withdrawn(&env, &user, market_id, amount, position.total_deposited);

    // Audit log for large withdraws (#502): flagged independently of the fee
    // path so operators/indexers can spot unusual outflows without parsing
    // every CollateralWithdrawn event.
    if amount >= LARGE_WITHDRAW_THRESHOLD {
        emit_large_withdraw(&env, &user, market_id, amount, env.ledger().timestamp());
    }

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
            closed_to_deposits: false,
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

    /// Treasury-unset: fee is retained in the contract balance, user still
    /// receives exactly `amount`, and `FeeRetainedNoTreasury` is emitted.
    #[test]
    fn test_withdraw_fee_retained_when_treasury_unset() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::IntoVal;

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
            storage::set_fee_rate_bps(&env, 1_000); // 10%, no treasury registered
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        // User receives exactly the requested amount.
        assert_eq!(token_client.balance(&user), 40);
        // Fee (4) stays inside the contract's own token balance.
        assert_eq!(token_client.balance(&contract_id), 200 - 40 - 4 + 4);
        assert_eq!(token_client.balance(&contract_id), 160);

        // total_deposited still reflects both the withdrawal and the fee.
        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).unwrap().unwrap()
        });
        assert_eq!(updated.total_deposited, 56); // 100 - 40 - 4

        // The explicit "fee retained" event was emitted.
        let events = env.events().all();
        let has_retained_event = events.iter().any(|(_, topics, _)| {
            let topic0: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&env);
            topic0 == soroban_sdk::Symbol::new(&env, "fee_retained_no_treasury")
        });
        assert!(
            has_retained_event,
            "expected FeeRetainedNoTreasury event when treasury is unset"
        );
    }

    /// Treasury-set: fee is routed to the real treasury contract and the
    /// `FeeRetainedNoTreasury` event must NOT fire.
    #[test]
    fn test_withdraw_fee_routed_when_treasury_set_no_retained_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::IntoVal;
        use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

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
        let admin = Address::generate(&env);
        let treasury_addr = env.register(TreasuryContract, ());
        TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &contract_id);

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 1_000); // 10%
            storage::set_treasury(&env, &treasury_addr);
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40)
        });
        assert!(result.is_ok());

        assert_eq!(token_client.balance(&user), 40);
        assert_eq!(token_client.balance(&treasury_addr), 4);

        let events = env.events().all();
        let has_retained_event = events.iter().any(|(_, topics, _)| {
            let topic0: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&env);
            topic0 == soroban_sdk::Symbol::new(&env, "fee_retained_no_treasury")
        });
        assert!(
            !has_retained_event,
            "FeeRetainedNoTreasury must not fire when a treasury is registered"
        );
    }

    /// #711: when the withdraw fee path invokes the treasury's `collect_fee`,
    /// the treasury's canonical `fee_collected` event must be emitted with the
    /// `market_id` topic and a `fee_amount` matching what the user was
    /// actually charged. Guards the indexer-facing event schema against a
    /// regression where the fee is transferred but no matching event fires.
    #[test]
    fn test_withdraw_emits_fee_collected_from_treasury_path() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::{IntoVal, Map, Symbol, TryIntoVal, Val};
        use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

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
        let admin = Address::generate(&env);
        let treasury_addr = env.register(TreasuryContract, ());
        TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &contract_id);

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 1_000); // 10%
            storage::set_treasury(&env, &treasury_addr);
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);

        env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40).unwrap();
        });

        // Locate the treasury's `fee_collected` event among everything emitted
        // during the withdraw (it is not the last event).
        let events = env.events().all();
        let fee_collected = events.iter().find(|(_, topics, _)| {
            let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
            topic0 == Symbol::new(&env, "fee_collected")
        });
        let (_, topics, data) = fee_collected.expect("fee_collected event must be emitted");

        let mid: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(mid, market_id, "fee_collected must carry the market_id topic");

        let data: Map<Symbol, Val> = data.clone().try_into_val(&env).unwrap();
        let fee_amount: i128 = data
            .get(Symbol::new(&env, "fee_amount"))
            .unwrap()
            .into_val(&env);
        assert_eq!(fee_amount, 4, "fee_amount must match the 10% fee on a 40 withdraw");
    }

    /// #711: a zero-fee (or fee-waived) withdraw must not invoke `collect_fee`
    /// at all — no `fee_collected` event, no misleading `amount = 0`.
    #[test]
    fn test_withdraw_zero_fee_emits_no_fee_collected() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::token::StellarAssetClient;
        use soroban_sdk::{IntoVal, Symbol};
        use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

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
        let admin = Address::generate(&env);
        let treasury_addr = env.register(TreasuryContract, ());
        TreasuryContractClient::new(&env, &treasury_addr).initialize(&admin, &contract_id);

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 0); // no fee
            storage::set_treasury(&env, &treasury_addr);
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &200);

        env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 40).unwrap();
        });

        let events = env.events().all();
        let emitted_fee_collected = events.iter().any(|(_, topics, _)| {
            let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
            topic0 == Symbol::new(&env, "fee_collected")
        });
        assert!(
            !emitted_fee_collected,
            "a zero-fee withdraw must not emit fee_collected"
        );
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
        });
        env.mock_all_auths();
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 1)
        });
        assert_eq!(result, Err(ContractError::InsufficientCollateral));
    }

    /// #502: withdrawals below the large-withdraw threshold must not emit the
    /// audit event (only the regular CollateralWithdrawn event fires).
    #[test]
    fn test_withdraw_below_threshold_no_audit_event() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
        let amount = LARGE_WITHDRAW_THRESHOLD - 1;
        let position = Position {
            market_id, user: user.clone(),
            yes_shares: 0, no_shares: 0,
            locked_collateral: 0, total_deposited: amount, is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &amount);
        env.events().all(); // clear setup events
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });
        assert!(result.is_ok());
        let events = env.events().all();
        assert!(
            !events
                .iter()
                .any(|e| e.topics.iter().any(|t| t.to_string().contains("large_withdraw"))),
            "LargeWithdraw audit event should not be emitted below threshold"
        );
    }

    /// #502: withdrawals at/above the large-withdraw threshold must emit the
    /// dedicated audit event alongside the regular withdraw event.
    #[test]
    fn test_withdraw_above_threshold_emits_audit_event() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
        let amount = LARGE_WITHDRAW_THRESHOLD;
        let position = Position {
            market_id, user: user.clone(),
            yes_shares: 0, no_shares: 0,
            locked_collateral: 0, total_deposited: amount, is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &amount);
        env.events().all(); // clear setup events
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });
        assert!(result.is_ok());
        let events = env.events().all();
        assert!(
            events
                .iter()
                .any(|e| e.topics.iter().any(|t| t.to_string().contains("large_withdraw"))),
            "LargeWithdraw audit event should be emitted at/above threshold"
        );
    }

    // ── Withdrawal cooldown (issue #413 / #565) ─────────────────────────────

    /// Early withdrawal is rejected when it falls within the cooldown window.
    ///
    /// The ledger timestamp is 0 by default in the test harness.
    /// A `LastDepositTime` of 0 means 0 seconds have elapsed → still in
    /// the 3 600-second window → `WithdrawCooldownActive`.
    #[test]
    fn test_withdraw_blocked_within_cooldown() {
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
            total_deposited: 1_000,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            // Record a deposit at ledger time 0 (current timestamp in test env).
            storage::set_last_deposit_time(
                &env,
                market_id,
                &user,
                env.ledger().timestamp(),
            );
        });
        env.mock_all_auths();

        // Attempt to withdraw immediately — 0 seconds have elapsed.
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 500)
        });
        assert_eq!(
            result,
            Err(ContractError::WithdrawCooldownActive),
            "withdrawal inside cooldown window must return WithdrawCooldownActive"
        );
    }

    /// Withdrawal succeeds once the cooldown window has fully elapsed.
    ///
    /// We simulate time advancing past `WITHDRAW_COOLDOWN_SECONDS` by
    /// storing a `LastDepositTime` in the past so that
    /// `env.ledger().timestamp() - last_deposit_time >= WITHDRAW_COOLDOWN_SECONDS`.
    #[test]
    fn test_withdraw_allowed_after_cooldown() {
        use soroban_sdk::token::StellarAssetClient;
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &token);
        let withdraw_amount = 500i128;
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 1_000,
            is_settled: false,
        };
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            // Simulate the deposit happening WITHDRAW_COOLDOWN_SECONDS ago.
            // ledger timestamp is 0; deposit time is set to a value that makes
            // elapsed == WITHDRAW_COOLDOWN_SECONDS (exactly on the boundary,
            // which passes the `elapsed < COOLDOWN` guard).
            let past = env
                .ledger()
                .timestamp()
                .saturating_sub(WITHDRAW_COOLDOWN_SECONDS);
            storage::set_last_deposit_time(&env, market_id, &user, past);
        });
        env.mock_all_auths();
        StellarAssetClient::new(&env, &token).mint(&contract_id, &1_000);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, withdraw_amount)
        });
        assert!(
            result.is_ok(),
            "withdrawal after cooldown window must succeed: {result:?}"
        );
    }

    /// A user who has never deposited (no `LastDepositTime` key) is not
    /// subject to any cooldown — the cooldown only applies after a deposit.
    #[test]
    fn test_withdraw_no_deposit_record_bypasses_cooldown() {
        let env = setup_env();
        let user = Address::generate(&env);
        let market_id = 1u32;
        let collateral_token = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        let market = create_test_market(&env, market_id, &collateral_token);
        // No LastDepositTime stored for this user.
        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            // No position either — so we expect InsufficientCollateral, NOT
            // WithdrawCooldownActive, confirming the cooldown is not triggered.
        });
        env.mock_all_auths();
        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, 500)
        });
        assert_ne!(
            result,
            Err(ContractError::WithdrawCooldownActive),
            "cooldown must not fire when no deposit has been recorded"
        );
    }
}
