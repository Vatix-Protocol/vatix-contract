use soroban_sdk::contracterror;

/// Error codes for the Vatix market contract.
///
/// Errors are grouped by category with reserved number ranges:
/// - Market Errors: 1-9
/// - Position Errors: 10-19
/// - Oracle Errors: 20-29
/// - Validation Errors: 30-39
/// - Authorization Errors: 41-49
/// - Token Errors: 50-59
/// - Arithmetic Errors: 60-69
/// - Treasury Errors: 70-79
/// - Reconciliation Errors: 80-89
/// - Conservation Errors: 90-99
///
/// # Stable not-found codes
///
/// Market lookups that fail because the requested market does not exist MUST
/// return [`ContractError::MarketNotFound`] (`= 1`). This code is part of the
/// public ABI: clients (e.g. `apps/web/lib/errors.ts`) map it to a stable,
/// user-facing "market not found" error. Do not renumber or reuse it, and do
/// not substitute a generic validation error for a missing market.
///
/// # Example
/// ```ignore
/// use vatix_market::error::ContractError;
///
/// // Check for specific error
/// match result {
///     Err(ContractError::MarketNotFound) => println!("Market does not exist"),
///     Err(ContractError::InvalidQuestion) => println!("Question is invalid"),
///     Ok(_) => println!("Success"),
/// }
/// ```
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    // ========== Market Errors (1-9) ==========
    /// The requested market does not exist in storage.
    ///
    /// Returned when attempting to access a market with an invalid or non-existent ID.
    ///
    /// This is the single, stable not-found code for market lookups. Every
    /// market entrypoint that resolves a `market_id` to a stored market MUST
    /// return this variant when the lookup misses, so clients can rely on a
    /// deterministic code instead of a generic failure.
    MarketNotFound = 1,

    /// Attempted to resolve a market that has already been resolved.
    ///
    /// Each market can only be resolved once. Attempting to resolve again will fail.
    MarketAlreadyResolved = 2,

    /// Settlement was attempted but the market has not been resolved yet.
    ///
    /// Wait for the oracle to submit a valid resolution before settling positions.
    MarketNotResolved = 3,

    /// Market has passed its end_time and is no longer active for trading.
    ///
    /// No new positions can be opened or modified after the market expires.
    MarketExpired = 4,

    /// Market is not in Active status (may be Resolved or Canceled).
    ///
    /// Only Active markets accept new trades and collateral deposits.
    MarketNotActive = 5,

    /// New collateral deposits are disabled for this market.
    ///
    /// The admin has called `close_market_to_deposits`. Deposits are blocked
    /// outright, and `update_position` calls that would increase locked
    /// collateral (opening new exposure) are also rejected. Trades that
    /// reduce or hold the lock flat (closing/reducing a position) and
    /// withdrawals continue to work normally.
    MarketClosedToDeposits = 6,

    /// Withdraw attempted before the cooldown period since the last deposit has elapsed.
    WithdrawCooldownActive = 7,

    /// The market has already been closed.
    ///
    /// `close_market` transitions a market into the terminal Closed state.
    /// Closing is idempotent-hostile by design: a second close attempt (or a
    /// replayed close request) is rejected so callers cannot re-run the
    /// money-path side effects (settlement gating, event emission) twice.
    MarketAlreadyClosed = 8,

    /// A settlement claim was submitted before the market was resolved.
    ///
    /// Claims are only valid once the oracle has resolved the market. This is
    /// enforced contract-side (not by clients) so an untrusted caller cannot
    /// bypass the resolve gate and drain liquidity against an unresolved
    /// outcome. Fail-closed: the claim is rejected outright rather than
    /// silently deferred.
    ClaimBeforeResolve = 9,

    // ========== Position Errors (10-19) ==========
    /// User does not have enough collateral locked to perform this operation.
    ///
    /// Ensure sufficient collateral is deposited before attempting trades.
    InsufficientCollateral = 10,

    /// Settlement was attempted on a position that has already been paid out.
    ///
    /// Each position can only be settled once.
    PositionAlreadySettled = 11,

    /// No position exists for this user in this market.
    ///
    /// The user must have an open position to perform this operation.
    NoPositionFound = 12,

    /// Share amount is invalid (e.g., negative or zero when positive required).
    ///
    /// Share amounts must be non-negative, and at least one side must be positive.
    InvalidShareAmount = 13,

    /// The batch supplied to `batch_settle_positions` was either empty or exceeded
    /// `MAX_BATCH_SETTLE_SIZE`. Empty batches are rejected to surface caller bugs
    /// early; oversized batches are rejected to prevent gas-griefing attacks.
    BatchTooLarge = 14,

    // ========== Oracle Errors (20-29) ==========
    /// Oracle signature verification failed.
    ///
    /// The provided signature does not match the oracle's public key or the market data.
    InvalidSignature = 20,

    /// Caller is not the authorized oracle for this market.
    ///
    /// Only the designated oracle can submit resolutions for this market.
    UnauthorizedOracle = 21,

    /// Resolution outcome value is invalid (must be true or false).
    ///
    /// Outcome must be a valid boolean value.
    InvalidOutcome = 22,

    /// Reflector oracle returned no price for the requested asset.
    ///
    /// Occurs when `lastprice(asset)` returns `None` — the asset may be
    /// unsupported, the oracle may not have a recent price, or the Reflector
    /// node network may be temporarily disconnected.
    OraclePriceUnavailable = 23,

    /// The oracle message has an `expires_at` deadline that has already
    /// passed. Signed outcomes cannot be replayed after their stated
    /// deadline — submit a freshly signed message instead.
    OracleMessageExpired = 24,

    /// The requested threshold is invalid (e.g., higher than signer count or zero).
    InvalidThresholdQuorum = 25,

    /// A Reflector/Pyth price observation is older than the adapter's
    /// maximum allowed staleness window (#682).
    ///
    /// Both adapters read a timestamp/publish_time alongside the price but
    /// previously never checked it — a disconnected or slow oracle feed
    /// could otherwise resolve a market against an arbitrarily old price.
    StalePrice = 26,

    // ========== Validation Errors (30-39) ==========
    /// Price is out of valid range (must be between 0 and 1).
    ///
    /// Prices represent probabilities and must be normalized.
    InvalidPrice = 30,

    /// Quantity is invalid (must be positive).
    ///
    /// Quantities, amounts, and counts must be greater than zero.
    InvalidQuantity = 31,

    /// Timestamp is invalid (e.g., end_time in the past or too far in future).
    ///
    /// Market end_time must be in the future and within one year.
    InvalidTimestamp = 32,

    /// Market question is invalid (e.g., empty string or exceeds 500 characters).
    ///
    /// Questions must be non-empty and reasonably sized (1-499 characters).
    InvalidQuestion = 33,

    /// Outcome count is not exactly 2.
    ///
    /// All markets on this protocol are binary (YES/NO). Any attempt to create
    /// or overwrite a market with an outcome_count other than 2 is rejected.
    InvalidOutcomeCount = 34,

    /// Admin address is invalid (e.g., contract address or zero address).
    ///
    /// The admin must be a valid user account address, not a contract address
    /// or any special/reserved address.
    InvalidAdmin = 35,

    /// Deposit amount is below the configured minimum deposit.
    BelowMinDeposit = 36,

    /// Market metadata URI is invalid (e.g. exceeds the maximum length).
    InvalidMetadataUri = 37,

    // ========== Authorization Errors (41-49) ==========
    /// Caller is not authorized to perform this operation.
    ///
    /// The caller must have the required role or permission.
    Unauthorized = 41,

    /// Caller is not the admin.
    ///
    /// Only the contract admin can perform administrative operations.
    NotAdmin = 42,

    /// Caller is not the market creator.
    ///
    /// Only the market creator can perform creator-restricted operations.
    NotCreator = 43,

    /// The contract has been paused by the admin.
    ///
    /// All state-changing operations are blocked while paused.
    ContractPaused = 44,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed.
    ///
    /// The token contract rejected the transfer (e.g., insufficient balance).
    TokenTransferFailed = 50,

    /// Token address is invalid.
    ///
    /// The provided token address is not a valid token contract.
    InvalidToken = 51,

    /// Token amount is invalid (e.g., zero or negative).
    ///
    /// Token amounts must be positive.
    InvalidTokenAmount = 52,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic overflow occurred during a calculation.
    ///
    /// The operation would exceed the maximum representable value. This is a
    /// fail-closed guard: rather than wrapping (which could mint or destroy
    /// value), the contract rejects the operation outright.
    ArithmeticOverflow = 60,

    /// Arithmetic underflow occurred during a calculation.
    ///
    /// The operation would go below the minimum representable value. Like
    /// [`ContractError::ArithmeticOverflow`], this is fail-closed: the
    /// operation is rejected instead of wrapping.
    ArithmeticUnderflow = 61,

    /// Division by zero was attempted.
    ///
    /// The divisor was zero. Division is rejected rather than panicking so
    /// callers receive a stable, typed error code.
    DivisionByZero = 62,

    // ========== Treasury Errors (70-79) ==========
    /// Treasury operation failed.
    ///
    /// A treasury-related operation could not be completed.
    TreasuryError = 70,

    /// Insufficient treasury balance.
    ///
    /// The treasury does not have enough funds for this operation.
    InsufficientTreasuryBalance = 71,

    // ========== Reconciliation Errors (80-89) ==========
    /// Reconciliation check failed.
    ///
    /// The on-chain state does not match the expected reconciled state.
    ReconciliationFailed = 80,

    /// Balance mismatch detected during reconciliation.
    ///
    /// The computed balance does not match the stored balance.
    BalanceMismatch = 81,

    // ========== Conservation Errors (90-99) ==========
    /// Conservation invariant violated.
    ///
    /// The total value in must equal the total value out. A violation
    /// indicates a critical bug and the operation is rejected fail-closed.
    ConservationViolation = 90,

    /// Share conservation violated.
    ///
    /// The sum of shares must be conserved across a settlement operation.
    ShareConservationViolation = 91,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arithmetic error codes are part of the public ABI and must remain
    /// stable so clients can map them deterministically. This guards against
    /// accidental renumbering when new variants are added.
    #[test]
    fn arithmetic_error_codes_are_stable() {
        assert_eq!(ContractError::ArithmeticOverflow as u32, 60);
        assert_eq!(ContractError::ArithmeticUnderflow as u32, 61);
        assert_eq!(ContractError::DivisionByZero as u32, 62);
    }

    /// The not-found code is the single stable code for market lookups and
    /// must never be renumbered or reused.
    #[test]
    fn market_not_found_code_is_stable() {
        assert_eq!(ContractError::MarketNotFound as u32, 1);
    }

    /// Error codes must be unique across the enum so a client can never
    /// confuse two distinct failure modes. This catches copy/paste mistakes
    /// when adding new variants.
    #[test]
    fn error_codes_are_unique() {
        let codes = [
            ContractError::MarketNotFound as u32,
            ContractError::MarketAlreadyResolved as u32,
            ContractError::MarketNotResolved as u32,
            ContractError::MarketExpired as u32,
            ContractError::MarketNotActive as u32,
            ContractError::MarketClosedToDeposits as u32,
            ContractError::WithdrawCooldownActive as u32,
            ContractError::MarketAlreadyClosed as u32,
            ContractError::ClaimBeforeResolve as u32,
            ContractError::InsufficientCollateral as u32,
            ContractError::PositionAlreadySettled as u32,
            ContractError::NoPositionFound as u32,
            ContractError::InvalidShareAmount as u32,
            ContractError::BatchTooLarge as u32,
            ContractError::InvalidSignature as u32,
            ContractError::UnauthorizedOracle as u32,
            ContractError::InvalidOutcome as u32,
            ContractError::OraclePriceUnavailable as u32,
            ContractError::OracleMessageExpired as u32,
            ContractError::InvalidThresholdQuorum as u32,
            ContractError::StalePrice as u32,
            ContractError::InvalidPrice as u32,
            ContractError::InvalidQuantity as u32,
            ContractError::InvalidTimestamp as u32,
            ContractError::InvalidQuestion as u32,
            ContractError::InvalidOutcomeCount as u32,
            ContractError::InvalidAdmin as u32,
            ContractError::BelowMinDeposit as u32,
            ContractError::InvalidMetadataUri as u32,
            ContractError::Unauthorized as u32,
            ContractError::NotAdmin as u32,
            ContractError::NotCreator as u32,
            ContractError::ContractPaused as u32,
            ContractError::TokenTransferFailed as u32,
            ContractError::InvalidToken as u32,
            ContractError::InvalidTokenAmount as u32,
            ContractError::ArithmeticOverflow as u32,
            ContractError::ArithmeticUnderflow as u32,
            ContractError::DivisionByZero as u32,
            ContractError::TreasuryError as u32,
            ContractError::InsufficientTreasuryBalance as u32,
            ContractError::ReconciliationFailed as u32,
            ContractError::BalanceMismatch as u32,
            ContractError::ConservationViolation as u32,
            ContractError::ShareConservationViolation as u32,
        ];

        for (i, a) in codes.iter().enumerate() {
            for b in codes.iter().skip(i + 1) {
                assert_ne!(a, b, "duplicate error code detected");
            }
        }
    }
}
