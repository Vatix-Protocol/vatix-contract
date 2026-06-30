use crate::events::{emit_position_limit_exceeded, emit_position_updated, emit_trade_executed};
use crate::types::{Market, Position};
use crate::validation;
use soroban_sdk::{contracterror, Address, Env};

const BASIS_POINTS: i128 = 10_000;
pub const STROOPS_PER_USDC: i128 = 10_000_000;

/// Errors returned by position validation and update operations.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PositionError {
    /// Proposed YES or NO share change would reduce that side below zero
    ShareBalanceBelowZero = 1,
    /// Market price is outside the valid basis-point range (0–10_000)
    InvalidMarketPrice = 2,
}

/// Scale `amount` by `price_bps` basis points (i.e. `amount * price_bps / 10_000`).
///
/// Uses checked arithmetic to avoid silent overflow. For valid share/price
/// values within the i128 range used by this contract overflow should not occur,
/// but we defensively cap at i128::MAX if it would.
fn scale_by_bps(amount: i128, price_bps: i128) -> i128 {
    let product = amount.saturating_mul(price_bps);
    let result = product.checked_div(BASIS_POINTS).unwrap_or(i128::MAX);
    result
}

/// Calculate required locked collateral based on net position.
///
/// # Arguments
/// * `yes_shares` - Number of YES shares held
/// * `no_shares` - Number of NO shares held
/// * `market_price` - Current market price in basis points (0–10_000)
///
/// # Returns
/// Collateral that must remain locked, in the same unit as the share values.
///
/// # Logic
/// - Net YES  => lock `net_yes * price / 10_000`
/// - Net NO   => lock `net_no * (10_000 - price) / 10_000`
/// - Hedged   => lock `0`
///
/// # Example
/// ```
/// // 100 YES shares at a 60% price => 60 units locked
/// let locked = calculate_locked_collateral(100, 0, 6_000);
/// assert_eq!(locked, 60);
/// ```
pub fn calculate_locked_collateral(yes_shares: i128, no_shares: i128, market_price: i128) -> i128 {
    if yes_shares == no_shares {
        return 0;
    }

    if yes_shares > no_shares {
        scale_by_bps(yes_shares - no_shares, market_price)
    } else {
        scale_by_bps(no_shares - yes_shares, BASIS_POINTS - market_price)
    }
}

/// Validate whether a proposed position change is allowed.
///
/// # Errors
/// Returns [`PositionError::ShareBalanceBelowZero`] when `yes_delta` or
/// `no_delta` would leave either share balance negative.
pub fn validate_position_change(
    current_position: &Position,
    yes_delta: i128,
    no_delta: i128,
) -> Result<(), PositionError> {
    let new_yes = current_position.yes_shares + yes_delta;
    let new_no = current_position.no_shares + no_delta;

    if new_yes < 0 || new_no < 0 {
        return Err(PositionError::ShareBalanceBelowZero);
    }

    Ok(())
}

/// Determine which side exceeded the allowed position limits.
///
/// Returns `true` when the YES side would underflow, or `false` when the NO
/// side would underflow.
fn position_limit_exceeded_side(
    current_position: &Position,
    yes_delta: i128,
    no_delta: i128,
) -> bool {
    let new_yes = current_position.yes_shares + yes_delta;
    let new_no = current_position.no_shares + no_delta;

    #[allow(clippy::nonminimal_bool)]
    let result = new_yes < 0 || (new_no < 0 && new_yes >= 0);
    result
}

/// Calculate net position from YES and NO shares.
///
/// # Arguments
/// * `yes_shares` - Number of YES shares held
/// * `no_shares` - Number of NO shares held
///
/// # Returns
/// Positive value => net long YES, negative => net long NO, zero => hedged.
///
/// # Example
/// ```
/// assert_eq!(calculate_net_position(100, 30), 70);  // net long YES
/// assert_eq!(calculate_net_position(30, 100), -70); // net long NO
/// ```
pub fn calculate_net_position(yes_shares: i128, no_shares: i128) -> i128 {
    yes_shares - no_shares
}

/// Check if a position is eligible for settlement.
///
/// Returns `true` only when the market is `Resolved` and the position has not
/// already been settled.
///
/// # Arguments
/// * `position` - The user's position to check
/// * `market` - The market the position belongs to
///
/// # Example
/// ```
/// // Returns false if position.is_settled == true, even on a resolved market.
/// assert!(!can_settle(&settled_position, &resolved_market));
/// ```
pub fn can_settle(position: &Position, market: &Market) -> bool {
    use crate::types::MarketStatus;
    matches!(market.status, MarketStatus::Resolved) && !position.is_settled
}

/// Update a user's position with new share deltas
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `user` - User address
/// * `yes_delta` - Change in YES shares (can be negative)
/// * `no_delta` - Change in NO shares (can be negative)
/// * `market_price` - Current market price for collateral calculation
///
/// # Returns
/// Updated Position struct
///
/// # Errors
/// - [`PositionError::ShareBalanceBelowZero`] if deltas would make shares negative
pub fn update_position(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_delta: i128,
    no_delta: i128,
    market_price: i128,
) -> Result<Position, PositionError> {
    // 0. Validate market price
    validation::validate_market_price(market_price)
        .map_err(|_| PositionError::InvalidMarketPrice)?;

    // 1. Load or initialize position
    let mut position =
        crate::storage::get_position(env, market_id, user)
            .unwrap_or_else(|_| None)
            .unwrap_or_else(|| Position {
                market_id,
                user: user.clone(),
                yes_shares: 0,
                no_shares: 0,
                locked_collateral: 0,
                total_deposited: 0,
                is_settled: false,
            });

    // 2. Validate deltas
    let side_yes = position_limit_exceeded_side(&position, yes_delta, no_delta);
    if let Err(e) = validate_position_change(&position, yes_delta, no_delta) {
        emit_position_limit_exceeded(env, market_id, user, side_yes);
        return Err(e);
    }

    // 3. Apply deltas
    position.yes_shares += yes_delta;
    position.no_shares += no_delta;

    // 4. Recalculate locked collateral
    let new_locked =
        calculate_locked_collateral(position.yes_shares, position.no_shares, market_price);
    position.locked_collateral = new_locked;

    // 5. Persist
    crate::storage::set_position(env, market_id, user, &position)
        .unwrap_or_default();

    // 6. Emit position_updated event
    emit_position_updated(
        env,
        market_id,
        user,
        position.yes_shares,
        position.no_shares,
        position.locked_collateral,
    );

    // 7. Emit trade_executed event(s) for the actual trades
    if yes_delta > 0 {
        emit_trade_executed(
            env,
            market_id,
            user,
            yes_delta,
            market_price,
            true,
            env.ledger().timestamp(),
        );
    } else if yes_delta < 0 {
        emit_trade_executed(
            env,
            market_id,
            user,
            -yes_delta,
            market_price,
            true,
            env.ledger().timestamp(),
        );
    }

    if no_delta > 0 {
        emit_trade_executed(
            env,
            market_id,
            user,
            no_delta,
            market_price,
            false,
            env.ledger().timestamp(),
        );
    } else if no_delta < 0 {
        emit_trade_executed(
            env,
            market_id,
            user,
            -no_delta,
            market_price,
            false,
            env.ledger().timestamp(),
        );
    }

    Ok(position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{self, AdapterType};
    use soroban_sdk::{testutils::Address as TestAddress, Address, BytesN, Env, String};

    fn setup_env() -> Env {
        Env::default()
    }

    /// Create a sample user address for testing
    fn sample_user(env: &Env, _id: u8) -> Address {
        <Address as TestAddress>::generate(env)
    }

    /// Create a sample market for testing
    fn sample_market(env: &Env) -> Market {
        Market {
            id: 1,
            question: String::from_str(env, "Will it rain tomorrow?"),
            end_time: 0,
            oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
            status: types::MarketStatus::Resolved,
            result: None,
            creator: <Address as TestAddress>::generate(env),
            created_at: 0,
            collateral_token: <Address as TestAddress>::generate(env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
        }
    }

    #[test]
    fn test_calculate_locked_collateral_net_yes() {
        let locked = calculate_locked_collateral(100 * STROOPS_PER_USDC, 0, 6000);
        assert_eq!(locked, 60 * STROOPS_PER_USDC);

        let locked =
            calculate_locked_collateral(100 * STROOPS_PER_USDC, 30 * STROOPS_PER_USDC, 5000);
        assert_eq!(locked, 35 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_net_no() {
        let locked = calculate_locked_collateral(0, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 40 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_calculate_locked_collateral_hedged() {
        let locked =
            calculate_locked_collateral(100 * STROOPS_PER_USDC, 100 * STROOPS_PER_USDC, 6000);
        assert_eq!(locked, 0);
    }

    #[test]
    fn test_validate_position_change() {
        let env = setup_env();
        let position = Position {
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 50,
            no_shares: 50,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };

        assert!(validate_position_change(&position, 10, -20).is_ok());
        assert_eq!(
            validate_position_change(&position, -60, 0),
            Err(PositionError::ShareBalanceBelowZero)
        );
        assert_eq!(
            validate_position_change(&position, 0, -60),
            Err(PositionError::ShareBalanceBelowZero)
        );
    }

    #[test]
    fn test_calculate_net_position() {
        assert_eq!(calculate_net_position(100, 30), 70);
        assert_eq!(calculate_net_position(30, 100), -70);
        assert_eq!(calculate_net_position(50, 50), 0);
    }

    #[test]
    fn test_can_settle_resolved_market() {
        let env = setup_env();
        let market = sample_market(&env);
        let position = Position {
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        };

        assert!(can_settle(&position, &market));
    }

    #[test]
    fn test_can_settle_already_settled() {
        let env = setup_env();
        let market = sample_market(&env);
        let position = Position {
            market_id: 1,
            user: sample_user(&env, 1),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: true,
        };

        assert!(!can_settle(&position, &market));
    }

    #[test]
    fn test_update_position_limit_exceeded_emits_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::IntoVal;

        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 3);
        let market_id = 3;

        let result = env.as_contract(&contract_id, || {
            crate::storage::set_version(&env);
            // Try to sell 50 YES shares the user doesn't have
            update_position(&env, market_id, &user, -50, 0, 5000)
        });

        assert_eq!(result.unwrap_err(), PositionError::ShareBalanceBelowZero);

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let topic0: soroban_sdk::Symbol = events.first().unwrap().1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic0,
            soroban_sdk::Symbol::new(&env, "position_limit_exceeded")
        );
    }

    #[test]
    fn test_update_position_emits_position_updated_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::IntoVal;

        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 5);
        let market_id = 5;

        env.as_contract(&contract_id, || {
            crate::storage::set_version(&env);
            update_position(&env, market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000)
                .expect("position update should succeed");
        });

        let events = env.events().all();
        // Now we emit both position_updated and trade_executed events
        assert_eq!(events.len(), 2);

        let topic0: soroban_sdk::Symbol = events.first().unwrap().1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic0,
            soroban_sdk::Symbol::new(&env, "position_updated")
        );

        let topic1: soroban_sdk::Symbol = events.get(1).unwrap().1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic1,
            soroban_sdk::Symbol::new(&env, "trade_executed")
        );
    }

    #[test]
    fn test_update_position_rejects_invalid_market_price() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 9);
        let market_id = 9;

        for bad_price in [-1i128, 10_001] {
            let result = env.as_contract(&contract_id, || {
                crate::storage::set_version(&env);
                update_position(&env, market_id, &user, 100, 0, bad_price)
            });
            assert_eq!(result, Err(PositionError::InvalidMarketPrice));
        }
    }

    #[test]
    fn test_update_position_new_user() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 1);
        let market_id = 1;

        let pos = env.as_contract(&contract_id, || {
            crate::storage::set_version(&env);
            update_position(&env, market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000)
                .expect("should update position")
        });

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 0);
        assert_eq!(pos.locked_collateral, 60 * STROOPS_PER_USDC);
        assert!(!pos.is_settled);
    }

    #[test]
    fn test_update_position_existing_user() {
        let env = setup_env();
        let contract_id = env.register(crate::MarketContract, ());
        let user = sample_user(&env, 2);
        let market_id = 2;

        // First update - buy YES
        let _ = env.as_contract(&contract_id, || {
            crate::storage::set_version(&env);
            update_position(&env, market_id, &user, 100 * STROOPS_PER_USDC, 0, 6000).unwrap()
        });

        // Second update - buy some NO
        let pos = env.as_contract(&contract_id, || {
            update_position(&env, market_id, &user, 0, 30 * STROOPS_PER_USDC, 6000).unwrap()
        });

        assert_eq!(pos.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 30 * STROOPS_PER_USDC);
        assert_eq!(pos.locked_collateral, 42 * STROOPS_PER_USDC);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crate::types::Position;
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as TestAddress, Address, Env};

    // Upper bound chosen so that `net * 10_000` never overflows i128.
    // scale_by_bps does `amount.checked_mul(price_bps).unwrap()`, so any
    // `amount` up to this value is safe for all valid prices [0, 10_000].
    const MAX_SAFE_SHARES: i128 = i128::MAX / 10_001;

    fn make_position(env: &Env, yes_shares: i128, no_shares: i128) -> Position {
        Position {
            market_id: 0,
            user: <Address as TestAddress>::generate(env),
            yes_shares,
            no_shares,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1_000))]

        /// Locked collateral is always >= 0 for any valid inputs.
        #[test]
        fn prop_locked_collateral_never_negative(
            yes in 0i128..=MAX_SAFE_SHARES,
            no in 0i128..=MAX_SAFE_SHARES,
            price in 0i128..=10_000i128,
        ) {
            let locked = calculate_locked_collateral(yes, no, price);
            prop_assert!(
                locked >= 0,
                "locked={locked} yes={yes} no={no} price={price}"
            );
        }

        /// Equal YES and NO shares always produce zero locked collateral,
        /// regardless of market price.
        #[test]
        fn prop_locked_collateral_hedged_is_zero(
            shares in 0i128..=MAX_SAFE_SHARES,
            price in 0i128..=10_000i128,
        ) {
            prop_assert_eq!(calculate_locked_collateral(shares, shares, price), 0);
        }

        /// Locked collateral never exceeds the absolute net position.
        /// Invariant: locked <= |yes - no|.
        #[test]
        fn prop_locked_lte_net_position(
            yes in 0i128..=MAX_SAFE_SHARES,
            no in 0i128..=MAX_SAFE_SHARES,
            price in 0i128..=10_000i128,
        ) {
            let locked = calculate_locked_collateral(yes, no, price);
            let net = (yes - no).abs();
            prop_assert!(
                locked <= net,
                "locked={locked} > net={net}  yes={yes} no={no} price={price}"
            );
        }

        /// At price = 5_000 (50 %), long-YES and long-NO of equal net magnitude
        /// lock the same amount of collateral (symmetry).
        #[test]
        fn prop_locked_symmetric_at_midpoint(
            net in 0i128..=MAX_SAFE_SHARES,
        ) {
            let yes_heavy = calculate_locked_collateral(net, 0, 5_000);
            let no_heavy  = calculate_locked_collateral(0, net, 5_000);
            prop_assert_eq!(yes_heavy, no_heavy);
        }

        /// At price = 0 a net-YES position locks nothing; a net-NO position
        /// locks its full magnitude (cost-to-close at 100 % price).
        #[test]
        fn prop_locked_at_zero_price(
            yes in 0i128..=MAX_SAFE_SHARES,
            no in 0i128..=MAX_SAFE_SHARES,
        ) {
            let locked = calculate_locked_collateral(yes, no, 0);
            if yes >= no {
                prop_assert_eq!(locked, 0, "net-YES at price=0 should lock 0");
            } else {
                prop_assert_eq!(
                    locked, no - yes,
                    "net-NO at price=0 should lock (no-yes)"
                );
            }
        }

        /// At price = 10_000 a net-NO position locks nothing; a net-YES
        /// position locks its full magnitude.
        #[test]
        fn prop_locked_at_full_price(
            yes in 0i128..=MAX_SAFE_SHARES,
            no in 0i128..=MAX_SAFE_SHARES,
        ) {
            let locked = calculate_locked_collateral(yes, no, 10_000);
            if no >= yes {
                prop_assert_eq!(locked, 0, "net-NO at price=10000 should lock 0");
            } else {
                prop_assert_eq!(
                    locked, yes - no,
                    "net-YES at price=10000 should lock (yes-no)"
                );
            }
        }

        /// validate_position_change returns Err iff the resulting share balance
        /// would drop below zero on either side.
        #[test]
        fn prop_validate_rejects_iff_shares_go_negative(
            yes_shares in 0i128..=1_000_000i128,
            no_shares in 0i128..=1_000_000i128,
            yes_delta in -1_000_000i128..=1_000_000i128,
            no_delta in -1_000_000i128..=1_000_000i128,
        ) {
            let env = Env::default();
            let pos = make_position(&env, yes_shares, no_shares);
            let result = validate_position_change(&pos, yes_delta, no_delta);
            let new_yes = yes_shares + yes_delta;
            let new_no  = no_shares  + no_delta;
            if new_yes < 0 || new_no < 0 {
                prop_assert_eq!(result, Err(PositionError::ShareBalanceBelowZero));
            } else {
                prop_assert!(result.is_ok());
            }
        }

        /// After a valid position change both share counts are non-negative.
        #[test]
        fn prop_valid_position_has_non_negative_shares(
            yes_shares in 0i128..=1_000_000i128,
            no_shares in 0i128..=1_000_000i128,
            yes_delta in -1_000_000i128..=1_000_000i128,
            no_delta in -1_000_000i128..=1_000_000i128,
        ) {
            let env = Env::default();
            let pos = make_position(&env, yes_shares, no_shares);
            if validate_position_change(&pos, yes_delta, no_delta).is_ok() {
                prop_assert!(yes_shares + yes_delta >= 0);
                prop_assert!(no_shares  + no_delta  >= 0);
            }
        }

        /// calculate_net_position is antisymmetric: swapping YES and NO negates it.
        #[test]
        fn prop_net_position_antisymmetric(
            yes in 0i128..=1_000_000_000i128,
            no in 0i128..=1_000_000_000i128,
        ) {
            let net         = calculate_net_position(yes, no);
            let net_swapped = calculate_net_position(no, yes);
            prop_assert_eq!(net, -net_swapped);
        }
    }
}
