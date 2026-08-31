// Issue #765: Required no_std attribute for Soroban WASM contract execution
#![no_std]
#![warn(clippy::all)]

//! # Market Contract
//!
//! Core prediction market contract for the Vatix protocol. Manages market
//! creation, collateral deposits/withdrawals, share trading (YES/NO), oracle
//! resolution, and position settlement.
//!
//! ## Fee flow
//!
//! ```text
//!  User withdrawal
//!      │  fee_amount = amount * fee_rate_bps / 10_000
//!      ▼
//!  MarketContract
//!      │  token.transfer(market → treasury, fee_amount)  (if treasury registered)
//!      │  treasury.collect_fee(market, token, market_id, fee_amount)
//!      ▼
//!  TreasuryContract  ← accumulates per-token balances
//! ```
//!
//! ## Authorization model
//!
//! | Operation                          | Who may call                    |
//! |------------------------------------|---------------------------------|
//! | `initialize`                       | anyone (once)                   |
//! | `initialize_market` / set_*        | admin                           |
//! | `deposit_collateral`               | any user (rejected if `closed_to_deposits`) |
//! | `update_position`                  | any user (active market; rejected if `closed_to_deposits` **and** the trade would increase locked collateral — see `update_position`'s "Closed-to-deposits policy" docs) |
//! | `withdraw_unused_collateral`       | any user                        |
//! | `resolve_market` (oracle key)      | anyone (valid signature wins)   |
//! | `resolve_market` (admin forced)    | admin (when oracle key is zero) |
//! | `settle_position` / `batch_settle` | any user (resolved market)      |
//! | `update_market_oracle`             | admin                           |
//! | `add_fee_waiver` / `remove_fee_waiver` | admin                       |
//! | `pause` / `unpause`                | admin                           |
//! | `close_market_to_deposits`         | admin                           |
//! | `set_resolution_contract`          | admin                           |
//! | `set_fee_cap`                      | admin                           |
//! | `void_market`                      | registered resolution contract  |
//!
//! ## Storage layout
//!
//! | Key                                 | Type            | Description                                        |
//! |-------------------------------------|-----------------|----------------------------------------------------|
//! | `StorageVersion`                    | `u32`           | Schema version guard (bumped on breaking changes)  |
//! | `Admin`                             | `Address`       | Protocol admin                                     |
//! | `PendingAdmin`                      | `Address`       | Nominated admin for two-step transfer              |
//! | `MarketCounter`                     | `u32`           | Auto-increment counter for market IDs              |
//! | `Market(u32)`                       | `Market`        | Per-market metadata                                |
//! | `Position(u32, Address)`            | `Position`      | Per-user, per-market position (shares, collateral) |
//! | `Treasury`                          | `Address`       | Optional treasury contract for fee collection      |
//! | `FeeRateBps`                        | `i128`          | Withdrawal fee rate in basis points (0–10_000)     |
//! | `OutcomeTokenContract`              | `Address`       | Optional outcome-token contract for mint/burn      |
//! | `ResolutionContract`                | `Address`       | Optional resolution contract that gates resolution |
//! | `ThresholdSigners`                  | `Vec<BytesN<32>>` | Multi-signer quorum public keys (#378)           |
//! | `ThresholdQuorum`                   | `u32`           | Min valid signatures required for resolution (#378)|
//! | `FeeWaivers`                        | `Vec<Address>`  | Admin-managed fee-exempt address list (#483)       |
//! | `MarketParticipants(u32)`           | `Vec<Address>`  | Every address that ever held a position (#495)     |
//! | `PendingFeeRate`                    | `PendingFeeRateChange` | Timelocked fee rate change awaiting execution (#496) |
//! | `Paused`                            | `bool`          | Emergency pause flag; blocks state-mutating calls  |
//! | `AdapterEnabled(AdapterType)`       | `bool`          | Whether the Reflector/Pyth adapter is live (#488)  |
//! | `DepositLock`                       | `bool`          | Reentrancy lock for `deposit_collateral` (#501)    |

mod deposit;
mod error;
mod events;
pub mod oracle;
#[cfg(feature = "oracle-adapter")]
pub mod oracle_adapter;
// `positions` is called from `update_position` and `settle_position` inside the
// `#[contractimpl]` block; the macro expansion hides the call-sites from Clippy's
// dead-code analysis, so the cfg_attr allow is required to keep CI green in
// non-test (release/check) builds.  In test builds the allow is deliberately
// omitted so that any truly dead item inside this module surfaces as a warning.
//
// AUDIT NOTE (#764): this cfg_attr is the intentional, documented suppression
// for contractimpl macro-hidden call-sites only.  A bare #[allow(dead_code)]
// here (without cfg_attr) would be caught by the canary test in storage.rs.
#[cfg_attr(not(test), allow(dead_code))]
// used via contractimpl macro expansion (positions::update_position, positions::calculate_locked_collateral)
mod positions;
// `reconciliation` is called from `get_position_token_parity` and
// `reconcile_position_tokens` inside the `#[contractimpl]` block.
mod reconciliation;
// `settlement` is called from `settle_position` and `batch_settle_positions` inside
// the `#[contractimpl]` block; same macro-expansion visibility issue as `positions`.
//
// AUDIT NOTE (#764): cfg_attr(not(test)) intentionally limits suppression to
// non-test builds so test compilation still sees potential dead items.
#[cfg_attr(not(test), allow(dead_code))]
// used via contractimpl macro expansion (settlement::settle_position, settlement::batch_settle)
pub mod settlement;
mod withdraw;

// `storage` is re-exported as `pub mod` for workspace integration tests and is
// called throughout the `#[contractimpl]` methods; individual helpers that are
// only exercised through tests are flagged by Clippy without this allow.
//
// AUDIT NOTE (#764): cfg_attr(not(test)) keeps the suppression scoped to
// non-test builds where the pub re-export hides usages from Clippy.
#[cfg_attr(not(test), allow(dead_code))] // pub-exported for integration tests; helpers used in contractimpl methods
pub mod storage;
mod test;
#[cfg(test)]
mod tests_vectors;
pub mod types;
#[cfg(test)]
mod withdraw_fuzz;
// `validation` helpers are called from `deposit`, `withdraw`, `positions`, and
// `oracle` sub-modules; Clippy cannot trace cross-module usages through the
// `#![no_std]` + macro context.
//
// AUDIT NOTE (#764): cfg_attr(not(test)) intentionally limits suppression to
// non-test builds; any item that is truly only used in tests will surface
// as dead_code in test builds and should be moved under #[cfg(test)] instead.
#[cfg_attr(not(test), allow(dead_code))]
// called by deposit::deposit_collateral, withdraw::withdraw_unused_collateral, oracle::verify_market_outcome
mod validation;

use crate::error::ContractError;
#[cfg(feature = "oracle-adapter")]
use crate::oracle_adapter::Asset;
#[cfg(feature = "oracle-adapter")]
use crate::types::MarketAdapterConfig;
use crate::types::{AdapterType, Market, MarketStatus, Position};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};
use vatix_outcome_token_contract::{types::TokenKind, OutcomeTokenContractClient};
use vatix_resolution_contract::types::CandidateStatus as ResolutionCandidateStatus;
use vatix_resolution_contract::ResolutionContractClient;

/// Delay, in seconds, an admin-proposed fee rate change must wait before it
/// can be applied via `execute_fee_rate_change` (Issue #496). 48 hours gives
/// integrators and users advance notice of a fee change before it lands.
pub const FEE_RATE_TIMELOCK_SECONDS: u64 = 172_800;

/// Maximum number of addresses that a single `batch_settle_positions` call may
/// process (Issue #551). Callers who need to settle more positions should either
/// use the paginated `settle_positions_page` endpoint, or split their list into
/// multiple calls of at most this many addresses.
///
/// The cap prevents gas-griefing: a malicious or buggy caller cannot force the
/// contract to iterate an unbounded list in one transaction.
pub const MAX_BATCH_SETTLE_SIZE: u32 = 100;

#[contract]
pub struct MarketContract;

#[contractimpl]
impl MarketContract {
    /// Create a new prediction market and return its unique identifier.
    ///
    /// Only the stored admin may call this function. The market starts in
    /// [`MarketStatus::Active`] and accepts collateral deposits immediately.
    ///
    /// # Arguments
    /// * `env` - Soroban contract environment
    /// * `creator` - Admin address that authorizes market creation
    /// * `question` - Human-readable market question (1–499 characters)
    /// * `end_time` - Unix timestamp after which trading closes (must be
    ///   within one year of the current ledger time)
    /// * `oracle_pubkey` - Ed25519 public key of the oracle that will sign
    ///   the resolution outcome
    /// * `collateral_token` - Address of the SAC token used as collateral
    ///   (e.g. USDC)
    ///
    /// # Returns
    /// The `u32` market ID assigned to the new market (auto-incremented).
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] – `creator` is not the admin
    /// - [`ContractError::InvalidQuestion`] – question is empty or ≥ 500 chars
    /// - [`ContractError::InvalidTimestamp`] – `end_time` is in the past or
    ///   more than one year in the future
    ///
    /// # Events
    /// Emits [`MarketCreated`] with `market_id`, `creator`, `question`,
    /// and `end_time` as payload.
    ///
    /// # Example
    /// ```ignore
    /// let market_id = client.initialize_market(
    ///     &admin,
    ///     &String::from_str(&env, "Will BTC reach $100k by end of year?"),
    ///     &(env.ledger().timestamp() + 86_400),
    ///     &oracle_pubkey,
    ///     &usdc_token,
    /// );
    /// assert_eq!(market_id, 1);
    /// ```
    /// Bootstrap the contract by setting the admin address.
    ///
    /// Must be called once by the admin immediately after deployment.
    /// Subsequent calls return [`ContractError::AlreadyInitialized`].
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `admin` - Admin address (must be a user account, not a contract)
    ///
    /// # Returns
    /// `Ok(())` on successful initialization
    ///
    /// # Errors
    /// - [`ContractError::AlreadyInitialized`] – contract was previously initialized
    /// - [`ContractError::InvalidAdmin`] – admin address is a contract or otherwise invalid
    ///
    /// # Security
    /// - Requires authorization from the admin address
    /// - Can only be called once per deployment
    /// - Validates admin is a user account, not a contract
    ///
    /// # Example
    /// ```ignore
    /// client.initialize(&admin_address)?;
    /// ```
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        // 1. Validate admin address before authorization to fail fast
        validation::validate_admin_address(&admin)?;

        // 2. Require authorization from the admin
        admin.require_auth();

        // 3. Check if already initialized.
        //
        // `has_admin` is the canonical "already initialized" sentinel — the
        // same value that `require_initialized` checks — so this guard is
        // consistent with every other function's initialization check.
        // A second call to `initialize` after a successful first call always
        // returns `AlreadyInitialized` (#42), leaving all storage unchanged.
        if storage::has_admin(&env) {
            return Err(ContractError::AlreadyInitialized);
        }

        // 4. Write version BEFORE writing admin (Issue #548).
        //
        // If the transaction is interrupted between the two writes (e.g. an
        // out-of-gas or host trap), the resulting partial state differs based
        // on which write completed first:
        //
        //   Admin set, version NOT set (old ordering):
        //     has_admin() == true  →  initialize() returns AlreadyInitialized
        //     assert_version() returns UpgradeRequired
        //     Result: contract is permanently bricked — initialization cannot
        //     be retried, and no storage accessor can be called. The only
        //     recovery is redeployment.
        //
        //   Version set, admin NOT set (new ordering):
        //     has_admin() == false  →  initialize() can be retried
        //     assert_version() returns Ok
        //     All require_initialized guards reject callers in the gap.
        //     Result: contract is in a recoverable, safe state.
        //
        // Writing version first eliminates the bricked state entirely.
        storage::set_version(&env);
        storage::set_admin(&env, &admin);

        // 4.5. Fail closed on legacy V1 oracle signatures (#701). V1 lacks
        // the network-passphrase/epoch binding that V2 provides, so a fresh
        // deployment must not silently accept it. Disabling it here — rather
        // than relying on the storage default — means a misconfigured
        // deployment that never calls `set_oracle_v1_disabled` still fails
        // closed instead of quietly accepting weaker signatures. Operators
        // who still need V1 for a migration window must explicitly
        // re-enable it via `set_oracle_v1_disabled(admin, false)`.
        storage::set_oracle_v1_disabled(&env, true);

        // 5. Emit initialization event
        events::emit_contract_initialized(&env, &admin);

        Ok(())
    }

    /// Begin a two-step admin transfer by nominating a new admin address.
    ///
    /// Only the current admin may call this. The nominated address becomes the
    /// pending admin and must confirm the transfer by calling [`accept_admin`].
    /// Calling this again before acceptance overwrites the previous nomination.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `current_admin` - Current admin authorizing the transfer
    /// * `new_admin` - Address to nominate as pending admin (must be a user account)
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – contract is not initialized or `current_admin` is not the stored admin
    /// - [`ContractError::InvalidAdmin`] – `new_admin` is a contract or otherwise invalid
    pub fn propose_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        if !storage::has_admin(&env) {
            return Err(ContractError::NotAdmin);
        }

        // 3. Verify current admin
        let stored_admin = storage::get_admin(&env)?;
        if current_admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        // 4. The current admin must actually authorize this call. Without this
        //    a bad merge previously left `propose_admin` unauthenticated:
        //    `current_admin` is a plain parameter, so anyone could pass the
        //    real admin's address, nominate themselves, then self-`accept_admin`
        //    (which only checks the *new* admin's auth) to seize the contract.
        current_admin.require_auth();

        // 5. Reject a contract address as the nominee (mirrors `initialize`);
        //    `accept_admin` cannot `require_auth` a contract that never signs.
        validation::validate_admin_address(&new_admin)?;

        storage::set_pending_admin(&env, &new_admin);
        events::emit_admin_transfer_proposed(&env, &current_admin, &new_admin);

        Ok(())
    }

    /// Complete a two-step admin transfer by accepting a pending nomination.
    ///
    /// Must be called by the address that was nominated via [`propose_admin`].
    /// On success the caller becomes the new admin and the pending nomination
    /// is cleared.
    ///
    /// # Errors
    /// - [`ContractError::NoPendingAdmin`] – no nomination is outstanding
    /// - [`ContractError::Unauthorized`] – `new_admin` does not match the pending nomination
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        let pending = storage::get_pending_admin(&env).ok_or(ContractError::NoPendingAdmin)?;
        if new_admin != pending {
            return Err(ContractError::Unauthorized);
        }
        new_admin.require_auth();
        let old_admin = storage::get_admin(&env)?;
        storage::set_admin(&env, &new_admin);
        storage::clear_pending_admin(&env);
        events::emit_admin_transfer_accepted(&env, &old_admin, &new_admin);
        Ok(())
    }

    /// Cancel an outstanding two-step admin transfer nomination.
    ///
    /// Only the current admin may call this. Clears the pending nomination
    /// set by [`propose_admin`] so it can no longer be accepted via
    /// [`accept_admin`]. Safe to call at any time before acceptance; has no
    /// effect on the current admin.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `current_admin` - Current admin authorizing the cancellation
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – contract is not initialized or `current_admin` is not the stored admin
    /// - [`ContractError::NoPendingAdmin`] – no nomination is outstanding to cancel
    pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        if !storage::has_admin(&env) {
            return Err(ContractError::NotAdmin);
        }

        let stored_admin = storage::get_admin(&env)?;
        if current_admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        current_admin.require_auth();

        let pending = storage::get_pending_admin(&env).ok_or(ContractError::NoPendingAdmin)?;
        storage::clear_pending_admin(&env);
        events::emit_admin_transfer_canceled(&env, &current_admin, &pending);

        Ok(())
    }

    pub fn initialize_market(
        env: Env,
        creator: Address,
        question: String,
        end_time: u64,
        oracle_pubkey: BytesN<32>,
        collateral_token: Address,
        metadata_uri: Option<String>,
    ) -> Result<u32, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        // Emergency mode: market creation is only allowed in Normal mode
        validation::require_emergency_mode_allows(&env, &[crate::types::EmergencyMode::Normal])?;
        // 1. Verify creator is admin
        creator.require_auth();
        let admin = storage::get_admin(&env)?;
        if creator != admin {
            return Err(ContractError::NotAdmin);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;
        validation::validate_metadata_uri(&metadata_uri)?;

        // Guard: an all-zero pubkey can never produce a valid Ed25519 signature,
        // making the market permanently unresolvable.
        if oracle_pubkey == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(ContractError::InvalidSignature);
        }

        // 3. Generate market ID
        let market_id = storage::increment_market_id(&env)?;

        // Guard: the generated ID must not already be in storage.
        // Under normal operation this cannot happen (the counter is monotonic),
        // but we reject explicitly to prevent any accidental overwrite.
        if storage::has_market(&env, market_id)? {
            return Err(ContractError::AlreadyInitialized);
        }

        // 4. Create Market struct
        let market = Market {
            id: market_id,
            question: question.clone(),
            end_time,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: creator.clone(),
            created_at: current_time,
            collateral_token,
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: crate::types::AdapterType::Ed25519,
            outcome_count: 2,
            closed_to_deposits: false,
        };

        // 5. Store market
        storage::set_market(&env, market_id, &market)?;
        storage::append_market_id(&env, market_id);

        // 6. Emit event
        events::emit_market_created(
            &env,
            market_id,
            &creator,
            &question,
            end_time,
            &metadata_uri,
        );

        // 7. Return market ID
        Ok(market_id)
    }

    /// Deposit USDC collateral into a prediction market
    ///
    /// # Arguments
    /// * `user` - User's Stellar address (must authorize this call)
    /// * `market_id` - Market identifier
    /// * `amount` - Amount in stroops (1 USDC = 10^7 stroops)
    ///
    /// # Errors
    /// - `MarketNotFound`: market_id doesn't exist
    /// - `MarketNotActive`: Market is resolved or cancelled
    /// - `InvalidQuantity`: amount <= 0 or exceeds max
    /// - `TokenTransferFailed`: USDC transfer failed
    /// - `ArithmeticOverflow`: Amount would cause overflow
    pub fn deposit_collateral(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        deposit::deposit_collateral(env, user, market_id, amount)
    }

    /// Withdraw unused collateral from a market
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User withdrawing
    /// * `market_id` - Market to withdraw from
    /// * `amount` - Amount to withdraw in stroops
    ///
    /// # Returns
    /// Unit (success)
    ///
    /// # Errors
    /// - MarketNotFound
    /// - InsufficientCollateral: Trying to withdraw locked collateral
    /// - InvalidQuantity: Amount <= 0
    ///
    /// # Events
    /// Emits CollateralWithdrawn event
    pub fn withdraw_unused_collateral(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        withdraw::withdraw_unused_collateral(env, user, market_id, amount)
    }

    /// Resolve a market with oracle-signed outcome
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market to resolve (decimal string, e.g. "1")
    /// * `outcome` - Outcome (true = YES won, false = NO won)
    /// * `signature` - Oracle's Ed25519 signature (64 bytes)
    /// * `expires_at` - Unix timestamp deadline after which this signed message
    ///   is no longer valid. Pass `0` to disable expiry enforcement (backwards
    ///   compatible with callers that do not supply a deadline). A non-zero
    ///   value that is in the past returns [`ContractError::OracleMessageExpired`].
    ///
    /// # Returns
    /// Unit (success)
    ///
    /// # Errors
    /// - MarketNotFound
    /// - MarketAlreadyResolved
    /// - OracleMessageExpired: The oracle message deadline has passed
    /// - InvalidSignature: Signature verification failed
    /// - UnauthorizedOracle: Wrong oracle pubkey
    ///
    /// # Events
    /// Emits MarketResolved event with the authorized oracle public key as resolver.
    pub fn resolve_market(
        env: Env,
        resolver: Address,
        market_id: String,
        outcome: bool,
        signature: BytesN<64>,
        expires_at: u64,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        // Emergency mode: resolve is blocked in SettleOnly and GlobalFreeze;
        // allowed in Normal and TradingHalted.
        validation::require_emergency_mode_allows(
            &env,
            &[
                crate::types::EmergencyMode::Normal,
                crate::types::EmergencyMode::TradingHalted,
            ],
        )?;
        resolver.require_auth();
        let market_id = validation::parse_market_id(&market_id)?;
        // Step 1: Load and validate market
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if storage::is_oracle_v1_disabled(&env) {
            return Err(ContractError::UnauthorizedOracle);
        }

        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        // Step 1.5 (expiry): Reject stale oracle messages. Fails closed
        // (#701): `expires_at == 0` is no longer treated as "no expiry" —
        // that sentinel used to silently disable the whole check, letting a
        // resolver bypass expiry entirely by passing zero. Every call must
        // now supply a genuine future timestamp, or it is rejected outright.
        if expires_at == 0 || env.ledger().timestamp() > expires_at {
            return Err(ContractError::OracleMessageExpired);
        }

        // Step 1.6: When a resolution contract is registered for this
        // contract (see `set_resolution_contract`), resolve_market may only
        // be reached once that contract has finalized a matching candidate
        // — i.e. its challenge-window lifecycle has run to completion. This
        // is what lets `ResolutionContract::finalize`'s callback into
        // `resolve_market` succeed while any other direct caller (oracle key
        // holder, admin, etc.) is rejected until finalize() has run.
        require_resolution_finalized(&env, market_id, outcome, &signature)?;

        // Step 2: Verify outcome using the configured adapter for this market.
        oracle::verify_market_outcome(
            &env,
            market_id,
            &market,
            market.adapter_type.clone(),
            outcome,
            &signature,
        )?;
        events::emit_oracle_signature_verified(&env, market_id, outcome, env.ledger().timestamp());

        // Step 3: Update market (status, outcome, resolver, persist)
        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);
        market.resolver = Some(resolver.clone());
        let resolved_at = env.ledger().timestamp();
        market.resolved_at = Some(resolved_at);
        storage::set_market(&env, market_id, &market)?;

        // Step 4: Emit event
        events::emit_market_resolved(
            &env,
            market_id,
            &market.oracle_pubkey,
            &resolver,
            outcome,
            resolved_at,
        );

        Ok(())
    }

    /// Resolve a market with V2 oracle-signed outcome (binding network passphrase, expiry, and market epoch)
    pub fn resolve_market_v2(
        env: Env,
        resolver: Address,
        market_id: String,
        outcome: bool,
        valid_until: u64,
        epoch: u32,
        signature: BytesN<64>,
        passphrase_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        validation::require_emergency_mode_allows(
            &env,
            &[
                crate::types::EmergencyMode::Normal,
                crate::types::EmergencyMode::TradingHalted,
            ],
        )?;
        resolver.require_auth();
        let market_id = validation::parse_market_id(&market_id)?;

        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;

        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        if env.ledger().timestamp() > valid_until {
            return Err(ContractError::OracleMessageExpired);
        }

        require_resolution_finalized(&env, market_id, outcome, &signature)?;

        oracle::verify_market_outcome_v2(
            &env,
            &passphrase_hash,
            market_id,
            &market,
            market.adapter_type.clone(),
            outcome,
            valid_until,
            epoch,
            &signature,
        )?;
        events::emit_oracle_signature_verified(&env, market_id, outcome, env.ledger().timestamp());

        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);
        market.resolver = Some(resolver.clone());
        let resolved_at = env.ledger().timestamp();
        market.resolved_at = Some(resolved_at);
        storage::set_market(&env, market_id, &market)?;

        events::emit_market_resolved(
            &env,
            market_id,
            &market.oracle_pubkey,
            &resolver,
            outcome,
            resolved_at,
        );

        Ok(())
    }

    /// Verify an oracle signature for `(market_id, outcome)` without mutating
    /// any state (#489).
    ///
    /// This is the read-only counterpart of the verification step performed
    /// inside [`resolve_market`]: it delegates to
    /// [`oracle::verify_market_outcome`] so the exact same signed-payload
    /// construction, adapter dispatch, and Ed25519 fallback logic (#488) are
    /// used. The resolution contract's `propose()` calls this cross-contract
    /// to reject an invalid oracle signature before opening a challenge
    /// window, rather than only discovering the bad signature later at
    /// `finalize()` time.
    ///
    /// No authorization is required — this never changes contract state, it
    /// only reports whether `signature` verifies.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] — the market does not exist.
    /// - [`ContractError::UnauthorizedOracle`] — legacy V1 oracle signatures
    ///   are disabled (#701; see [`set_oracle_v1_disabled`]).
    /// - [`ContractError::InvalidSignature`] / [`ContractError::UnauthorizedOracle`]
    ///   — signature does not verify (see [`oracle::verify_market_outcome`]).
    pub fn verify_signature(
        env: Env,
        market_id: u32,
        outcome: bool,
        signature: BytesN<64>,
    ) -> Result<(), ContractError> {
        // Fail closed (#701): this must mirror `resolve_market`'s V1 gate.
        // Without it, the resolution contract's `propose()` — which calls
        // this cross-contract to pre-validate a signature — would accept a
        // V1 signature and open a challenge window for a candidate that
        // `resolve_market` can never actually finalize once V1 is disabled.
        if storage::is_oracle_v1_disabled(&env) {
            return Err(ContractError::UnauthorizedOracle);
        }
        let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        oracle::verify_market_outcome(
            &env,
            market_id,
            &market,
            market.adapter_type.clone(),
            outcome,
            &signature,
        )
    }

    /// Verify a V2 oracle signature for `(market_id, outcome)` without mutating any state.
    pub fn verify_signature_v2(
        env: Env,
        passphrase_hash: BytesN<32>,
        market_id: u32,
        outcome: bool,
        valid_until: u64,
        epoch: u32,
        signature: BytesN<64>,
    ) -> Result<(), ContractError> {
        let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        oracle::verify_market_outcome_v2(
            &env,
            &passphrase_hash,
            market_id,
            &market,
            market.adapter_type.clone(),
            outcome,
            valid_until,
            epoch,
            &signature,
        )
    }

    /// Disable or enable legacy V1 oracle signatures (#657).
    ///
    /// Admin-controlled toggle for mainnet security compliance.
    pub fn set_oracle_v1_disabled(
        env: Env,
        admin: Address,
        disabled: bool,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_oracle_v1_disabled(&env, disabled);
        Ok(())
    }

    /// Return whether legacy V1 oracle signatures are currently disabled (#657).
    pub fn is_oracle_v1_disabled(env: Env) -> bool {
        storage::is_oracle_v1_disabled(&env)
    }

    /// Enable or disable the Reflector/Pyth oracle adapter for resolution (#488).
    ///
    /// Only the stored admin may call this. While an adapter is disabled (the
    /// default), `resolve_market` and `verify_signature` fall back to direct
    /// Ed25519 verification of the proof against the market's `oracle_pubkey`
    /// instead of routing through the (unavailable) adapter — see
    /// [`oracle::verify_market_outcome`] for the full rationale.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn set_adapter_enabled(
        env: Env,
        admin: Address,
        adapter_type: AdapterType,
        enabled: bool,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_adapter_enabled(&env, &adapter_type, enabled);
        events::emit_oracle_adapter_configured(&env, adapter_type, enabled);
        Ok(())
    }

    /// Return whether the given oracle adapter type is currently enabled (#488).
    pub fn is_adapter_enabled(env: Env, adapter_type: AdapterType) -> bool {
        storage::is_adapter_enabled(&env, &adapter_type)
    }

    /// Set (or replace) the Reflector/Pyth adapter config for `market_id`
    /// (#681) — the oracle contract address, asset, and price threshold
    /// `oracle::verify_market_outcome` uses once the corresponding adapter
    /// type is enabled via `set_adapter_enabled` (#680).
    ///
    /// Only available when the `oracle-adapter` Cargo feature is compiled in
    /// (#778). Without the feature the contract's adapter dispatch already
    /// fails closed with `UnauthorizedOracle` when Reflector is enabled, so
    /// there is nothing to configure.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::MarketNotFound`] — `market_id` does not exist.
    #[cfg(feature = "oracle-adapter")]
    pub fn set_market_adapter_config(
        env: Env,
        admin: Address,
        market_id: u32,
        oracle_contract: Address,
        asset: Asset,
        resolution_price: i128,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        storage::set_market_adapter_config(
            &env,
            market_id,
            &MarketAdapterConfig {
                oracle_contract,
                asset,
                resolution_price,
            },
        );
        Ok(())
    }

    /// Return the stored Reflector/Pyth adapter config for `market_id`, if any (#681).
    /// Only available when the `oracle-adapter` Cargo feature is compiled in (#778).
    #[cfg(feature = "oracle-adapter")]
    pub fn get_market_adapter_config(env: Env, market_id: u32) -> Option<MarketAdapterConfig> {
        storage::get_market_adapter_config(&env, market_id)
    }

    /// Pause the contract for emergency maintenance.
    ///
    /// While paused, `deposit_collateral`, `withdraw_unused_collateral`,
    /// `update_position`, `initialize_market`, `cancel_market`,
    /// `resolve_market`, and `resolve_market_threshold` all reject with
    /// [`ContractError::ContractPaused`] via [`validation::require_not_paused`].
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_paused(&env, true);
        events::emit_emergency_pause_toggled(&env, true);
        Ok(())
    }

    /// Unpause the contract, restoring normal operation.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_paused(&env, false);
        events::emit_emergency_pause_toggled(&env, false);
        Ok(())
    }

    /// Return whether the contract is currently paused for emergency maintenance.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Set the coordinated emergency mode (Issue #662).
    ///
    /// Only the stored admin may call this. The mode is shared (or mirrored)
    /// across the Market, Treasury, and Resolution contracts. Operators should
    /// set it on all three contracts with the same value for coordinated
    /// behaviour.
    ///
    /// # Mode effects
    ///
    /// | Mode             | Blocked operations                                     |
    /// |------------------|--------------------------------------------------------|
    /// | `Normal`         | (none — all operations allowed)                        |
    /// | `TradingHalted`  | deposit, trade, create market, propose resolution      |
    /// | `SettleOnly`     | deposit, trade, create market, resolve, propose        |
    /// | `GlobalFreeze`   | all non-admin operations                               |
    ///
    /// In `TradingHalted` and `SettleOnly`, withdraw and settle remain
    /// available so users can always exit during an incident.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::EmergencyModeActive`] — when setting to a mode other
    ///   than `Normal` from `GlobalFreeze` (must unpause first ... actually no,
    ///   this is the admin changing the mode, so always allowed).
    pub fn set_emergency_mode(
        env: Env,
        admin: Address,
        new_mode: crate::types::EmergencyMode,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_emergency_mode(&env, &new_mode);
        events::emit_emergency_mode_changed(&env, &new_mode, &admin);
        Ok(())
    }

    /// Return the current coordinated emergency mode.
    /// Defaults to `Normal` when never explicitly set.
    pub fn get_emergency_mode(env: Env) -> crate::types::EmergencyMode {
        storage::get_emergency_mode(&env)
    }

    /// Cancel a market before it is resolved, halting all further trading.
    ///
    /// Only the stored admin may call this. The market must still be
    /// [`MarketStatus::Active`]; a resolved market has a final outcome and an
    /// already-canceled market is rejected to surface the redundant call.
    /// Once canceled, deposits and position updates are rejected (both already
    /// require an `Active` status), and affected users may reclaim their
    /// collateral via [`withdraw_canceled_collateral`].
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `admin` - Must be the stored admin address (authorizes the call)
    /// * `market_id` - Identifier of the market to cancel
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin
    /// - [`ContractError::MarketNotFound`] – the market does not exist
    /// - [`ContractError::MarketAlreadyResolved`] – the market is already resolved
    /// - [`ContractError::MarketNotActive`] – the market is already canceled
    ///
    /// # Events
    /// Emits [`MarketCanceled`] with `market_id`, `canceler`, and
    /// `canceled_at` on success.
    pub fn cancel_market(env: Env, admin: Address, market_id: u32) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        // 1. Authorization: only the stored admin may cancel a market.
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        // 2. Load the market and enforce the cancel policy (Active only).
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        validation::validate_cancelable(&market.status)?;

        // 3. Transition to Canceled and persist.
        market.status = MarketStatus::Canceled;
        storage::set_market(&env, market_id, &market)?;

        // 4. Emit the cancellation event for off-chain indexers.
        events::emit_market_canceled(&env, market_id, &admin, env.ledger().timestamp());

        Ok(())
    }

    /// Explicitly reopen a previously canceled market, restoring it to Active.
    ///
    /// This is the **only** sanctioned path from `Canceled` back to `Active`.
    /// All other entry points that mutate market state either require `Active`
    /// as a precondition or transition to `Resolved`/`Canceled` — never back
    /// to `Active`. Calling [`reopen_market`] on a market that is already
    /// `Active` or `Resolved` is rejected; only `Canceled` markets may be
    /// reopened.
    ///
    /// # When to use
    /// Use this when a market was canceled prematurely (e.g. an admin error)
    /// and the admin wants to restore it to its original trading state. Users
    /// who already reclaimed collateral via [`withdraw_canceled_collateral`]
    /// will need to re-deposit if they wish to resume trading.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `admin` - Must be the stored admin address (authorizes the call)
    /// * `market_id` - Identifier of the canceled market to reopen
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin
    /// - [`ContractError::MarketNotFound`] – the market does not exist
    /// - [`ContractError::MarketAlreadyResolved`] – the market is resolved; resolved
    ///   markets are terminal and can never return to Active
    /// - [`ContractError::MarketNotActive`] – the market is already Active; calling
    ///   reopen on an Active market is a no-op and is rejected to surface bugs
    ///
    /// # Events
    /// Emits [`MarketReopened`] with `market_id`, `admin`, and `reopened_at`.
    pub fn reopen_market(env: Env, admin: Address, market_id: u32) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;

        // 1. Authorization: only the stored admin may reopen a market.
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        // 2. Load the market and enforce the reopen policy (Canceled only).
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        validation::validate_reopenable(&market.status)?;

        // 3. Transition back to Active and persist.
        market.status = MarketStatus::Active;
        storage::set_market(&env, market_id, &market)?;

        // 4. Emit the reopen event for off-chain indexers.
        events::emit_market_reopened(&env, market_id, &admin, env.ledger().timestamp());

        Ok(())
    }

    /// Void a market at the direction of the registered resolution contract
    /// (Issue #708).
    ///
    /// This is the market-side half of the resolution contract's `void_market`
    /// arbitration outcome: when a dispute cannot be safely vindicated
    /// on-chain for either side, the resolution contract slashes/refunds the
    /// posted bonds and then calls this to force the market to
    /// [`MarketStatus::Canceled`], unsticking it so users can reclaim their
    /// collateral via [`Self::withdraw_canceled_collateral`].
    ///
    /// # Authorization
    /// Callable **only** by the address registered as the resolution contract
    /// (see [`Self::propose_resolution_contract`] /
    /// [`Self::execute_resolution_contract`]). Every other caller — the admin
    /// included — is rejected with [`ContractError::Unauthorized`], and the
    /// call also fails closed with [`ContractError::Unauthorized`] when no
    /// resolution contract is registered at all, so a wrong or malicious
    /// caller can never void a live market.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `caller` - Must be the registered resolution contract (authorizes the call)
    /// * `market_id` - Identifier of the market to void
    ///
    /// # Errors
    /// - [`ContractError::NotInitialized`] – the contract is not initialized
    /// - [`ContractError::Unauthorized`] – `caller` is not the registered
    ///   resolution contract, or none is registered
    /// - [`ContractError::MarketNotFound`] – the market does not exist
    /// - [`ContractError::MarketAlreadyResolved`] – the market already has a
    ///   final outcome and cannot be voided
    /// - [`ContractError::MarketNotActive`] – the market is already canceled
    ///
    /// # Events
    /// Emits [`events::MarketVoided`] with `market_id`, `voided_by`, and
    /// `voided_at` on success.
    pub fn void_market(env: Env, caller: Address, market_id: u32) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        // Intentionally NOT gated by `require_not_paused`: voiding is the
        // sanctioned unstick path for a market frozen mid-dispute, and the
        // only possible caller is the resolution contract itself.
        caller.require_auth();

        // Fail closed: an unset resolution contract means nobody is authorized
        // to void — never fall back to admin or open access.
        let resolution_contract =
            storage::get_resolution_contract(&env).ok_or(ContractError::Unauthorized)?;
        if caller != resolution_contract {
            return Err(ContractError::Unauthorized);
        }

        // Checks: load the market and enforce the void policy (Active only).
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        match market.status {
            MarketStatus::Resolved => return Err(ContractError::MarketAlreadyResolved),
            MarketStatus::Canceled => return Err(ContractError::MarketNotActive),
            MarketStatus::Active => {}
        }

        // Effects: transition to Canceled and persist before emitting.
        market.status = MarketStatus::Canceled;
        storage::set_market(&env, market_id, &market)?;

        events::emit_market_voided(&env, market_id, &caller, env.ledger().timestamp());

        Ok(())
    }

    /// Reclaim deposited collateral from a canceled market.
    ///
    /// When a market is canceled before resolution there is no winning outcome,
    /// so each user is made whole by returning the full collateral they have
    /// deposited in that market. The user's position balances are zeroed and the
    /// collateral (SAC) tokens are transferred from the contract back to them.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User reclaiming their collateral (must authorize the call)
    /// * `market_id` - Identifier of the canceled market
    ///
    /// # Returns
    /// The amount of collateral refunded to the user, in stroops.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – the market does not exist
    /// - [`ContractError::MarketNotActive`] – the market is not canceled, so the
    ///   reclaim path does not apply
    /// - [`ContractError::NoPositionFound`] – the user has no position in the market
    /// - [`ContractError::InsufficientCollateral`] – the user has no collateral to reclaim
    ///
    /// # Events
    /// Emits `CollateralWithdrawn` with the refunded amount and the user's new
    /// (zero) total.
    pub fn withdraw_canceled_collateral(
        env: Env,
        user: Address,
        market_id: u32,
    ) -> Result<i128, ContractError> {
        validation::require_not_paused(&env)?;
        // 1. Authorization: only the position owner may reclaim their collateral.
        user.require_auth();

        // 2. The reclaim path is exclusive to canceled markets.
        let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Canceled {
            return Err(ContractError::MarketNotActive);
        }

        // 3. Load the user's position and the full deposited balance.
        let mut position =
            storage::get_position(&env, market_id, &user)?.ok_or(ContractError::NoPositionFound)?;
        let refund = position.total_deposited;
        if refund <= 0 {
            return Err(ContractError::InsufficientCollateral);
        }

        // 4. Refund the collateral from the contract back to the user.
        let contract_address = env.current_contract_address();
        let token_client = soroban_sdk::token::Client::new(&env, &market.collateral_token);
        token_client.transfer(&contract_address, &user, &refund);

        // 5. Zero out the position balances now that the collateral has left.
        position.total_deposited = 0;
        position.locked_collateral = 0;
        storage::set_position(&env, market_id, &user, &position)?;

        // 6. Emit position_updated so indexers see the zeroed balances.
        events::emit_position_updated(
            &env,
            market_id,
            &user,
            position.yes_shares,
            position.no_shares,
            position.locked_collateral,
        );

        // 7. Reuse the collateral-withdrawn event so indexers track the refund.
        events::emit_collateral_withdrawn(&env, &user, market_id, refund, position.total_deposited);

        Ok(refund)
    }

    /// Buy or sell YES/NO shares by applying signed deltas to a user's position.
    ///
    /// **This is the primary trading entry point for the Vatix prediction market protocol.**
    ///
    /// This function provides the on-chain interface for share trading, implementing
    /// the core logic from [`positions::update_position`] with comprehensive market-level
    /// and authorization validations. It supports both buying (positive delta) and
    /// selling (negative delta) of YES and NO shares in a single atomic operation.
    ///
    /// # Trading Flow
    /// 1. User deposits collateral via [`deposit_collateral`]
    /// 2. User calls `update_position` to buy/sell shares
    /// 3. Contract validates market state, user authorization, and collateral requirements
    /// 4. Position is updated and locked collateral is recalculated
    /// 5. Outcome tokens are minted/burned (if outcome-token contract is registered)
    /// 6. Events are emitted for off-chain indexing
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User whose position is updated (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `yes_delta` - Change in YES shares (positive to buy, negative to sell)
    /// * `no_delta` - Change in NO shares (positive to buy, negative to sell)
    /// * `market_price` - Current market price in basis points (0–10_000) used
    ///   to calculate locked collateral for the resulting net position
    ///
    /// # Returns
    /// The updated [`Position`] structure containing the new share balances,
    /// locked collateral, and total deposited amount.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – market does not exist
    /// - [`ContractError::MarketNotActive`] – market is `Resolved` **or
    ///   `Canceled`**. The check is `status != Active`, so a canceled market
    ///   rejects every trade with this same error — there is no separate
    ///   "canceled" trading error, by design, since the caller-facing action
    ///   (no trades allowed) is identical for both non-Active states. See
    ///   `tests/canceled_market_guard_test.rs` for coverage of this path.
    /// - [`ContractError::MarketExpired`] – current time exceeds market `end_time`
    /// - [`ContractError::InvalidPrice`] – `market_price` is outside valid range (0–10_000)
    /// - [`ContractError::MarketClosedToDeposits`] – the market was closed via
    ///   [`close_market_to_deposits`] and this call would increase locked
    ///   collateral (open new exposure). Trades that keep the lock flat or
    ///   reduce it (selling / closing out a position) are unaffected — only
    ///   new exposure is blocked. See `# Closed-to-deposits policy` below.
    /// - [`ContractError::InsufficientCollateral`] – deposited collateral insufficient
    ///   to cover the increased locked amount
    /// - [`ContractError::InvalidShareAmount`] – deltas would result in negative share balance
    ///
    /// # Events
    /// - `PositionUpdated` – emitted on successful position change with new balances
    /// - `TradeExecuted` – emitted for each non-zero delta (YES and/or NO)
    /// - `PositionLimitExceeded` – emitted when delta would drive share balance negative
    ///
    /// # Examples
    /// ```ignore
    /// // Buy 100 YES shares at 60% market price
    /// let position = client.update_position(
    ///     &user,
    ///     &market_id,
    ///     &(100 * STROOPS_PER_USDC),  // yes_delta: buy 100
    ///     &0i128,                       // no_delta: no change
    ///     &6_000i128,                   // market_price: 60%
    /// );
    /// // Result: 60 USDC locked (100 shares * 60% price)
    ///
    /// // Sell 50 YES shares
    /// let position = client.update_position(
    ///     &user,
    ///     &market_id,
    ///     &(-50 * STROOPS_PER_USDC),  // yes_delta: sell 50
    ///     &0i128,                       // no_delta: no change
    ///     &6_000i128,                   // market_price: 60%
    /// );
    /// ```
    ///
    /// # Closed-to-deposits policy
    /// [`close_market_to_deposits`] blocks [`deposit_collateral`] outright, and
    /// also blocks `update_position` calls that would *increase* locked
    /// collateral (opening a new position or growing an existing one) — this
    /// is the "no last-minute position changes" use case the admin flag is
    /// for. Calls that keep the lock flat or reduce it (selling shares,
    /// closing out part or all of a position) remain unaffected, since they
    /// shed risk rather than add it and require no new deposit. Withdrawals
    /// and settlement are never affected by `closed_to_deposits`.
    ///
    /// # Security
    /// - Requires user authorization via `user.require_auth()`
    /// - Validates market is Active and not expired
    /// - Enforces collateral requirements before state changes
    /// - Blocks new exposure once the market is closed to deposits
    /// - Prevents negative share balances
    /// - All state changes are atomic (succeed or revert together)
    pub fn update_position(
        env: Env,
        user: Address,
        market_id: u32,
        yes_delta: i128,
        no_delta: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        validation::require_not_paused(&env)?;
        // Emergency mode: trading is blocked unless mode is Normal
        validation::require_emergency_mode_allows(&env, &[crate::types::EmergencyMode::Normal])?;
        // 1. Authorization
        user.require_auth();

        // 2. Validate market state: must exist, be Active, and not be expired
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }
        if env.ledger().timestamp() > market.end_time {
            return Err(ContractError::MarketExpired);
        }

        // 3. Validate the market price up front for a clear ContractError
        validation::validate_market_price(market_price)?;

        // 4. Enforce that deposited collateral covers any increase in the lock,
        //    and that a market closed to deposits cannot be used to open new
        //    exposure. Trades that keep the lock flat or reduce it (selling /
        //    closing out an existing position) are always allowed, even when
        //    closed_to_deposits is set, since they reduce risk rather than add
        //    it — only opening/increasing a position is "last-minute" new
        //    exposure. Negative-share deltas are left for
        //    positions::update_position to reject (it also emits a
        //    PositionLimitExceeded event).
        let existing_position = storage::get_position(&env, market_id, &user)?;
        let position = existing_position
            .clone()
            .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));
        let new_yes = position.yes_shares + yes_delta;
        let new_no = position.no_shares + no_delta;
        if new_yes >= 0 && new_no >= 0 {
            let prospective_locked =
                positions::calculate_locked_collateral(new_yes, new_no, market_price);
            let lock_increased = prospective_locked > position.locked_collateral;
            if lock_increased && market.closed_to_deposits {
                return Err(ContractError::MarketClosedToDeposits);
            }
            // Protocol-wide collateral check (ADR-002, issue #685): collateral
            // is a single balance per user shared across every market
            // (`storage::CollateralBalance`) rather than siloed per market.
            // A trade is only rejected here when it would *increase* this
            // market's lock beyond what the user's balance can cover once
            // collateral already locked in every *other* market
            // (`storage::TotalLockedCollateral` minus this market's current
            // lock) is accounted for.
            let locked_elsewhere = storage::get_total_locked_collateral(&env, &user)
                .saturating_sub(position.locked_collateral);
            if lock_increased {
                let protocol_balance = storage::get_collateral_balance(&env, &user);
                if positions::check_protocol_collateral(
                    prospective_locked,
                    protocol_balance,
                    locked_elsewhere,
                )
                .is_err()
                {
                    return Err(ContractError::InsufficientCollateral);
                }
            }
            // Keep the protocol-wide aggregate in sync so other markets see
            // this market's updated lock immediately.
            storage::set_total_locked_collateral(
                &env,
                &user,
                locked_elsewhere.saturating_add(prospective_locked),
            );
        }

        // 5. Apply the share deltas (persists the position and emits an event)
        let result =
            positions::update_position(&env, market_id, &user, yes_delta, no_delta, market_price)
                .map_err(|e| match e {
                positions::PositionError::ShareBalanceBelowZero => {
                    ContractError::InvalidShareAmount
                }
                positions::PositionError::InvalidMarketPrice => ContractError::InvalidPrice,
            })?;

        // 5a. Track first-time participants so the market can later be
        //     settled page-by-page via `settle_positions_page` (Issue #495)
        //     without requiring an off-chain index of every trader.
        if existing_position.is_none() {
            storage::add_market_participant(&env, market_id, &user);
        }

        // 5b. Mint or burn outcome tokens for the updated position.
        if let Some(outcome_token_address) = storage::get_outcome_token_contract(&env) {
            let token_client = OutcomeTokenContractClient::new(&env, &outcome_token_address);
            if yes_delta > 0 {
                token_client.mint(&market_id, &user, &TokenKind::Yes, &yes_delta);
            } else if yes_delta < 0 {
                token_client.burn(&market_id, &user, &TokenKind::Yes, &(-yes_delta));
            }

            if no_delta > 0 {
                token_client.mint(&market_id, &user, &TokenKind::No, &no_delta);
            } else if no_delta < 0 {
                token_client.burn(&market_id, &user, &TokenKind::No, &(-no_delta));
            }
        }

        // 6. Persist the updated price so withdraw and other callers see it
        market.price_bps = market_price;
        storage::set_market(&env, market_id, &market)?;

        Ok(result)
    }

    /// Settle a user's position in a resolved market and pay out their winnings.
    ///
    /// Completes the deposit -> resolve -> settle -> receive-funds loop: it
    /// calculates the payout for the resolved outcome, marks the position
    /// settled, and transfers the payout in collateral (SAC) tokens from the
    /// contract to the user.
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
    /// - [`ContractError::NoPositionFound`] - the user has no position
    /// - [`ContractError::MarketNotResolved`] - the market is not resolved
    /// - [`ContractError::PositionAlreadySettled`] - already settled
    ///
    /// # Events
    /// Emits `PositionSettled` with the payout amount.
    pub fn settle_position(env: Env, user: Address, market_id: u32) -> Result<i128, ContractError> {
        validation::require_not_paused(&env)?;
        settlement::settle_position(&env, &user, market_id)
    }

    /// Settle multiple users' positions in a resolved market in one call.
    ///
    /// This is a batched variant of [`settle_position`] intended for operators
    /// settling many users at once (e.g. a cron job after resolution). Each
    /// user is settled independently; already-settled or missing positions are
    /// silently skipped so a single bad entry does not abort the whole batch.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market identifier (must be resolved)
    /// * `users` - Addresses to settle
    ///
    /// # Returns
    /// Total collateral (in stroops) transferred across all settled positions.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] - the market does not exist
    /// - [`ContractError::MarketNotResolved`] - the market is not resolved
    ///
    /// # Events
    /// Emits `PositionSettled` for each successfully settled position.
    pub fn batch_settle_positions(
        env: Env,
        market_id: u32,
        users: soroban_sdk::Vec<Address>,
    ) -> Result<i128, ContractError> {
        validation::require_not_paused(&env)?;
        settlement::batch_settle_positions(&env, market_id, users)
    }

    /// Settle a bounded page of a resolved market's participants (Issue #495).
    ///
    /// Unlike [`batch_settle_positions`], the caller does not supply the user
    /// list — it is drawn from the on-chain participant registry that
    /// [`Self::update_position`] maintains, so a market with more positions
    /// than fit one transaction's resource budget can be fully settled by
    /// repeated calls, advancing `start_index` to the returned next index
    /// each time until `is_complete` is `true`.
    ///
    /// # Returns
    /// `(total_payout_this_page, next_index, is_complete)`
    pub fn settle_positions_page(
        env: Env,
        market_id: u32,
        start_index: u32,
        limit: u32,
    ) -> Result<(i128, u32, bool), ContractError> {
        validation::require_not_paused(&env)?;
        settlement::settle_positions_page(&env, market_id, start_index, limit)
    }

    /// Number of distinct addresses that have ever held a position in a market.
    ///
    /// Useful to determine how many [`Self::settle_positions_page`] calls are
    /// needed to fully settle a resolved market.
    pub fn get_market_participant_count(env: Env, market_id: u32) -> u32 {
        storage::get_market_participant_count(&env, market_id)
    }

    /// Register the treasury contract address for protocol fee routing.
    ///
    /// Once set, any non-zero withdrawal fee computed during
    /// [`withdraw_unused_collateral`] will be transferred to this address and
    /// recorded via the treasury's `collect_fee` entry point.
    ///
    /// Only the stored admin may call this.
    ///
    /// Propose a new treasury contract address, subject to a timelock.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin.
    pub fn propose_treasury_contract(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS; // Use same timelock duration
        storage::set_pending_treasury(
            &env,
            &crate::types::PendingAddressChange {
                new_address: treasury.clone(),
                effective_at,
            },
        );
        events::emit_treasury_proposed(&env, &treasury, effective_at);
        Ok(())
    }

    /// Execute a previously proposed treasury contract change.
    pub fn execute_treasury_contract(env: Env) -> Result<Address, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        let pending =
            storage::get_pending_treasury(&env).ok_or(ContractError::NoPendingFeeChange)?; // We can add NoPendingChange later, reusing NoPendingFeeChange for now

        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        storage::set_treasury(&env, &pending.new_address);
        storage::clear_pending_treasury(&env);
        events::emit_treasury_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    /// Cancel a pending treasury contract change.
    pub fn cancel_treasury_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        storage::clear_pending_treasury(&env);
        // We could emit a cancel event here
        Ok(())
    }

    /// Propose a new withdrawal fee rate in basis points (0–10_000), subject
    /// to a timelock (Issue #496) before it takes effect.
    ///
    /// Only the stored admin may call this. The change does not apply
    /// immediately — call [`Self::execute_fee_rate_change`] once
    /// [`FEE_RATE_TIMELOCK_SECONDS`] have elapsed to actually apply it.
    /// Proposing again before the pending change executes overwrites it with
    /// a freshly-timed change.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::InvalidPrice`] — `fee_rate_bps` outside 0–10_000.
    /// - [`ContractError::FeeCapExceeded`] — `fee_rate_bps` exceeds the fee cap.
    pub fn set_fee_rate(env: Env, admin: Address, fee_rate_bps: i128) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        validation::validate_fee_rate_bps(fee_rate_bps)?;
        let cap = storage::get_fee_cap_bps(&env);
        if fee_rate_bps > cap {
            return Err(ContractError::FeeCapExceeded);
        }

        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS;
        storage::set_pending_fee_rate_change(
            &env,
            &crate::types::PendingFeeRateChange {
                new_rate_bps: fee_rate_bps,
                effective_at,
            },
        );
        events::emit_fee_rate_change_proposed(&env, fee_rate_bps, effective_at);
        Ok(())
    }

    /// Apply a previously-proposed fee rate change once its timelock has
    /// elapsed (Issue #496). Callable by anyone — the timelock itself is the
    /// access control; there is nothing sensitive about who triggers it.
    ///
    /// # Errors
    /// - [`ContractError::NoPendingFeeChange`] — no change is currently pending.
    /// - [`ContractError::TimelockNotElapsed`] — `effective_at` has not passed yet.
    /// - [`ContractError::FeeCapExceeded`] — the pending rate exceeds the
    ///   *current* fee cap. The cap is re-checked here (not just at proposal
    ///   time in [`Self::set_fee_rate`]) so a cap lowered by the admin while a
    ///   change is in flight cannot let a stale, now-excessive rate through.
    pub fn execute_fee_rate_change(env: Env) -> Result<i128, ContractError> {
        // Guard: contract must be fully initialized before a pending fee-rate
        // change can be applied (Issue #547). Without this check a caller could
        // invoke execute_fee_rate_change on a contract where the admin has not
        // yet been set, writing FeeRateBps storage before initialization is
        // complete and leaving the contract in an inconsistent state.
        // This is the same guard used by every other state-mutating entry point.
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;

        let pending =
            storage::get_pending_fee_rate_change(&env).ok_or(ContractError::NoPendingFeeChange)?;
        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }
        let cap = storage::get_fee_cap_bps(&env);
        if pending.new_rate_bps > cap {
            return Err(ContractError::FeeCapExceeded);
        }
        storage::set_fee_rate_bps(&env, pending.new_rate_bps);
        storage::clear_pending_fee_rate_change(&env);
        events::emit_fee_rate_change_executed(&env, pending.new_rate_bps, env.ledger().timestamp());
        Ok(pending.new_rate_bps)
    }

    /// Cancel a pending fee rate change before it takes effect.
    ///
    /// Only the stored admin may call this. Clears the pending change set by
    /// [`Self::set_fee_rate`] so it can no longer be applied via
    /// [`Self::execute_fee_rate_change`].
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::NoPendingFeeChange`] — no change is pending.
    pub fn cancel_fee_rate_change(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::get_pending_fee_rate_change(&env).ok_or(ContractError::NoPendingFeeChange)?;
        storage::clear_pending_fee_rate_change(&env);
        Ok(())
    }

    /// Set the hard upper bound on the withdrawal fee rate, in basis points
    /// (0–10_000). Enforced both when a new rate is proposed
    /// ([`Self::set_fee_rate`]) and when a pending change is applied
    /// ([`Self::execute_fee_rate_change`]).
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::InvalidPrice`] — `cap_bps` is outside 0–10_000.
    pub fn set_fee_cap(env: Env, admin: Address, cap_bps: i128) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        validation::validate_fee_rate_bps(cap_bps)?;
        storage::set_fee_cap_bps(&env, cap_bps);
        Ok(())
    }

    /// Return the currently pending fee rate change, if any (Issue #496).
    pub fn get_pending_fee_rate_change(env: Env) -> Option<crate::types::PendingFeeRateChange> {
        storage::get_pending_fee_rate_change(&env)
    }

    // ========== Fee Waiver List (#483) ==========

    /// Add `account` to the admin-managed fee waiver list.
    ///
    /// Waived addresses pay no withdrawal fee regardless of the configured
    /// [`set_fee_rate`]. Adding an address that is already waived is a no-op.
    ///
    /// Only the stored admin may call this. `account` must be an ordinary
    /// user account: contract addresses are rejected, and the admin cannot
    /// waive itself (#584) — see [`validation::validate_fee_waiver_account`].
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::InvalidFeeWaiverAccount`] — `account` is a contract
    ///   address or equals the admin.
    pub fn add_fee_waiver(env: Env, admin: Address, account: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        validation::validate_fee_waiver_account(&account, &stored_admin)?;
        storage::add_fee_waiver(&env, &account);
        events::emit_fee_waiver_added(&env, &account, &admin);
        Ok(())
    }

    /// Remove `account` from the admin-managed fee waiver list.
    ///
    /// Removing an address that is not currently waived is a no-op.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn remove_fee_waiver(
        env: Env,
        admin: Address,
        account: Address,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::remove_fee_waiver(&env, &account);
        events::emit_fee_waiver_removed(&env, &account, &admin);
        Ok(())
    }

    /// Return whether `account` is currently exempt from withdrawal fees.
    pub fn is_fee_waived(env: Env, account: Address) -> bool {
        storage::is_fee_waived(&env, &account)
    }

    /// Return the full list of addresses currently exempt from withdrawal fees.
    pub fn get_fee_waivers(env: Env) -> soroban_sdk::Vec<Address> {
        storage::get_fee_waivers(&env)
    }

    // ========== Oracle Pubkey Rotation (#486) ==========

    /// Rotate the oracle public key used to verify resolution signatures for
    /// `market_id`.
    ///
    /// Intended for recovery from a compromised or retired oracle signing key
    /// without requiring the market to be recreated. Only permitted while the
    /// market is still [`MarketStatus::Active`] — a resolved or canceled
    /// market has no further use for oracle verification.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `admin` - Must be the stored admin address (authorizes the call)
    /// * `market_id` - Identifier of the market whose oracle key is rotated
    /// * `new_oracle_pubkey` - Replacement Ed25519 oracle public key
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin.
    /// - [`ContractError::MarketNotFound`] – the market does not exist.
    /// - [`ContractError::MarketNotActive`] – the market is resolved or canceled.
    /// - [`ContractError::InvalidSignature`] – `new_oracle_pubkey` is the
    ///   all-zero key, which can never produce a valid Ed25519 signature.
    ///
    /// # Events
    /// Emits [`events::MarketOracleUpdated`] with the old and new oracle keys.
    pub fn propose_market_oracle(
        env: Env,
        admin: Address,
        market_id: u32,
        new_oracle_pubkey: BytesN<32>,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        // Guard: an all-zero pubkey can never produce a valid Ed25519 signature
        if new_oracle_pubkey == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(ContractError::InvalidSignature);
        }

        let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }

        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS;
        storage::set_pending_market_oracle(
            &env,
            market_id,
            &crate::types::PendingBytesNChange {
                new_bytes: new_oracle_pubkey.clone(),
                effective_at,
            },
        );

        events::emit_market_oracle_proposed(
            &env,
            market_id,
            &admin,
            &market.oracle_pubkey,
            &new_oracle_pubkey,
            effective_at,
        );

        Ok(())
    }

    pub fn execute_market_oracle(env: Env, market_id: u32) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        let pending = storage::get_pending_market_oracle(&env, market_id)
            .ok_or(ContractError::NoPendingFeeChange)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }

        let old_oracle_pubkey = market.oracle_pubkey.clone();
        market.oracle_pubkey = pending.new_bytes.clone();
        storage::set_market(&env, market_id, &market)?;
        storage::clear_pending_market_oracle(&env, market_id);

        events::emit_market_oracle_updated(
            &env,
            market_id,
            &storage::get_admin(&env)?,
            &old_oracle_pubkey,
            &pending.new_bytes,
            env.ledger().timestamp(),
        );

        Ok(())
    }

    pub fn cancel_market_oracle(
        env: Env,
        admin: Address,
        market_id: u32,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::clear_pending_market_oracle(&env, market_id);
        Ok(())
    }

    /// Configure the multi-signer quorum for threshold-based resolution (#378).
    ///
    /// `signers` is the ordered set of oracle public keys. `quorum` is the
    /// minimum number of valid signatures required by `resolve_market_threshold`.
    /// Setting `quorum` to 0 or passing an empty `signers` list effectively
    /// disables threshold resolution.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::InvalidThresholdQuorum`] — `quorum` exceeds
    ///   `signers.len()`; such a quorum could never be satisfied.
    ///
    /// Propose a threshold signer set and quorum update, subject to timelock (#665).
    ///
    /// This is now the *only* production path to change the global threshold
    /// signer set — the legacy instant `set_threshold_signers` entrypoint was
    /// removed (#684) because it let an admin bypass the
    /// [`FEE_RATE_TIMELOCK_SECONDS`] (172,800s / 48h) delay enforced here and
    /// in [`Self::execute_threshold_signers`].
    pub fn propose_threshold_signers(
        env: Env,
        admin: Address,
        signers: soroban_sdk::Vec<BytesN<32>>,
        quorum: u32,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        let signers_len = signers.len();
        if quorum == 0 || signers_len == 0 || quorum > signers_len {
            return Err(ContractError::InvalidThresholdQuorum);
        }
        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS;
        storage::set_pending_threshold_signers(
            &env,
            &crate::types::PendingThresholdSignersChange {
                signers,
                quorum,
                effective_at,
            },
        );
        Ok(())
    }

    /// Execute a previously proposed threshold signers update (#665).
    pub fn execute_threshold_signers(env: Env) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        let pending = storage::get_pending_threshold_signers(&env)
            .ok_or(ContractError::NoPendingFeeChange)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        let signers_len = pending.signers.len();
        if pending.quorum == 0 || signers_len == 0 || pending.quorum > signers_len {
            return Err(ContractError::InvalidThresholdQuorum);
        }

        storage::set_threshold_signers(&env, &pending.signers);
        storage::set_threshold_quorum(&env, pending.quorum);
        storage::clear_pending_threshold_signers(&env);
        Ok(())
    }

    /// Cancel a pending threshold signers update (#665).
    pub fn cancel_threshold_signers(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::clear_pending_threshold_signers(&env);
        Ok(())
    }

    /// Configure per-market threshold signers and quorum override (#665).
    pub fn set_market_threshold_signers(
        env: Env,
        admin: Address,
        market_id: u32,
        signers: soroban_sdk::Vec<BytesN<32>>,
        quorum: u32,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        let market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }
        let signers_len = signers.len();
        if quorum == 0 || signers_len == 0 || quorum > signers_len {
            return Err(ContractError::InvalidThresholdQuorum);
        }
        storage::set_market_threshold_signers(&env, market_id, &signers);
        storage::set_market_threshold_quorum(&env, market_id, quorum);
        Ok(())
    }

    /// Return the current threshold signer set.
    pub fn get_threshold_signers(env: Env) -> soroban_sdk::Vec<BytesN<32>> {
        storage::get_threshold_signers(&env)
    }

    /// Return the current quorum requirement.
    pub fn get_threshold_quorum(env: Env) -> u32 {
        storage::get_threshold_quorum(&env)
    }

    /// Resolve a market using a quorum of oracle signatures (#378).
    ///
    /// Callers provide one signature per registered signer (use 64 zero bytes
    /// for signers whose signature is unavailable). The market resolves once
    /// the valid-signature count reaches the stored quorum.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] — market does not exist.
    /// - [`ContractError::MarketAlreadyResolved`] — already resolved.
    /// - [`ContractError::UnauthorizedOracle`] — no signers/quorum configured.
    /// - [`ContractError::InvalidSignature`] — fewer than quorum valid sigs.
    /// If a resolution contract is registered, this returns
    /// [`ContractError::ResolutionNotFinalized`]. Challenge-based and
    /// threshold-based resolution are intentionally mutually exclusive.
    pub fn resolve_market_threshold(
        env: Env,
        resolver: Address,
        market_id: u32,
        outcome: bool,
        signatures: soroban_sdk::Vec<BytesN<64>>,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        resolver.require_auth();

        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        // Threshold resolution and challenge-window resolution are explicit,
        // mutually exclusive modes. Once a resolution contract is registered,
        // every resolution must pass through its propose/challenge/finalize
        // lifecycle and the single-signature callback guarded by
        // `require_resolution_finalized`. Allowing this entry point as well
        // would let a quorum bypass an open or challenged candidate.
        require_threshold_resolution_mode(&env)?;

        let signers = storage::get_threshold_signers(&env);
        let quorum = storage::get_threshold_quorum(&env);

        oracle::verify_threshold_signatures(
            &env,
            market_id,
            outcome,
            &signers,
            &signatures,
            quorum,
        )?;
        events::emit_oracle_signature_verified(&env, market_id, outcome, env.ledger().timestamp());

        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);
        market.resolver = Some(resolver.clone());
        let resolved_at = env.ledger().timestamp();
        market.resolved_at = Some(resolved_at);
        storage::set_market(&env, market_id, &market)?;

        events::emit_market_resolved(
            &env,
            market_id,
            &market.oracle_pubkey,
            &resolver,
            outcome,
            resolved_at,
        );

        Ok(())
    }

    /// Resolve a market using V2 threshold signatures (#665).
    pub fn resolve_market_threshold_v2(
        env: Env,
        resolver: Address,
        market_id: u32,
        outcome: bool,
        valid_until: u64,
        epoch: u32,
        signatures: soroban_sdk::Vec<BytesN<64>>,
        passphrase_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        validation::require_not_paused(&env)?;
        resolver.require_auth();

        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        if env.ledger().timestamp() > valid_until {
            return Err(ContractError::OracleMessageExpired);
        }

        let signers = storage::get_market_threshold_signers(&env, market_id)
            .unwrap_or_else(|| storage::get_threshold_signers(&env));
        let quorum = storage::get_market_threshold_quorum(&env, market_id)
            .unwrap_or_else(|| storage::get_threshold_quorum(&env));

        oracle::verify_threshold_signatures_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
            &signers,
            &signatures,
            quorum,
        )?;
        events::emit_oracle_signature_verified(&env, market_id, outcome, env.ledger().timestamp());

        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);
        market.resolver = Some(resolver.clone());
        let resolved_at = env.ledger().timestamp();
        market.resolved_at = Some(resolved_at);
        storage::set_market(&env, market_id, &market)?;

        events::emit_market_resolved(
            &env,
            market_id,
            &market.oracle_pubkey,
            &resolver,
            outcome,
            resolved_at,
        );

        Ok(())
    }

    /// Return the current withdrawal fee rate in basis points.
    ///
    /// Returns 0 if no fee rate has been configured.
    pub fn get_fee_rate(env: Env) -> i128 {
        storage::get_fee_rate_bps(&env)
    }

    /// Register the deployed outcome-token contract address used by this
    /// market contract to mint and burn outcome tokens for position updates.
    ///
    /// Only the stored admin may call this.
    pub fn propose_outcome_token_contract(
        env: Env,
        admin: Address,
        outcome_token_contract: Address,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS;
        storage::set_pending_outcome_token_contract(
            &env,
            &crate::types::PendingAddressChange {
                new_address: outcome_token_contract.clone(),
                effective_at,
            },
        );
        events::emit_outcome_token_proposed(&env, &outcome_token_contract, effective_at);
        Ok(())
    }

    pub fn execute_outcome_token_contract(env: Env) -> Result<Address, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        let pending = storage::get_pending_outcome_token_contract(&env)
            .ok_or(ContractError::NoPendingFeeChange)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        storage::set_outcome_token_contract(&env, &pending.new_address);
        storage::clear_pending_outcome_token_contract(&env);
        // We reuse the set event logic if there is one, or add a new one
        events::emit_outcome_token_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_outcome_token_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::clear_pending_outcome_token_contract(&env);
        Ok(())
    }

    /// Return a market's current [`MarketStatus`].
    ///
    /// Exposed as a lightweight cross-contract read for companion contracts
    /// (e.g. the outcome-token contract's `transfer`, which must gate
    /// peer-to-peer transfers on the market having resolved) that cannot
    /// depend on this crate directly.
    pub fn get_market_status(env: Env, market_id: u32) -> MarketStatus {
        storage::get_market(&env, market_id)
            .ok()
            .flatten()
            .expect("market not found")
            .status
    }

    /// Return a market's collateral (SAC) token address.
    ///
    /// Exposed as a lightweight cross-contract read for companion contracts
    /// (e.g. the resolution contract, which locks proposer bonds in the same
    /// token as the market's collateral).
    pub fn get_collateral_token(env: Env, market_id: u32) -> Address {
        storage::get_market(&env, market_id)
            .ok()
            .flatten()
            .expect("market not found")
            .collateral_token
    }

    /// Return the full [`Market`] struct for a given market ID.
    ///
    /// This is the canonical read endpoint for off-chain consumers (indexers,
    /// the web app, and companion contracts) that need the complete market
    /// snapshot — status, fees, `closed_to_deposits`, price, resolver, etc.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – no market exists for `market_id`.
    pub fn get_market(env: Env, market_id: u32) -> Result<Market, ContractError> {
        storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)
    }

    /// Return the registered outcome-token contract address, if any.
    pub fn get_outcome_token_contract(env: Env) -> Option<Address> {
        storage::get_outcome_token_contract(&env)
    }

    /// Compare a user's stored `Position` shares against their
    /// `OutcomeToken` balances for `market_id` (dual-ledger reconciliation
    /// view — see `reconciliation` module docs).
    ///
    /// Read-only; callable by anyone (auditors, indexers, or a user checking
    /// their own account). When no outcome-token contract is registered the
    /// two ledgers cannot diverge, so parity is always reported as matched.
    pub fn get_position_token_parity(
        env: Env,
        market_id: u32,
        user: Address,
    ) -> Result<reconciliation::PositionTokenParity, ContractError> {
        reconciliation::get_position_token_parity(&env, market_id, &user)
    }

    /// Admin-gated repair for a Position/OutcomeToken divergence detected via
    /// [`Self::get_position_token_parity`] (or surfaced as a
    /// [`ContractError::PositionTokenMismatch`] rejection from
    /// `update_position`/`settle_position`).
    ///
    /// Mints or burns the user's `OutcomeToken` balances so they match the
    /// stored `Position` — `Position` is the source of truth (see
    /// `reconciliation` module docs). No-op if the ledgers already agree.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    ///
    /// # Events
    /// Emits `PositionTokensReconciled` with the signed mint/burn deltas
    /// applied, whenever a repair actually occurs.
    pub fn reconcile_position_tokens(
        env: Env,
        admin: Address,
        market_id: u32,
        user: Address,
    ) -> Result<reconciliation::PositionTokenParity, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        reconciliation::reconcile_position_tokens(&env, &admin, market_id, &user)
    }

    /// Register the resolution contract that gates `resolve_market`.
    ///
    /// When set, `resolve_market` will call into this contract to verify that
    /// a finalized candidate exists for the market before accepting a resolution.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin.
    pub fn propose_resolution_contract(
        env: Env,
        admin: Address,
        resolution_contract: Address,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        let effective_at = env.ledger().timestamp() + FEE_RATE_TIMELOCK_SECONDS;
        storage::set_pending_resolution_contract(
            &env,
            &crate::types::PendingAddressChange {
                new_address: resolution_contract.clone(),
                effective_at,
            },
        );
        events::emit_resolution_proposed(&env, &resolution_contract, effective_at);
        Ok(())
    }

    pub fn execute_resolution_contract(env: Env) -> Result<Address, ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        let pending = storage::get_pending_resolution_contract(&env)
            .ok_or(ContractError::NoPendingFeeChange)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        storage::set_resolution_contract(&env, &pending.new_address);
        storage::clear_pending_resolution_contract(&env);
        events::emit_resolution_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_resolution_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::clear_pending_resolution_contract(&env);
        Ok(())
    }

    /// Return the registered resolution contract address, if any.
    pub fn get_resolution_contract(env: Env) -> Option<Address> {
        storage::get_resolution_contract(&env)
    }

    // ========== Trading Convenience Functions ==========

    /// Buy YES shares in a market at the specified price.
    ///
    /// This is a convenience wrapper around [`update_position`] for the common
    /// case of buying only YES shares. Equivalent to calling `update_position`
    /// with `yes_delta > 0` and `no_delta = 0`.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User buying shares (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `amount` - Number of YES shares to buy (must be positive)
    /// * `market_price` - Current market price in basis points (0–10_000)
    ///
    /// # Returns
    /// The updated [`Position`] after the purchase.
    ///
    /// # Errors
    /// Same as [`update_position`], plus:
    /// - [`ContractError::InvalidQuantity`] – amount is zero or negative
    ///
    /// # Example
    /// ```ignore
    /// // Buy 100 YES shares at 60% price
    /// let position = client.buy_yes(
    ///     &user,
    ///     &market_id,
    ///     &(100 * STROOPS_PER_USDC),
    ///     &6_000i128,
    /// );
    /// ```
    pub fn buy_yes(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidQuantity);
        }
        Self::update_position(env, user, market_id, amount, 0, market_price)
    }

    /// Buy NO shares in a market at the specified price.
    ///
    /// This is a convenience wrapper around [`update_position`] for the common
    /// case of buying only NO shares. Equivalent to calling `update_position`
    /// with `yes_delta = 0` and `no_delta > 0`.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User buying shares (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `amount` - Number of NO shares to buy (must be positive)
    /// * `market_price` - Current market price in basis points (0–10_000)
    ///
    /// # Returns
    /// The updated [`Position`] after the purchase.
    ///
    /// # Errors
    /// Same as [`update_position`], plus:
    /// - [`ContractError::InvalidQuantity`] – amount is zero or negative
    ///
    /// # Example
    /// ```ignore
    /// // Buy 100 NO shares at 40% price (60% YES implies 40% NO)
    /// let position = client.buy_no(
    ///     &user,
    ///     &market_id,
    ///     &(100 * STROOPS_PER_USDC),
    ///     &6_000i128,
    /// );
    /// ```
    pub fn buy_no(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidQuantity);
        }
        Self::update_position(env, user, market_id, 0, amount, market_price)
    }

    /// Sell YES shares in a market at the specified price.
    ///
    /// This is a convenience wrapper around [`update_position`] for the common
    /// case of selling only YES shares. Equivalent to calling `update_position`
    /// with `yes_delta < 0` and `no_delta = 0`.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User selling shares (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `amount` - Number of YES shares to sell (must be positive; internally negated)
    /// * `market_price` - Current market price in basis points (0–10_000)
    ///
    /// # Returns
    /// The updated [`Position`] after the sale.
    ///
    /// # Errors
    /// Same as [`update_position`], plus:
    /// - [`ContractError::InvalidQuantity`] – amount is zero or negative
    ///
    /// # Example
    /// ```ignore
    /// // Sell 50 YES shares
    /// let position = client.sell_yes(
    ///     &user,
    ///     &market_id,
    ///     &(50 * STROOPS_PER_USDC),
    ///     &6_000i128,
    /// );
    /// ```
    pub fn sell_yes(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidQuantity);
        }
        Self::update_position(env, user, market_id, -amount, 0, market_price)
    }

    /// Sell NO shares in a market at the specified price.
    ///
    /// This is a convenience wrapper around [`update_position`] for the common
    /// case of selling only NO shares. Equivalent to calling `update_position`
    /// with `yes_delta = 0` and `no_delta < 0`.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User selling shares (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `amount` - Number of NO shares to sell (must be positive; internally negated)
    /// * `market_price` - Current market price in basis points (0–10_000)
    ///
    /// # Returns
    /// The updated [`Position`] after the sale.
    ///
    /// # Errors
    /// Same as [`update_position`], plus:
    /// - [`ContractError::InvalidQuantity`] – amount is zero or negative
    ///
    /// # Example
    /// ```ignore
    /// // Sell 50 NO shares
    /// let position = client.sell_no(
    ///     &user,
    ///     &market_id,
    ///     &(50 * STROOPS_PER_USDC),
    ///     &6_000i128,
    /// );
    /// ```
    pub fn sell_no(
        env: Env,
        user: Address,
        market_id: u32,
        amount: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidQuantity);
        }
        Self::update_position(env, user, market_id, 0, -amount, market_price)
    }

    // ========== View Functions ==========

    /// Get a user's current position in a market.
    ///
    /// Returns position details including share balances, locked collateral,
    /// and settlement status. This is a read-only query function.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market identifier
    /// * `user` - User address to query
    ///
    /// # Returns
    /// `Some(Position)` if the user has ever traded or deposited in this
    /// market, `None` if the market exists but the user has no position yet.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – `market_id` does not correspond
    ///   to any market. This is checked explicitly so that querying a typo'd
    ///   or never-created `market_id` fails clearly instead of being
    ///   indistinguishable from "market exists, user has no position" (both
    ///   would otherwise return `Ok(None)`).
    ///
    /// # Example
    /// ```ignore
    /// match client.try_get_position(&market_id, &user) {
    ///     Ok(Ok(Some(position))) => { /* has a position */ }
    ///     Ok(Ok(None)) => { /* market exists, no position yet */ }
    ///     Ok(Err(ContractError::MarketNotFound)) => { /* bad market_id */ }
    ///     _ => {}
    /// }
    /// ```
    pub fn get_position(
        env: Env,
        market_id: u32,
        user: Address,
    ) -> Result<Option<Position>, ContractError> {
        if !storage::has_market(&env, market_id)? {
            return Err(ContractError::MarketNotFound);
        }
        storage::get_position(&env, market_id, &user)
    }

    /// Return a user's net position across YES and NO shares in a market.
    ///
    /// Positive => net long YES, negative => net long NO, zero => hedged (or
    /// no position at all — a user with no stored `Position` is treated as
    /// fully hedged at `0`).
    ///
    /// # Arguments
    /// * `market_id` - Market identifier
    /// * `user` - User address to query
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – `market_id` does not correspond
    ///   to any market (see [`Self::get_position`] for the rationale).
    ///
    /// # Example
    /// ```ignore
    /// let net = client.get_net_position(&market_id, &user);
    /// // net > 0  => user is net long YES
    /// // net < 0  => user is net long NO
    /// ```
    pub fn get_net_position(
        env: Env,
        market_id: u32,
        user: Address,
    ) -> Result<i128, ContractError> {
        if !storage::has_market(&env, market_id)? {
            return Err(ContractError::MarketNotFound);
        }
        let position = storage::get_position(&env, market_id, &user)?;
        Ok(match position {
            Some(p) => positions::calculate_net_position(p.yes_shares, p.no_shares),
            None => 0,
        })
    }

    /// Return the current fee cap in basis points (defaults to 10_000 when unset).
    pub fn get_fee_cap(env: Env) -> i128 {
        storage::get_fee_cap_bps(&env)
    }

    /// Return a paginated slice of markets ordered by creation.
    ///
    /// # Arguments
    /// * `start` - Zero-based index into the ordered list of markets.
    /// * `limit` - Maximum number of markets to return (capped at 100).
    ///
    /// # Returns
    /// A `Vec<Market>` of up to `limit` markets starting at `start`.
    /// Returns an empty vec when `start` is beyond the end of the list.
    pub fn list_markets(
        env: Env,
        start: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<crate::types::Market>, ContractError> {
        let ids = storage::get_market_ids(&env);
        let total = ids.len();
        let limit = limit.min(100);
        let mut result = soroban_sdk::Vec::new(&env);
        let end = (start + limit).min(total);
        let mut i = start;
        while i < end {
            let market_id = ids.get(i).unwrap();
            if let Some(market) = storage::get_market(&env, market_id)? {
                result.push_back(market);
            }
            i += 1;
        }
        Ok(result)
    }

    /// Prevent new collateral deposits into a market, and block
    /// [`update_position`] calls that would open new exposure, while
    /// preserving withdrawals and settlement.
    ///
    /// Once closed:
    /// - Any call to [`deposit_collateral`] for this market returns
    ///   [`ContractError::MarketClosedToDeposits`].
    /// - Any call to [`update_position`] that would *increase* a user's
    ///   locked collateral (opening or growing a position) also returns
    ///   [`ContractError::MarketClosedToDeposits`]. Trades that keep the lock
    ///   flat or reduce it (selling shares, closing out a position) still
    ///   succeed, since they shed risk rather than add it.
    /// - [`withdraw_unused_collateral`] and settlement are unaffected.
    ///
    /// The flag is idempotent — calling this on an already-closed market is a
    /// no-op and succeeds without error.
    ///
    /// # Use cases
    /// - Lock down a market approaching its expiry to prevent last-minute
    ///   position changes.
    /// - Halt new deposits during a resolution or dispute window.
    ///
    /// # Arguments
    /// * `env`       – Soroban contract environment
    /// * `admin`     – Address that must match the stored contract admin
    /// * `market_id` – Unique identifier of the market to close
    ///
    /// # Errors
    /// - [`ContractError::NotInitialized`] – contract has not been initialised yet
    /// - [`ContractError::NotAdmin`]       – `admin` is not the stored admin
    /// - [`ContractError::MarketNotFound`] – no market with `market_id` exists
    ///
    /// # Events
    /// Emits [`events::MarketClosedToDeposits`] with `market_id`,
    /// `admin`, and `closed_at` timestamp so off-chain indexers can track
    /// when a market was locked.
    pub fn close_market_to_deposits(
        env: Env,
        admin: Address,
        market_id: u32,
    ) -> Result<(), ContractError> {
        validation::require_initialized(&env)?;
        validation::require_not_paused(&env)?;
        admin.require_auth();

        // Only the stored admin may close a market to deposits.
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }

        // Load the market — returns MarketNotFound for an unknown market_id.
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;

        // Idempotent: if already closed, nothing to do.
        if !market.closed_to_deposits {
            market.closed_to_deposits = true;
            storage::set_market(&env, market_id, &market)?;
        }

        // Always emit the event so indexers can observe every admin call,
        // including redundant ones (useful for audit trails).
        let closed_at = env.ledger().timestamp();
        events::emit_market_closed_to_deposits(&env, market_id, &admin, closed_at);

        Ok(())
    }
}

/// Enforce the resolution-contract gate for `resolve_market`.
///
/// If no resolution contract is registered (`storage::get_resolution_contract`
/// returns `None`), this is a no-op and `resolve_market` behaves exactly as
/// it did before the gate existed — a valid oracle signature is sufficient.
///
/// When a resolution contract *is* registered, `resolve_market` may only
/// succeed once that contract has recorded a `Finalized` candidate for
/// `market_id` whose `outcome` and `signature` match this call. In practice
/// that means the only caller that can push a resolved market past this
/// check is `ResolutionContract::finalize` itself (it flips the candidate to
/// `Finalized` in its own storage, then immediately invokes
/// `resolve_market` in the same transaction) — any other direct caller
/// (oracle key holder, admin, etc.) is rejected with
/// `ContractError::ResolutionNotFinalized` until finalize() has run.
fn require_resolution_finalized(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let Some(resolution_contract) = storage::get_resolution_contract(env) else {
        return Ok(());
    };

    let client = ResolutionContractClient::new(env, &resolution_contract);
    let candidate_id = client
        .get_candidate_id_for_market(&market_id)
        .ok_or(ContractError::ResolutionNotFinalized)?;
    let candidate = client
        .get_candidate(&candidate_id)
        .ok_or(ContractError::ResolutionNotFinalized)?;

    if candidate.status != ResolutionCandidateStatus::Finalized
        || candidate.outcome != outcome
        || &candidate.signature != signature
    {
        return Err(ContractError::ResolutionNotFinalized);
    }
    Ok(())
}

/// Enforce mutually exclusive market-resolution modes.
///
/// Threshold resolution is available only when no challenge-based resolution
/// contract is registered. Once an admin registers one, its
/// propose/challenge/finalize lifecycle becomes the sole resolution path; the
/// threshold entry point fails closed regardless of whether the current
/// candidate is proposed, challenged, or ready to finalize.
fn require_threshold_resolution_mode(env: &Env) -> Result<(), ContractError> {
    if storage::get_resolution_contract(env).is_some() {
        return Err(ContractError::ResolutionNotFinalized);
    }
    Ok(())
}
