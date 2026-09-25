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

    /// The market has already been closed.
    ///
    /// `close_market` transitions a market into the terminal Closed state.
    /// Closing is idempotent-hostile by design: a second close attempt (or a
    /// replayed close request) is rejected so callers cannot re-run the
    /// money-path side effects (settlement gating, event emission) twice.
    MarketAlreadyClosed = 8,

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

    /// Fee rate is invalid (e.g. exceeds the configured fee cap or is out of range).
    InvalidFeeRate = 38,

    /// Fee waiver account is invalid (a contract address, or the admin itself).
    ///
    /// The admin-managed fee waiver list (#483) may only hold ordinary user
    /// accounts. Contract addresses are rejected the same way `InvalidAdmin`
    /// rejects them, and the admin's own address is rejected so the admin
    /// cannot quietly exempt itself from withdrawal fees it controls (#584).
    InvalidFeeWaiverAccount = 39,

    /// Treasury address is invalid (e.g. contract address or zero address).
    ///
    /// The admin-only `set_treasury` setter (#857) must receive a valid user
    /// account address. Contract addresses and reserved/zero addresses are
    /// rejected so fees cannot be routed to an address the admin does not
    /// actually control.
    InvalidTreasury = 40,

    // ========== Authorization Errors (41-49) ==========
    /// Caller is not authorized to perform this operation.
    ///
    /// The caller must be the admin or an address explicitly granted the
    /// required role. Deny-by-default: any privileged entrypoint rejects
    /// callers that do not hold the required authorization.
    Unauthorized = 41,

    /// The caller is not the contract admin.
    ///
    /// Admin-only entrypoints (configuration, treasury, fee waivers, pause)
    /// reject any caller other than the stored admin address.
    NotAdmin = 42,

    /// The contract is paused and money-path operations are disabled.
    ///
    /// The admin kill-switch halts deposits, trades, and settlement until the
    /// contract is unpaused. Reads remain available.
    ContractPaused = 43,

    // ========== Token Errors (50-59) ==========
    /// Token transfer failed.
    ///
    /// The underlying token contract rejected the transfer (insufficient
    /// balance, frozen account, or a failing token implementation).
    TokenTransferFailed = 50,

    /// Token address is invalid or unsupported.
    ///
    /// The configured collateral token must be a valid token contract.
    InvalidToken = 51,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic overflow occurred during a calculation.
    ///
    /// Inputs were too large for the intermediate result to fit in the
    /// target integer type. Callers should reduce magnitudes or split work.
    ArithmeticOverflow = 60,

    /// Arithmetic underflow occurred during a calculation.
    ///
    /// A subtraction would have produced a negative value where only
    /// non-negative results are valid.
    ArithmeticUnderflow = 61,

    /// Division by zero was attempted.
    ///
    /// The divisor evaluated to zero; callers must guard against zero
    /// denominators before performing the division.
    DivisionByZero = 62,

    // ========== Treasury Errors (70-79) ==========
    /// Treasury withdrawal failed.
    ///
    /// The treasury could not release the requested amount (insufficient
    /// balance or a failing token transfer).
    TreasuryWithdrawFailed = 70,

    /// The requested treasury amount exceeds the available balance.
    ///
    /// Withdrawals are capped at the currently accrued treasury balance.
    InsufficientTreasuryBalance = 71,

    // ========== Reconciliation Errors (80-89) ==========
    /// The reconciliation request was rejected because the caller is not
    /// authorized to run reconciliation for this market.
    ///
    /// Reconciliation is a privileged, deny-by-default surface: only the
    /// admin (or an explicitly authorized reconciler) may invoke it. Untrusted
    /// clients cannot bypass this policy.
    ReconciliationUnauthorized = 80,

    /// The reconciliation request was replayed or is already in progress.
    ///
    /// Reconciliation entrypoints are idempotent: a request carrying a
    /// correlation id that has already been processed (or is currently being
    /// processed) is rejected so money-path side effects cannot run twice.
    ReconciliationAlreadyProcessed = 81,

    /// The reconciliation inputs are inconsistent with on-chain state.
    ///
    /// The supplied balances/positions do not match the contract's recorded
    /// state, so the reconciliation is refused rather than silently applied.
    /// The contract remains the source of truth for balances and swaps.
    ReconciliationMismatch = 82,

    /// Reconciliation could not complete because a required dependency
    /// (RPC/DB/Redis or oracle feed) is unavailable.
    ///
    /// Writes fail closed: no partial reconciliation is persisted when a
    /// dependency is down.
    ReconciliationDependencyUnavailable = 83,

    /// The reconciliation batch was empty or exceeded the configured maximum
    /// size, and was rejected to surface caller bugs and prevent griefing.
    ReconciliationBatchInvalid = 84,
}
