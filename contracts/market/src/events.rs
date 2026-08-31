//! Event emission functions for the Vatix prediction market contract
//!
//! # Index-friendly topic naming (Issue #389)
//!
//! Event struct names follow the `{Noun}{Verb}` PascalCase pattern without a
//! redundant `Event` suffix. Soroban converts them to snake_case topics
//! automatically, producing clean, indexer-friendly topic strings.
//!
//! | Struct                   | Topic symbol                        |
//! |--------------------------|-------------------------------------|
//! | `ContractInitialized`    | `contract_initialized`              |
//! | `MarketCreated`          | `market_created`                    |
//! | `CollateralDeposited`    | `collateral_deposited`              |
//! | `CollateralWithdrawn`    | `collateral_withdrawn`              |
//! | `LargeWithdraw`          | `large_withdraw`                    |
//! | `WithdrawEdgeCase`       | `withdraw_edge_case`                |
//! | `MarketResolved`         | `market_resolved`                   |
//! | `MarketCanceled`         | `market_canceled`                   |
//! | `MarketVoided`           | `market_voided`                     |
//! | `PositionSettled`        | `position_settled`                  |
//! | `PositionUpdated`        | `position_updated`                  |
//! | `PositionLimitExceeded`  | `position_limit_exceeded`           |
//! | `OracleSignatureVerified`| `oracle_signature_verified`         |
//! | `FeeCalculated`          | `fee_calculated`                    |
//! | `TreasurySet`            | `treasury_set`                      |
//! | `AdminTransferProposed`  | `admin_transfer_proposed`           |
//! | `AdminTransferAccepted`  | `admin_transfer_accepted`           |
//! | `AdminTransferCanceled`  | `admin_transfer_canceled`           |
//! | `PositionTokenMismatchDetected` | `position_token_mismatch_detected` |
//! | `PositionTokensReconciled`      | `position_tokens_reconciled`       |
//!
//! # Event schema versioning (Issue #500)
//!
//! Every event carries [`EVENT_VERSION`] as its first explicit topic (right
//! after the auto-generated event-name topic), so off-chain indexers can
//! distinguish payload schema versions across future contract upgrades
//! without needing to parse the data payload first.

use soroban_sdk::{contractevent, Address, BytesN, Env, String, Vec};

/// Schema version stamped into every event's topics. Bump when an event's
/// topic/data shape changes in a way consumers need to distinguish.
pub const EVENT_VERSION: u32 = 1;

#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractInitialized {
    #[topic]
    pub version: u32,
    #[topic]
    pub admin: Address,
    /// Ledger timestamp when the contract was bootstrapped.
    pub initialized_at: u64,
}

/// Emit an event when the contract is initialized with an admin.
///
/// Publishes a [`ContractInitialized`] to the Soroban event stream when
/// `initialize` is called for the first time. Indexed by `admin` as a topic
/// so off-chain indexers can confirm who bootstrapped the contract.
///
/// # Arguments
/// * `env` - Contract environment
/// * `admin` - The address stored as the contract admin
pub fn emit_contract_initialized(env: &Env, admin: &Address) {
    ContractInitialized {
        version: EVENT_VERSION,
        admin: admin.clone(),
        initialized_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Event emitted when the contract is paused or unpaused for emergency maintenance.
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPauseToggled {
    #[topic]
    pub version: u32,
    #[topic]
    pub paused: bool,
    /// Ledger timestamp when the pause state was changed.
    pub timestamp: u64,
}

/// Emit event when the emergency pause flag is toggled.
pub fn emit_emergency_pause_toggled(env: &Env, paused: bool) {
    EmergencyPauseToggled {
        version: EVENT_VERSION,
        paused,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Event emitted when the coordinated emergency mode is changed (Issue #662).
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyModeChanged {
    #[topic]
    pub version: u32,
    #[topic]
    pub new_mode: crate::types::EmergencyMode,
    pub admin: Address,
    pub changed_at: u64,
}

/// Emit event when the emergency mode changes.
pub fn emit_emergency_mode_changed(
    env: &Env,
    new_mode: &crate::types::EmergencyMode,
    admin: &Address,
) {
    EmergencyModeChanged {
        version: EVENT_VERSION,
        new_mode: new_mode.clone(),
        admin: admin.clone(),
        changed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCreated {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub creator: Address,
    pub question: String,
    pub end_time: u64,
    pub metadata_uri: Option<String>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralDeposited {
    #[topic]
    pub version: u32,
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralWithdrawn {
    #[topic]
    pub version: u32,
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct LargeWithdraw {
    #[topic]
    pub version: u32,
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawEdgeCase {
    #[topic]
    pub version: u32,
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
}

/// Emit event when collateral is deposited
///
/// Publishes a [`CollateralDeposited`] to the Soroban event stream.
/// This event is indexed by `user` and `market_id` as topics, allowing
/// off-chain indexers to efficiently query deposits by user or market.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount deposited in stroops
/// * new_total - User's total collateral in this market after deposit
///
/// # Example
/// ```ignore
/// emit_collateral_deposited(&env, &user, 1, 5_000_000, 5_000_000);
/// ```
pub fn emit_collateral_deposited(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralDeposited {
        version: EVENT_VERSION,
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Emit event when collateral is withdrawn
///
/// Publishes a [`CollateralWithdrawn`] to the Soroban event stream.
/// This event is indexed by `user` and `market_id` as topics for efficient
/// querying by off-chain services.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount withdrawn in stroops
/// * new_total - User's total collateral in this market after withdrawal
///
/// # Example
/// ```ignore
/// emit_collateral_withdrawn(&env, &user, 1, 2_000_000, 3_000_000);
/// ```
pub fn emit_collateral_withdrawn(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralWithdrawn {
        version: EVENT_VERSION,
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Publishes a [`LargeWithdraw`] audit event when a withdrawal reaches the
/// large-withdraw threshold (#502), so operators/indexers can flag unusual
/// outflows without parsing every `CollateralWithdrawn` event.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount withdrawn in stroops
/// * timestamp - Ledger timestamp of the withdrawal
pub fn emit_large_withdraw(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    timestamp: u64,
) {
    LargeWithdraw {
        version: EVENT_VERSION,
        user: user.clone(),
        market_id,
        amount,
        timestamp,
    }
    .publish(env);
}

/// Emit a MarketCreated event
///
/// Publishes a [`MarketCreated`] to the Soroban event stream when a new
/// prediction market is initialized. The event is indexed by `market_id` as
/// a topic for efficient lookup by off-chain indexers and frontends.
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the created market
/// * creator - Address that created the market
/// * question - The market question
/// * end_time - Unix timestamp when market closes for trading
///
/// # Example
/// ```ignore
/// emit_market_created(&env, 1, &creator, &String::from_str(&env, "Will BTC hit $100k?"), 1735689600);
/// ```
pub fn emit_market_created(
    env: &Env,
    market_id: u32,
    creator: &Address,
    question: &String,
    end_time: u64,
    metadata_uri: &Option<String>,
) {
    // Publish the event with topics and data
    MarketCreated {
        version: EVENT_VERSION,
        market_id,
        creator: creator.clone(),
        question: question.clone(),
        end_time,
        metadata_uri: metadata_uri.clone(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketClosedToDeposits {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub admin: Address,
    pub closed_at: u64,
}

/// Emit event when a market is closed to new deposits
///
/// Publishes a [`MarketClosedToDeposits`] to the Soroban event stream when
/// an admin closes a market to prevent new collateral deposits. The event is indexed
/// by `market_id` as a topic for efficient lookup by off-chain indexers.
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the market being closed
/// * admin - Address of the admin who closed the market
/// * closed_at - Unix timestamp when the market was closed
///
/// # Example
/// ```ignore
/// emit_market_closed_to_deposits(&env, 1, &admin, env.ledger().timestamp());
/// ```
pub fn emit_market_closed_to_deposits(env: &Env, market_id: u32, admin: &Address, closed_at: u64) {
    MarketClosedToDeposits {
        version: EVENT_VERSION,
        market_id,
        admin: admin.clone(),
        closed_at,
    }
    .publish(env);
}

/// Emit event for withdraw edge case when user has zero collateral deposited
///
/// Publishes a [`WithdrawEdgeCase`] when a user attempts to withdraw
/// from a market where they have no deposited collateral. This helps off-chain
/// monitoring tools identify potential UI bugs or user confusion.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount attempted to withdraw in stroops
///
/// # Example
/// ```ignore
/// emit_withdraw_edge_case(&env, &user, 1, 1_000_000);
/// ```
pub fn emit_withdraw_edge_case(env: &Env, user: &Address, market_id: u32, amount: i128) {
    WithdrawEdgeCase {
        version: EVENT_VERSION,
        user: user.clone(),
        market_id,
        amount,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketResolved {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub oracle_pubkey: BytesN<32>,
    pub resolver: Address,
    pub outcome: bool,
    pub resolved_at: u64,
}

/// Emit a MarketResolved event
///
/// Publishes a [`MarketResolved`] to the Soroban event stream when a
/// market is resolved by an oracle. The event is indexed by `market_id` as
/// a topic, allowing efficient queries for resolution events.
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the resolved market
/// * oracle_pubkey - Oracle public key used to verify the resolution signature
/// * resolver - Address of the resolver who submitted the resolution
/// * outcome - Market outcome (true = YES won, false = NO won)
/// * resolved_at - Unix timestamp when market was resolved
///
/// # Example
/// ```ignore
/// emit_market_resolved(&env, 1, &oracle_pubkey, &resolver_address, true, env.ledger().timestamp());
/// ```
pub fn emit_market_resolved(
    env: &Env,
    market_id: u32,
    oracle_pubkey: &BytesN<32>,
    resolver: &Address,
    outcome: bool,
    resolved_at: u64,
) {
    MarketResolved {
        version: EVENT_VERSION,
        market_id,
        oracle_pubkey: oracle_pubkey.clone(),
        resolver: resolver.clone(),
        outcome,
        resolved_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCanceled {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub canceler: Address,
    pub canceled_at: u64,
}

/// Emit a MarketCanceled event
///
/// Publishes a [`MarketCanceled`] to the Soroban event stream when an
/// admin halts a market before resolution. The event is indexed by `market_id`
/// as a topic so off-chain indexers can detect canceled markets and surface
/// collateral-reclaim flows to affected users.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the canceled market
/// * `canceler` - Admin address that canceled the market
/// * `canceled_at` - Unix timestamp (ledger time) when the cancellation occurred
///
/// # Example
/// ```ignore
/// emit_market_canceled(&env, 1, &admin, env.ledger().timestamp());
/// ```
pub fn emit_market_canceled(env: &Env, market_id: u32, canceler: &Address, canceled_at: u64) {
    MarketCanceled {
        version: EVENT_VERSION,
        market_id,
        canceler: canceler.clone(),
        canceled_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketVoided {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    /// The registered resolution contract address that voided the market.
    pub voided_by: Address,
    pub voided_at: u64,
}

/// Emit a `MarketVoided` event.
///
/// Published when the registered resolution contract voids a market via
/// [`crate::MarketContract::void_market`] (Issue #708) — the dispute path
/// where neither side could be safely vindicated on-chain, so the market is
/// force-transitioned to [`crate::types::MarketStatus::Canceled`] and users
/// reclaim collateral through `withdraw_canceled_collateral`.
///
/// Distinct from [`MarketCanceled`] (admin-initiated `cancel_market`) so
/// indexers can tell a governance cancellation apart from a resolution-driven
/// void, even though both land the market in the same `Canceled` state.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the voided market
/// * `voided_by` - The registered resolution contract address
/// * `voided_at` - Unix timestamp (ledger time) when the void occurred
pub fn emit_market_voided(env: &Env, market_id: u32, voided_by: &Address, voided_at: u64) {
    MarketVoided {
        version: EVENT_VERSION,
        market_id,
        voided_by: voided_by.clone(),
        voided_at,
    }
    .publish(env);
}

/// Event emitted when an admin explicitly reopens a previously canceled market.
///
/// A `Canceled` → `Active` transition is only valid through this explicit
/// admin-gated path. Any other route that would silently restore `Active`
/// status is rejected by [`crate::validation::validate_reopenable`].
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketReopened {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub admin: Address,
    pub reopened_at: u64,
}

/// Emit a MarketReopened event when the admin explicitly restores a canceled
/// market to Active status via the guarded `reopen_market` entry point.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the reopened market
/// * `admin` - Admin address that authorized the reopen
/// * `reopened_at` - Unix timestamp (ledger time) of the reopen
pub fn emit_market_reopened(env: &Env, market_id: u32, admin: &Address, reopened_at: u64) {
    MarketReopened {
        version: EVENT_VERSION,
        market_id,
        admin: admin.clone(),
        reopened_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PositionLimitExceeded {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    /// The share side that would go negative: true = YES, false = NO
    pub side_yes: bool,
}

/// Emit an event when a position change is rejected because it would push a
/// share balance below zero.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier where the limit was hit
/// * `user` - Address of the user whose position change was rejected
/// * `side_yes` - `true` if the YES share balance would go negative; `false`
///   if the NO side would go negative
///
/// # Example
/// ```ignore
/// emit_position_limit_exceeded(&env, market_id, &user, true);
/// ```
pub fn emit_position_limit_exceeded(env: &Env, market_id: u32, user: &Address, side_yes: bool) {
    PositionLimitExceeded {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        side_yes,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionUpdated {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    pub locked_collateral: i128,
}

/// Emit an event whenever a user's position is modified (shares bought or sold).
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user whose position was updated
/// * `yes_shares` - New total YES share balance after the update
/// * `no_shares` - New total NO share balance after the update
/// * `locked_collateral` - Collateral (in stroops) now locked to cover the
///   net position
///
/// # Example
/// ```ignore
/// emit_position_updated(&env, market_id, &user, 100, 0, 100);
/// ```
#[allow(dead_code)]
pub fn emit_position_updated(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128,
) {
    PositionUpdated {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        yes_shares,
        no_shares,
        locked_collateral,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TradeExecuted {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub quantity: i128,
    pub price_bps: i128,
    pub side_yes: bool,
    pub executed_at: u64,
}

/// Emit an event when a trade is executed (shares bought or sold).
///
/// Publishes a [`TradeExecuted`] to the Soroban event stream indexed by
/// `market_id` and `user` to allow efficient off-chain indexing of trades
/// by market or trader.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user executing the trade
/// * `quantity` - Number of shares traded (always positive)
/// * `price_bps` - Market price in basis points (0–10_000)
/// * `side_yes` - `true` for YES side, `false` for NO side
/// * `executed_at` - Unix timestamp when the trade was executed
///
/// # Example
/// ```ignore
/// emit_trade_executed(&env, 1, &user, 100, 5_000, true, env.ledger().timestamp());
/// ```
pub fn emit_trade_executed(
    env: &Env,
    market_id: u32,
    user: &Address,
    quantity: i128,
    price_bps: i128,
    side_yes: bool,
    executed_at: u64,
) {
    TradeExecuted {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        quantity,
        price_bps,
        side_yes,
        executed_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ValidationFailed {
    #[topic]
    pub version: u32,
    #[topic]
    pub context: soroban_sdk::Symbol,
    pub error_code: u32,
}

/// Emit an event when a validation step fails, recording which context triggered
/// the failure and the associated error code.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `context` - Symbol identifying the validation site (e.g.
///   `Symbol::new(&env, "validate_collateral")`)
/// * `error_code` - Numeric value of the [`ContractError`] variant that was
///   returned (e.g. `31` for `InvalidQuantity`)
///
/// # Example
/// ```ignore
/// emit_validation_failed(
///     &env,
///     Symbol::new(&env, "validate_collateral"),
///     ContractError::InvalidQuantity as u32,
/// );
/// ```
#[allow(dead_code)]
pub fn emit_validation_failed(env: &Env, context: soroban_sdk::Symbol, error_code: u32) {
    ValidationFailed {
        version: EVENT_VERSION,
        context,
        error_code,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionSettled {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub payout: i128,
    pub settled_at: u64,
}

/// Emit an event when a user's position is settled and payout is transferred.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user receiving the payout
/// * `payout` - Amount transferred to the user in stroops
/// * `settled_at` - Unix timestamp (ledger time) when settlement occurred
///
/// # Example
/// ```ignore
/// emit_position_settled(&env, market_id, &user, 500_000, env.ledger().timestamp());
/// ```
#[allow(dead_code)]
pub fn emit_position_settled(
    env: &Env,
    market_id: u32,
    user: &Address,
    payout: i128,
    settled_at: u64,
) {
    PositionSettled {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        payout,
        settled_at,
    }
    .publish(env);
}

/// A single aggregated event for a batch of settled positions (Issue #499),
/// emitted once by `batch_settle_positions` in place of one
/// `PositionSettled`/`PositionUpdated` pair per user. Event emission cost
/// scales with call count as well as payload size, so collapsing an
/// N-position settlement batch into one event materially cuts the
/// per-position overhead versus the naive per-user emission loop.
#[contractevent]
#[derive(Clone, Debug)]
pub struct PositionsBatchSettled {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub users: Vec<Address>,
    pub payouts: Vec<i128>,
    pub settled_at: u64,
}

/// Emit one aggregated event for a batch of settled positions.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `users` - Addresses whose positions were settled in this batch
/// * `payouts` - Payout amounts, aligned by index with `users`
/// * `settled_at` - Unix timestamp (ledger time) when the batch was settled
pub fn emit_positions_batch_settled(
    env: &Env,
    market_id: u32,
    users: &Vec<Address>,
    payouts: &Vec<i128>,
    settled_at: u64,
) {
    PositionsBatchSettled {
        version: EVENT_VERSION,
        market_id,
        users: users.clone(),
        payouts: payouts.clone(),
        settled_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct OracleSignatureVerified {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub verified_at: u64,
}

/// Emit event when oracle signature is verified
///
/// Publishes an [`OracleSignatureVerified`] when an oracle's Ed25519
/// signature is successfully verified during market resolution. This event
/// provides an audit trail for resolution authenticity and is indexed by
/// `market_id` for efficient querying.
///
/// # Arguments
/// * env - Soroban environment
/// * market_id - Market identifier
/// * outcome - Verified outcome (true = YES, false = NO)
/// * verified_at - Unix timestamp when verification occurred
///
/// # Example
/// ```ignore
/// emit_oracle_signature_verified(&env, 1, true, env.ledger().timestamp());
/// ```
pub fn emit_oracle_signature_verified(env: &Env, market_id: u32, outcome: bool, verified_at: u64) {
    OracleSignatureVerified {
        version: EVENT_VERSION,
        market_id,
        outcome,
        verified_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCalculated {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub fee_amount: i128,
    pub available_after_fee: i128,
}

/// Emit event when a fee is calculated during a withdrawal action.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user performing the withdrawal
/// * `fee_amount` - Fee deducted in stroops
/// * `available_after_fee` - Collateral available to withdraw after fee
pub fn emit_fee_calculated(
    env: &Env,
    market_id: u32,
    user: &Address,
    fee_amount: i128,
    available_after_fee: i128,
) {
    FeeCalculated {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        fee_amount,
        available_after_fee,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeRetainedNoTreasury {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub fee_amount: i128,
}

/// Emit event when a non-zero withdrawal fee is retained in the market
/// contract's own balance because no treasury address is registered (#treasury-optional).
///
/// This makes the "treasury unset" code path observable on-chain: the fee is
/// never dropped or burned, it simply stays in the contract's collateral
/// token balance until an admin registers a treasury and/or sweeps it later.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user whose withdrawal generated the fee
/// * `fee_amount` - Fee amount retained in the contract, in stroops
pub fn emit_fee_retained_no_treasury(env: &Env, market_id: u32, user: &Address, fee_amount: i128) {
    FeeRetainedNoTreasury {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        fee_amount,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposed {
    #[topic]
    pub version: u32,
    #[topic]
    pub current_admin: Address,
    #[topic]
    pub pending_admin: Address,
    pub proposed_at: u64,
}

pub fn emit_admin_transfer_proposed(env: &Env, current_admin: &Address, pending_admin: &Address) {
    AdminTransferProposed {
        version: EVENT_VERSION,
        current_admin: current_admin.clone(),
        pending_admin: pending_admin.clone(),
        proposed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferAccepted {
    #[topic]
    pub version: u32,
    #[topic]
    pub old_admin: Address,
    #[topic]
    pub new_admin: Address,
    pub accepted_at: u64,
}

pub fn emit_admin_transfer_accepted(env: &Env, old_admin: &Address, new_admin: &Address) {
    AdminTransferAccepted {
        version: EVENT_VERSION,
        old_admin: old_admin.clone(),
        new_admin: new_admin.clone(),
        accepted_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferCanceled {
    #[topic]
    pub version: u32,
    #[topic]
    pub current_admin: Address,
    #[topic]
    pub canceled_pending_admin: Address,
    pub canceled_at: u64,
}

pub fn emit_admin_transfer_canceled(
    env: &Env,
    current_admin: &Address,
    canceled_pending_admin: &Address,
) {
    AdminTransferCanceled {
        version: EVENT_VERSION,
        current_admin: current_admin.clone(),
        canceled_pending_admin: canceled_pending_admin.clone(),
        canceled_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketOracleUpdated {
    #[topic]
    pub market_id: u32,
    pub admin: Address,
    pub old_oracle_pubkey: BytesN<32>,
    pub new_oracle_pubkey: BytesN<32>,
    pub updated_at: u64,
}

/// Emit an event when a market's oracle public key is rotated by the admin (#486).
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market whose oracle key was rotated
/// * `admin` - Admin address that authorized the rotation
/// * `old_oracle_pubkey` - Previous oracle public key
/// * `new_oracle_pubkey` - Newly configured oracle public key
/// * `updated_at` - Ledger timestamp of the rotation
pub fn emit_market_oracle_updated(
    env: &Env,
    market_id: u32,
    admin: &Address,
    old_oracle_pubkey: &BytesN<32>,
    new_oracle_pubkey: &BytesN<32>,
    updated_at: u64,
) {
    MarketOracleUpdated {
        market_id,
        admin: admin.clone(),
        old_oracle_pubkey: old_oracle_pubkey.clone(),
        new_oracle_pubkey: new_oracle_pubkey.clone(),
        updated_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasurySet {
    #[topic]
    pub version: u32,
    #[topic]
    pub treasury: Address,
    pub set_at: u64,
}

pub fn emit_treasury_set(env: &Env, treasury: &Address) {
    TreasurySet {
        version: EVENT_VERSION,
        treasury: treasury.clone(),
        set_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin proposes a new withdrawal fee rate (Issue #496).
/// The change does not take effect until `effective_at` and
/// `execute_fee_rate_change` is called.
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeRateChangeProposed {
    #[topic]
    pub version: u32,
    pub new_rate_bps: i128,
    pub effective_at: u64,
}

pub fn emit_fee_rate_change_proposed(env: &Env, new_rate_bps: i128, effective_at: u64) {
    FeeRateChangeProposed {
        version: EVENT_VERSION,
        new_rate_bps,
        effective_at,
    }
    .publish(env);
}

/// Emitted once a previously-proposed fee rate change actually takes effect.
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeRateChangeExecuted {
    #[topic]
    pub version: u32,
    pub new_rate_bps: i128,
    pub executed_at: u64,
}

pub fn emit_fee_rate_change_executed(env: &Env, new_rate_bps: i128, executed_at: u64) {
    FeeRateChangeExecuted {
        version: EVENT_VERSION,
        new_rate_bps,
        executed_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminRenounceProposed {
    #[topic]
    pub version: u32,
    #[topic]
    pub admin: Address,
    pub proposed_at: u64,
}

pub fn emit_admin_renounce_proposed(env: &Env, admin: &Address) {
    AdminRenounceProposed {
        version: EVENT_VERSION,
        admin: admin.clone(),
        proposed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminRenounced {
    #[topic]
    pub version: u32,
    #[topic]
    pub former_admin: Address,
    pub renounced_at: u64,
}

pub fn emit_admin_renounced(env: &Env, admin: &Address) {
    AdminRenounced {
        version: EVENT_VERSION,
        former_admin: admin.clone(),
        renounced_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Oracle adapter enable/disable (#488) ──────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct OracleAdapterConfigured {
    #[topic]
    pub adapter_type: crate::types::AdapterType,
    pub enabled: bool,
    pub configured_at: u64,
}

/// Emitted when the admin enables/disables the Reflector or Pyth adapter via
/// `set_adapter_enabled`. While an adapter is disabled, resolution falls back
/// to direct Ed25519 verification — see `oracle::verify_market_outcome`.
pub fn emit_oracle_adapter_configured(
    env: &Env,
    adapter_type: crate::types::AdapterType,
    enabled: bool,
) {
    OracleAdapterConfigured {
        adapter_type,
        enabled,
        configured_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Fee waiver list (#483) ────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeWaiverAdded {
    #[topic]
    pub account: Address,
    pub admin: Address,
    pub added_at: u64,
}

/// Emit an event when the admin adds an address to the fee waiver list.
pub fn emit_fee_waiver_added(env: &Env, account: &Address, admin: &Address) {
    FeeWaiverAdded {
        account: account.clone(),
        admin: admin.clone(),
        added_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeWaiverRemoved {
    #[topic]
    pub account: Address,
    pub admin: Address,
    pub removed_at: u64,
}

/// Emit an event when the admin removes an address from the fee waiver list.
pub fn emit_fee_waiver_removed(env: &Env, account: &Address, admin: &Address) {
    FeeWaiverRemoved {
        account: account.clone(),
        admin: admin.clone(),
        removed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryProposed {
    #[topic]
    pub version: u32,
    pub treasury: Address,
    pub effective_at: u64,
}

pub fn emit_treasury_proposed(env: &Env, treasury: &Address, effective_at: u64) {
    TreasuryProposed {
        version: EVENT_VERSION,
        treasury: treasury.clone(),
        effective_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct OutcomeTokenProposed {
    #[topic]
    pub version: u32,
    pub outcome_token: Address,
    pub effective_at: u64,
}

pub fn emit_outcome_token_proposed(env: &Env, outcome_token: &Address, effective_at: u64) {
    OutcomeTokenProposed {
        version: EVENT_VERSION,
        outcome_token: outcome_token.clone(),
        effective_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct OutcomeTokenSet {
    #[topic]
    pub version: u32,
    pub outcome_token: Address,
    pub set_at: u64,
}

pub fn emit_outcome_token_set(env: &Env, outcome_token: &Address) {
    OutcomeTokenSet {
        version: EVENT_VERSION,
        outcome_token: outcome_token.clone(),
        set_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ResolutionProposed {
    #[topic]
    pub version: u32,
    pub resolution: Address,
    pub effective_at: u64,
}

pub fn emit_resolution_proposed(env: &Env, resolution: &Address, effective_at: u64) {
    ResolutionProposed {
        version: EVENT_VERSION,
        resolution: resolution.clone(),
        effective_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ResolutionSet {
    #[topic]
    pub version: u32,
    pub resolution: Address,
    pub set_at: u64,
}

pub fn emit_resolution_set(env: &Env, resolution: &Address) {
    ResolutionSet {
        version: EVENT_VERSION,
        resolution: resolution.clone(),
        set_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketOracleProposed {
    #[topic]
    pub market_id: u32,
    pub admin: Address,
    pub old_oracle_pubkey: BytesN<32>,
    pub new_oracle_pubkey: BytesN<32>,
    pub effective_at: u64,
}

pub fn emit_market_oracle_proposed(
    env: &Env,
    market_id: u32,
    admin: &Address,
    old_oracle_pubkey: &BytesN<32>,
    new_oracle_pubkey: &BytesN<32>,
    effective_at: u64,
) {
    MarketOracleProposed {
        market_id,
        admin: admin.clone(),
        old_oracle_pubkey: old_oracle_pubkey.clone(),
        new_oracle_pubkey: new_oracle_pubkey.clone(),
        effective_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PositionTokenMismatchDetected {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    pub yes_token_balance: i128,
    pub no_token_balance: i128,
}

pub fn emit_position_token_mismatch_detected(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_shares: i128,
    no_shares: i128,
    yes_token_balance: i128,
    no_token_balance: i128,
) {
    PositionTokenMismatchDetected {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        yes_shares,
        no_shares,
        yes_token_balance,
        no_token_balance,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PositionTokensReconciled {
    #[topic]
    pub version: u32,
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub admin: Address,
    pub yes_delta_applied: i128,
    pub no_delta_applied: i128,
    pub reconciled_at: u64,
}

pub fn emit_position_tokens_reconciled(
    env: &Env,
    market_id: u32,
    user: &Address,
    admin: &Address,
    yes_delta_applied: i128,
    no_delta_applied: i128,
) {
    PositionTokensReconciled {
        version: EVENT_VERSION,
        market_id,
        user: user.clone(),
        admin: admin.clone(),
        yes_delta_applied,
        no_delta_applied,
        reconciled_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarketContract;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Env, IntoVal, Map, String, Symbol, TryIntoVal, Val,
    };

    #[test]
    fn test_emit_contract_initialized() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            emit_contract_initialized(&env, &admin);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "contract_initialized"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, admin);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let initialized_at_val: u64 = data
            .get(Symbol::new(&env, "initialized_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(initialized_at_val, env.ledger().timestamp());
    }

    #[test]
    fn test_emit_market_created() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let creator = Address::generate(&env);
        let question = String::from_str(&env, "Will BTC hit $100k?");
        let end_time = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_created(&env, market_id, &creator, &question, end_time, &None);
        });

        // Verify event was published
        let events = env.events().all();
        assert_eq!(events.len(), 1);

        // Verify event content
        let event = events.first().unwrap();
        // event is (Contract Address, Topics, Data)

        // Topics
        let topics = &event.1;
        assert_eq!(topics.len(), 3);

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_created"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        // Data
        // Data
        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let creator_val: Address = data
            .get(Symbol::new(&env, "creator"))
            .unwrap()
            .into_val(&env);
        let question_val: String = data
            .get(Symbol::new(&env, "question"))
            .unwrap()
            .into_val(&env);
        let end_time_val: u64 = data
            .get(Symbol::new(&env, "end_time"))
            .unwrap()
            .into_val(&env);
        assert_eq!(creator_val, creator);
        assert_eq!(question_val, question);
        assert_eq!(end_time_val, end_time);
    }

    #[test]
    fn test_emit_market_resolved() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let resolver = Address::generate(&env);
        let outcome = true;
        let resolved_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_resolved(
                &env,
                market_id,
                &oracle_pubkey,
                &resolver,
                outcome,
                resolved_at,
            );
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_resolved"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let resolver_val: BytesN<32> = data
            .get(Symbol::new(&env, "resolver"))
            .unwrap()
            .into_val(&env);
        let outcome_val: bool = data
            .get(Symbol::new(&env, "outcome"))
            .unwrap()
            .into_val(&env);
        let resolved_at_val: u64 = data
            .get(Symbol::new(&env, "resolved_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(resolver_val, resolver);
        assert_eq!(outcome_val, outcome);
        assert_eq!(resolved_at_val, resolved_at);
    }

    #[test]
    fn test_emit_market_canceled() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let canceler = Address::generate(&env);
        let canceled_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_canceled(&env, market_id, &canceler, canceled_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_canceled"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let canceler_val: Address = data
            .get(Symbol::new(&env, "canceler"))
            .unwrap()
            .into_val(&env);
        let canceled_at_val: u64 = data
            .get(Symbol::new(&env, "canceled_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(canceler_val, canceler);
        assert_eq!(canceled_at_val, canceled_at);
    }

    #[test]
    fn test_emit_market_voided() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 7u32;
        let voided_by = Address::generate(&env);
        let voided_at = 1_700_000_000u64;

        env.as_contract(&contract_id, || {
            emit_market_voided(&env, market_id, &voided_by, voided_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_voided"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let voided_by_val: Address = data
            .get(Symbol::new(&env, "voided_by"))
            .unwrap()
            .into_val(&env);
        let voided_at_val: u64 = data
            .get(Symbol::new(&env, "voided_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(voided_by_val, voided_by);
        assert_eq!(voided_at_val, voided_at);
    }

    #[test]
    fn test_emit_position_updated() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let yes_shares = 100i128;
        let no_shares = 50i128;
        let locked_collateral = 150i128;

        env.as_contract(&contract_id, || {
            emit_position_updated(
                &env,
                market_id,
                &user,
                yes_shares,
                no_shares,
                locked_collateral,
            );
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_updated"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let topic3: Address = topics.get(3).unwrap().into_val(&env);
        assert_eq!(topic3, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let yes_shares_val: i128 = data
            .get(Symbol::new(&env, "yes_shares"))
            .unwrap()
            .into_val(&env);
        let no_shares_val: i128 = data
            .get(Symbol::new(&env, "no_shares"))
            .unwrap()
            .into_val(&env);
        let locked_collateral_val: i128 = data
            .get(Symbol::new(&env, "locked_collateral"))
            .unwrap()
            .into_val(&env);

        assert_eq!(yes_shares_val, yes_shares);
        assert_eq!(no_shares_val, no_shares);
        assert_eq!(locked_collateral_val, locked_collateral);
    }

    #[test]
    fn test_emit_position_settled() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let payout = 100i128;
        let settled_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_position_settled(&env, market_id, &user, payout, settled_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_settled"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let topic3: Address = topics.get(3).unwrap().into_val(&env);
        assert_eq!(topic3, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let payout_val: i128 = data
            .get(Symbol::new(&env, "payout"))
            .unwrap()
            .into_val(&env);
        assert_eq!(payout_val, payout);
    }

    #[test]
    fn test_emit_collateral_deposited() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 1000i128;
        let new_total = 1000i128;

        env.as_contract(&contract_id, || {
            emit_collateral_deposited(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_deposited"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let topic3: u32 = topics.get(3).unwrap().into_val(&env);
        assert_eq!(topic3, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }

    #[test]
    fn test_emit_validation_failed() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let context = Symbol::new(&env, "validate_collateral");
        let error_code = 31u32;

        env.as_contract(&contract_id, || {
            emit_validation_failed(&env, context.clone(), error_code);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "validation_failed"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: Symbol = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, context);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let code: u32 = data
            .get(Symbol::new(&env, "error_code"))
            .unwrap()
            .into_val(&env);
        assert_eq!(code, error_code);
    }

    #[test]
    fn test_emit_collateral_withdrawn() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 500i128;
        let new_total = 500i128;

        env.as_contract(&contract_id, || {
            emit_collateral_withdrawn(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_withdrawn"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let topic3: u32 = topics.get(3).unwrap().into_val(&env);
        assert_eq!(topic3, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }

    #[test]
    fn test_emit_oracle_signature_verified() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let outcome = true;
        let verified_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_oracle_signature_verified(&env, market_id, outcome, verified_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "oracle_signature_verified"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let outcome_val: bool = data
            .get(Symbol::new(&env, "outcome"))
            .unwrap()
            .into_val(&env);
        let verified_at_val: u64 = data
            .get(Symbol::new(&env, "verified_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(outcome_val, outcome);
        assert_eq!(verified_at_val, verified_at);
    }

    #[test]
    fn test_emit_fee_calculated() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let fee_amount = 500i128; // non-zero fee per #345
        let available_after_fee = 5_000i128;

        env.as_contract(&contract_id, || {
            emit_fee_calculated(&env, market_id, &user, fee_amount, available_after_fee);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "fee_calculated"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let fee_val: i128 = data
            .get(Symbol::new(&env, "fee_amount"))
            .unwrap()
            .into_val(&env);
        let available_val: i128 = data
            .get(Symbol::new(&env, "available_after_fee"))
            .unwrap()
            .into_val(&env);
        assert_eq!(fee_val, fee_amount);
        assert_eq!(available_val, available_after_fee);
    }

    /// #710: `market_closed_to_deposits` must expose the canonical topic set
    /// off-chain indexer mappings key on — `[event_name, version, market_id]`
    /// — with `admin` and `closed_at` in the data payload. Locks the wire
    /// shape so an indexer mapping written against it cannot silently break.
    #[test]
    fn test_emit_market_closed_to_deposits() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 42u32;
        let admin = Address::generate(&env);
        let closed_at = 1_700_000_123u64;

        env.as_contract(&contract_id, || {
            emit_market_closed_to_deposits(&env, market_id, &admin, closed_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;
        assert_eq!(topics.len(), 3);

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_closed_to_deposits"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, EVENT_VERSION);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let admin_val: Address = data
            .get(Symbol::new(&env, "admin"))
            .unwrap()
            .into_val(&env);
        let closed_at_val: u64 = data
            .get(Symbol::new(&env, "closed_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(admin_val, admin);
        assert_eq!(closed_at_val, closed_at);
    }
}
