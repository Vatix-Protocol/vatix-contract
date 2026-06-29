#![no_std]

mod deposit;
mod error;
mod events;
pub mod oracle;
#[cfg(feature = "oracle-adapter")]
pub mod oracle_adapter;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
pub mod settlement;
mod withdraw;

#[allow(dead_code)]
pub mod storage;
mod test;
#[cfg(test)]
mod withdraw_fuzz;
pub mod types;
#[allow(dead_code)]
mod validation;

use crate::error::ContractError;
use crate::types::{AdapterType, Market, MarketStatus, Position};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};
use vatix_outcome_token_contract::{OutcomeTokenContractClient, types::TokenKind};
use vatix_resolution_contract::types::CandidateStatus as ResolutionCandidateStatus;

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
    /// Emits [`MarketCreatedEvent`] with `market_id`, `creator`, `question`,
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
        
        // 3. Check if already initialized
        if storage::has_admin(&env) {
            return Err(ContractError::AlreadyInitialized);
        }
        
        // 4. Set admin and version
        storage::set_admin(&env, &admin);
        storage::set_version(&env);
        
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
        // 1. Validate new admin address
        validation::validate_admin_address(&new_admin)?;
        
        // 2. Check contract is initialized
        if !storage::has_admin(&env) {
            return Err(ContractError::NotAdmin);
        }
        
        // 3. Verify current admin
        let stored_admin = storage::get_admin(&env)?;
        if current_admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        
        // 4. Require authorization
        current_admin.require_auth();
        
        // 5. Set pending admin and emit event
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

    pub fn initialize_market(
        env: Env,
        creator: Address,
        question: String,
        end_time: u64,
        oracle_pubkey: BytesN<32>,
        collateral_token: Address,
    ) -> Result<u32, ContractError> {
        // 1. Verify creator is admin
        creator.require_auth();
        let admin = storage::get_admin(&env)?;
        if creator != admin {
            return Err(ContractError::NotAdmin);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;

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
        };

        // 5. Store market
        storage::set_market(&env, market_id, &market)?;

        // 6. Emit event
        events::emit_market_created(&env, market_id, &creator, &question, end_time);

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
        withdraw::withdraw_unused_collateral(env, user, market_id, amount)
    }

    /// Resolve a market with oracle-signed outcome
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market to resolve (decimal string, e.g. "1")
    /// * `outcome` - Outcome (true = YES won, false = NO won)
    /// * `signature` - Oracle's Ed25519 signature (64 bytes)
    ///
    /// # Returns
    /// Unit (success)
    ///
    /// # Errors
    /// - MarketNotFound
    /// - MarketAlreadyResolved
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
    ) -> Result<(), ContractError> {
        resolver.require_auth();
        let market_id = validation::parse_market_id(&market_id)?;
        // Step 1: Load and validate market
        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

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
    /// Emits [`MarketCanceledEvent`] with `market_id`, `canceler`, and
    /// `canceled_at` on success.
    pub fn cancel_market(
        env: Env,
        admin: Address,
        market_id: u32,
    ) -> Result<(), ContractError> {
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
        // 1. Authorization: only the position owner may reclaim their collateral.
        user.require_auth();

        // 2. The reclaim path is exclusive to canceled markets.
        let market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Canceled {
            return Err(ContractError::MarketNotActive);
        }

        // 3. Load the user's position and the full deposited balance.
        let mut position = storage::get_position(&env, market_id, &user)?
            .ok_or(ContractError::NoPositionFound)?;
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

        // 6. Reuse the collateral-withdrawn event so indexers track the refund.
        events::emit_collateral_withdrawn(&env, &user, market_id, refund, position.total_deposited);

        Ok(refund)
    }

    /// Buy or sell YES/NO shares by applying signed deltas to a user's position.
    ///
    /// This is the on-chain entry point for the share-trading logic implemented
    /// in [`positions::update_position`]. It layers the market- and
    /// authorization-level checks required before a position may be mutated.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `user` - User whose position is updated (must authorize the call)
    /// * `market_id` - Market identifier
    /// * `yes_delta` - Change in YES shares (negative to sell)
    /// * `no_delta` - Change in NO shares (negative to sell)
    /// * `market_price` - Current market price in basis points (0–10_000) used
    ///   to value the resulting net position
    ///
    /// # Returns
    /// The updated [`Position`] on success.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] – market does not exist
    /// - [`ContractError::MarketNotActive`] – market is resolved or canceled
    /// - [`ContractError::MarketExpired`] – market has passed its `end_time`
    /// - [`ContractError::InvalidPrice`] – `market_price` is outside 0–10_000
    /// - [`ContractError::InsufficientCollateral`] – deposited collateral does
    ///   not cover the increased locked amount
    /// - [`ContractError::InvalidShareAmount`] – deltas would push a share
    ///   balance below zero
    ///
    /// # Events
    /// Emits `PositionUpdated` on success, or `PositionLimitExceeded` when a
    /// delta would drive a share balance negative.
    pub fn update_position(
        env: Env,
        user: Address,
        market_id: u32,
        yes_delta: i128,
        no_delta: i128,
        market_price: i128,
    ) -> Result<Position, ContractError> {
        // 1. Authorization
        user.require_auth();

        // 2. Validate market state: must exist, be Active, and not be expired
        let mut market = storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status != MarketStatus::Active {
            return Err(ContractError::MarketNotActive);
        }
        if env.ledger().timestamp() > market.end_time {
            return Err(ContractError::MarketExpired);
        }

        // 3. Validate the market price up front for a clear ContractError
        validation::validate_market_price(market_price)?;

        // 4. Enforce that deposited collateral covers any increase in the lock.
        //    Negative-share deltas are left for positions::update_position to
        //    reject (it also emits a PositionLimitExceeded event).
        let position = storage::get_position(&env, market_id, &user)?
            .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));
        let new_yes = position.yes_shares + yes_delta;
        let new_no = position.no_shares + no_delta;
        if new_yes >= 0 && new_no >= 0 {
            let prospective_locked =
                positions::calculate_locked_collateral(new_yes, new_no, market_price);
            let lock_increased = prospective_locked > position.locked_collateral;
            if lock_increased && prospective_locked > position.total_deposited {
                return Err(ContractError::InsufficientCollateral);
            }
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

        // 5a. Mint or burn outcome tokens for the updated position.
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
        settlement::batch_settle_positions(&env, market_id, users)
    }

    /// Register the treasury contract address for protocol fee routing.
    ///
    /// Once set, any non-zero withdrawal fee computed during
    /// [`withdraw_unused_collateral`] will be transferred to this address and
    /// recorded via the treasury's `collect_fee` entry point.
    ///
    /// Only the stored admin may call this.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – `admin` is not the stored admin.
    pub fn set_treasury(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_treasury(&env, &treasury);
        events::emit_treasury_set(&env, &treasury);
        Ok(())
    }

    /// Set the withdrawal fee rate in basis points (0–10_000).
    ///
    /// Only the stored admin may call this. A rate of 0 disables fees.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    /// - [`ContractError::InvalidPrice`] — `fee_rate_bps` outside 0–10_000.
    pub fn set_fee_rate(
        env: Env,
        admin: Address,
        fee_rate_bps: i128,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        validation::validate_fee_rate_bps(fee_rate_bps)?;
        storage::set_fee_rate_bps(&env, fee_rate_bps);
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
    pub fn set_threshold_signers(
        env: Env,
        admin: Address,
        signers: soroban_sdk::Vec<BytesN<32>>,
        quorum: u32,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_threshold_signers(&env, &signers);
        storage::set_threshold_quorum(&env, quorum);
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
    pub fn resolve_market_threshold(
        env: Env,
        resolver: Address,
        market_id: u32,
        outcome: bool,
        signatures: soroban_sdk::Vec<BytesN<64>>,
    ) -> Result<(), ContractError> {
        resolver.require_auth();

        let mut market =
            storage::get_market(&env, market_id)?.ok_or(ContractError::MarketNotFound)?;
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        let signers = storage::get_threshold_signers(&env);
        let quorum = storage::get_threshold_quorum(&env);

        oracle::verify_threshold_signatures(&env, market_id, outcome, &signers, &signatures, quorum)?;
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
    pub fn set_outcome_token_contract(
        env: Env,
        admin: Address,
        outcome_token_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_outcome_token_contract(&env, &outcome_token_contract);
        Ok(())
    }

    /// Return the registered outcome-token contract address, if any.
    pub fn get_outcome_token_contract(env: Env) -> Option<Address> {
        storage::get_outcome_token_contract(&env)
    }

    /// Register the resolution contract that gates `resolve_market`.
    ///
    /// When set, `resolve_market` will call into this contract to verify that
    /// a finalized candidate exists for the market before accepting a resolution.
    /// Pass `None` (by omitting the storage entry) to remove the gate.
    ///
    /// Only the stored admin may call this.
    pub fn set_resolution_contract(
        env: Env,
        admin: Address,
        resolution_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        storage::set_resolution_contract(&env, &resolution_contract);
        Ok(())
    }

    /// Return the registered resolution contract address, if any.
    pub fn get_resolution_contract(env: Env) -> Option<Address> {
        storage::get_resolution_contract(&env)
    }

    /// Return the registered treasury contract address, if any.
    pub fn get_treasury(env: Env) -> Option<Address> {
        storage::get_treasury(&env)
    }

    /// Return a read-only view of a market by its ID.
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] — no market exists with the given ID.
    /// - [`ContractError::UpgradeRequired`] — storage version mismatch.
    pub fn get_market(env: Env, market_id: u32) -> Result<crate::types::Market, ContractError> {
        storage::get_market(&env, market_id)?
            .ok_or(ContractError::MarketNotFound)
    }

    /// Return the immutable outcome count for a market (always 2 for binary markets).
    ///
    /// # Errors
    /// - [`ContractError::MarketNotFound`] — no market exists with the given ID.
    /// - [`ContractError::UpgradeRequired`] — storage version mismatch.
    pub fn get_outcome_count(env: Env, market_id: u32) -> Result<u32, ContractError> {
        let market = storage::get_market(&env, market_id)?
            .ok_or(ContractError::MarketNotFound)?;
        Ok(market.outcome_count)
    }

    /// Cancel an active market, preventing further deposits and withdrawals.
    ///
    /// Only the stored admin may call this. 0 disables fees.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] – caller is not the stored admin.
    /// - [`ContractError::InvalidPrice`] – `fee_rate_bps` is outside 0–10_000.
    pub fn set_fee_rate(
        env: Env,
        admin: Address,
        fee_rate_bps: i128,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = storage::get_admin(&env)?;
        if admin != stored_admin {
            return Err(ContractError::NotAdmin);
        }
        validation::validate_fee_rate_bps(fee_rate_bps)?;
        storage::set_fee_rate_bps(&env, fee_rate_bps);
        Ok(())
    }
}
