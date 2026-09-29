use soroban_sdk::contracterror;

/// Error codes for the Vatix market contract.
///
/// Errors are grouped by category with reserved number ranges:
/// - Market Errors: 1-9
/// - Position Errors: 10-19
/// - Oracle Errors: 20-29
/// - Validation Errors: 30-39
/// - Authorization Errors: 40-49
/// - Token Errors: 50-59
/// - Arithmetic Errors: 60-69
/// - Treasury Errors: 70-79
/// - Reconciliation Errors: 80-89
/// - Conservation Errors: 90-99
///
/// The full numeric code table for every contract is in
/// `docs/error-codes.md`; keep it in sync when adding variants.
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

    /// Fee rate is invalid (e.g., exceeds the configured fee cap or is out of range).
    InvalidFeeRate = 38,

    /// Fee waiver account is invalid (a contract address, or the admin itself).
    ///
    /// The admin-managed fee waiver list (#483) may only hold ordinary user
    /// accounts. Contract addresses are rejected the same way `InvalidAdmin`
    /// rejects them, and the admin's own address is rejected so the admin
    /// cannot quietly exempt itself from fees.
    InvalidFeeWaiverAccount = 39,

    // ========== Authorization Errors (40-49) ==========
    /// Caller is not authorized to perform this privileged operation.
    ///
    /// Deny-by-default: every privileged admin/guardian entrypoint requires
    /// the caller to be the current admin (or guardian where applicable).
    /// Untrusted callers fail closed with this stable code rather than a
    /// generic panic, so off-chain observers can distinguish an authz
    /// rejection from a validation failure.
    Unauthorized = 40,

    /// Caller is authenticated but lacks the required role for this operation.
    ///
    /// Distinct from [`ContractError::Unauthorized`]: the caller proved
    /// control of their address (`require_auth` passed) but is not the admin,
    /// guardian, or oracle that the entrypoint demands. Kept separate so
    /// monitoring can alert on role-escalation attempts without conflating
    /// them with unauthenticated probes.
    InsufficientRole = 41,

    /// The admin/guardian role has been renounced or is otherwise unset.
    ///
    /// Privileged entrypoints fail closed with this code when no admin is
    /// configured, rather than silently allowing the call through.
    AdminNotSet = 42,

    /// The requested privileged operation is disabled by a kill-switch.
    ///
    /// Money-path or mainnet-affecting entrypoints can be paused by the
    /// admin. Callers receive this stable code so clients can surface a
    /// "temporarily disabled" state instead of a generic failure.
    OperationDisabled = 43,

    // ========== Idempotency Errors (44-49) ==========
    /// The request was already processed (replay detected).
    ///
    /// Concurrent or replayed requests carrying the same idempotency key
    /// fail closed with this code. Callers should treat it as success of the
    /// original request rather than retrying blindly.
    DuplicateRequest = 44,

    /// The supplied idempotency key is malformed or missing.
    ///
    /// Entrypoints that require an idempotency key reject empty/oversized
    /// keys with this code so callers cannot bypass replay protection.
    InvalidIdempotencyKey = 45,

    // ========== Dependency / Fail-Closed Errors (46-49) ==========
    /// A required external dependency (RPC/DB/Redis/oracle) is unavailable.
    ///
    /// Writes fail closed with this code when a dependency outage is
    /// detected, so no partial state is committed on the money path.
    DependencyUnavailable = 46,

    /// The operation was rejected because the contract is in a fail-closed
    /// state (e.g. reconciliation mismatch or paused writes).
    ///
    /// Distinct from [`ContractError::OperationDisabled`]: this signals an
    /// automatic safety trip rather than an admin-initiated pause.
    FailClosed = 47,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed.
    ///
    /// The underlying token contract rejected the transfer (e.g. insufficient
    /// balance or allowance).
    TokenTransferFailed = 50,

    /// Token address is invalid or unsupported.
    ///
    /// The token must be a supported SEP-41 asset configured for this market.
    InvalidToken = 51,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic overflow occurred during a calculation.
    ///
    /// All money-path arithmetic uses checked operations; overflow fails
    /// closed with this code rather than wrapping.
    ArithmeticOverflow = 60,

    /// Arithmetic underflow occurred during a calculation.
    ArithmeticUnderflow = 61,

    /// Division by zero was attempted.
    DivisionByZero = 62,

    // ========== Treasury Errors (70-79) ==========
    /// Treasury balance is insufficient for the requested payout.
    InsufficientTreasury = 70,

    /// Treasury withdrawal exceeds the configured cap.
    TreasuryCapExceeded = 71,

    // ========== Reconciliation Errors (80-89) ==========
    /// On-chain balances do not reconcile with the expected ledger state.
    ReconciliationMismatch = 80,

    /// A reconciliation run is already in progress.
    ReconciliationInProgress = 81,

    // ========== Conservation Errors (90-99) ==========
    /// The conservation invariant (sum of shares == total collateral) was violated.
    ConservationViolation = 90,

    /// A conservation check could not be completed due to missing data.
    ConservationCheckIncomplete = 91,
}
