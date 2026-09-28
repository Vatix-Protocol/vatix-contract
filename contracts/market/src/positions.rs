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
    /// The user's protocol-wide collateral balance cannot cover this
    /// market's prospective lock once collateral already locked in the
    /// user's other markets is accounted for (ADR-002, issue #685).
    InsufficientProtocolCollateral = 3,
    /// Caller is not the position owner nor an approved operator
    Unauthorized = 4,
    /// Transfer/merge request id has already been processed (replay)
    DuplicateRequest = 5,
    /// Source and destination positions belong to different markets
    MarketMismatch = 6,
    /// Transfer/merge would leave the source position settled or invalid
    InvalidPositionState = 7,
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

/// Check whether a user's protocol-wide collateral balance (ADR-002, issue
/// #685) can cover a prospective locked-collateral amount for one market,
/// once collateral already locked in the user's *other* markets is taken
/// into account.
///
/// Replaces the old per-market check against `Position.total_deposited`:
/// `collateral_balance` is the user's single balance shared across every
/// market (see `storage::CollateralBalance`), and `locked_elsewhere` is the
/// sum of `locked_collateral` across every *other* market the user holds a
/// position in (see `storage::TotalLockedCollateral`). A trade that would
/// only keep this market's lock flat or reduce it is never rejected by this
/// check — callers should only invoke it when the lock is increasing.
///
/// # Errors
/// Returns [`PositionError::InsufficientProtocolCollateral`] when
/// `prospective_locked + locked_elsewhere > collateral_balance`.
pub fn check_protocol_collateral(
    prospective_locked: i128,
    collateral_balance: i128,
    locked_elsewhere: i128,
) -> Result<(), PositionError> {
    if prospective_locked.saturating_add(locked_elsewhere) > collateral_balance {
        return Err(PositionError::InsufficientProtocolCollateral);
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

/// Authorize a position transfer/merge request.
///
/// Deny-by-default: the caller must be the position owner, or an operator
/// explicitly approved by the owner (see `storage::is_operator`). Any other
/// caller is rejected with [`PositionError::Unauthorized`].
///
/// # Errors
/// Returns [`PositionError::Unauthorized`] when `caller` is neither the owner
/// nor an approved operator for `owner`.
pub fn authorize_position_operator(
    env: &Env,
    owner: &Address,
    caller: &Address,
) -> Result<(), PositionError> {
    if caller == owner {
        return Ok(());
    }
    if crate::storage::is_operator(env, owner, caller) {
        return Ok(());
    }
    Err(PositionError::Unauthorized)
}

/// Guard against replay of a transfer/merge request.
///
/// Each request carries a caller-supplied `request_id`; the first successful
/// use of an id is recorded and any subsequent use is rejected with
/// [`PositionError::DuplicateRequest`]. This makes transfer/merge idempotent
/// under retries and concurrent submission.
///
/// # Errors
/// Returns [`PositionError::DuplicateRequest`] when `request_id` was already
/// consumed for `owner`.
pub fn consume_request_id(
    env: &Env,
    owner: &Address,
    request_id: u64,
) -> Result<(), PositionError> {
    if crate::storage::is_request_consumed(env, owner, request_id) {
        return Err(PositionError::DuplicateRequest);
    }
    crate::storage::mark_request_consumed(env, owner, request_id);
    Ok(())
}

/// Transfer a position (or a share slice of it) from `from` to `to`.
///
/// Moves `yes_shares`/`no_shares` out of the source position and into the
/// destination position within the same market, then recomputes locked
/// collateral on both sides from the current `market_price`. The caller must
/// be the source owner or an approved operator, and `request_id` must be
/// unused (replay protection). All validation happens before any state is
/// written, so a failure leaves both positions untouched (fail-closed).
///
/// # Errors
/// - [`PositionError::Unauthorized`] caller is not owner/operator
/// - [`PositionError::DuplicateRequest`] `request_id` already consumed
/// - [`PositionError::InvalidMarketPrice`] price outside 0–10_000
/// - [`PositionError::InvalidPositionState`] source is settled or empty
/// - [`PositionError::ShareBalanceBelowZero`] transfer exceeds source shares
pub fn transfer_position(
    env: &Env,
    market_id: u32,
    from: &Address,
    to: &Address,
    caller: &Address,
    yes_shares: i128,
    no_shares: i128,
    market_price: i128,
    request_id: u64,
) -> Result<(), PositionError> {
    validation::validate_market_price(market_price)
        .map_err(|_| PositionError::InvalidMarketPrice)?;
    authorize_position_operator(env, from, caller)?;
    consume_request_id(env, from, request_id)?;

    if yes_shares < 0 || no_shares < 0 {
        return Err(PositionError::ShareBalanceBelowZero);
    }

    let mut source = crate::storage::get_position(env, market_id, from)
        .unwrap_or_else(|_| None)
        .ok_or(PositionError::InvalidPositionState)?;
    if source.is_settled {
        return Err(PositionError::InvalidPositionState);
    }

    validate_position_change(&source, -yes_shares, -no_shares)?;

    let mut dest = crate::storage::get_position(env, market_id, to)
        .unwrap_or_else(|_| None)
        .unwrap_or_else(|| Position {
            market_id,
            user: to.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        });
    if dest.is_settled {
        return Err(PositionError::InvalidPositionState);
    }

    source.yes_shares -= yes_shares;
    source.no_shares -= no_shares;
    dest.yes_shares += yes_shares;
    dest.no_shares += no_shares;

    source.locked_collateral =
        calculate_locked_collateral(source.yes_shares, source.no_shares, market_price);
    dest.locked_collateral =
        calculate_locked_collateral(dest.yes_shares, dest.no_shares, market_price);

    crate::storage::set_position(env, market_id, from, &source).unwrap_or_default();
    crate::storage::set_position(env, market_id, to, &dest).unwrap_or_default();

    emit_position_updated(env, market_id, from);
    emit_position_updated(env, market_id, to);
    Ok(())
}

/// Merge two positions held by the same owner in the same market.
///
/// Combines `other` into `primary` (netting YES/NO shares and summing
/// `total_deposited`), recomputes locked collateral from `market_price`, and
/// zeroes out the merged position. The caller must be the owner or an
/// approved operator, and `request_id` must be unused. Validation precedes
/// all writes so a failure leaves both positions untouched (fail-closed).
///
/// # Errors
/// - [`PositionError::Unauthorized`] caller is not owner/operator
/// - [`PositionError::DuplicateRequest`] `request_id` already consumed
/// - [`PositionError::InvalidMarketPrice`] price outside 0–10_000
/// - [`PositionError::InvalidPositionState`] either position is settled
pub fn merge_positions(
    env: &Env,
    market_id: u32,
    owner: &Address,
    primary: &Address,
    other: &Address,
    caller: &Address,
    market_price: i128,
    request_id: u64,
) -> Result<(), PositionError> {
    validation::validate_market_price(market_price)
        .map_err(|_| PositionError::InvalidMarketPrice)?;
    authorize_position_operator(env, owner, caller)?;
    consume_request_id(env, owner, request_id)?;

    let mut primary_pos = crate::storage::get_position(env, market_id, primary)
        .unwrap_or_else(|_| None)
        .ok_or(PositionError::InvalidPositionState)?;
    let mut other_pos = crate::storage::get_position(env, market_id, other)
        .unwrap_or_else(|_| None)
        .ok_or(PositionError::InvalidPositionState)?;

    if primary_pos.is_settled || other_pos.is_settled {
        return Err(PositionError::InvalidPositionState);
    }

    primary_pos.yes_shares += other_pos.yes_shares;
    primary_pos.no_shares += other_pos.no_shares;
    primary_pos.total_deposited += other_pos.total_deposited;
    primary_pos.locked_collateral = calculate_locked_collateral(
        primary_pos.yes_shares,
        primary_pos.no_shares,
        market_price,
    );

    other_pos.yes_shares = 0;
    other_pos.no_shares = 0;
    other_pos.locked_collateral = 0;
    other_pos.total_deposited = 0;

    crate::storage::set_position(env, market_id, primary, &primary_pos).unwrap_or_default();
    crate::storage::set_position(env, market_id, other, &other_pos).unwrap_or_default();

    emit_position_updated(env, market_id, primary);
    emit_position_updated(env, market_id, other);
    Ok(())
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
    let mut position = crate::storage::get_position(env, market_id, user)
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
    crate::storage::set_position(env, market_id, user, &position).unwrap_or_default();

    // 6. Emit position_updated event
    emit_position_updated(
        env,
        market_id,
        user,
    

/* … truncated 16328 chars — edit only what you need near the top … */
