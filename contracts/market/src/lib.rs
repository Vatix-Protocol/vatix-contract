#![no_std]

mod deposit;
mod error;
mod events;
mod oracle;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
mod settlement;
mod withdraw;

#[allow(dead_code)]
mod storage;
mod test;
mod types;
#[allow(dead_code)]
mod validation;

use crate::error::ContractError;
use crate::types::{Market, MarketStatus};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

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
    /// Emits [`MarketCreatedEvent`] with `market_id`, `question`, and
    /// `end_time` as payload.
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
        let admin = storage::get_admin(&env);
        if creator != admin {
            return Err(ContractError::NotAdmin);
        }

        // 2. Validate inputs
        let current_time = env.ledger().timestamp();
        validation::validate_market_creation(&question, end_time, current_time)?;

        // 3. Generate market ID
        let market_id = storage::increment_market_id(&env);

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
        };

        // 5. Store market
        storage::set_market(&env, market_id, &market);

        // TODO(#issue): include creator address in MarketCreated event payload
        // 6. Emit event
        events::emit_market_created(&env, market_id, &question, end_time);

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
    /// Emits MarketResolved event
    pub fn resolve_market(
        env: Env,
        market_id: String,
        outcome: bool,
        signature: BytesN<64>,
    ) -> Result<(), ContractError> {
        let market_id = validation::parse_market_id(&market_id)?;
        // Step 1: Load and validate market
        let mut market =
            storage::get_market(&env, market_id).ok_or(ContractError::MarketNotFound)?;
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        // Step 2: Verify oracle signature (Ed25519; uses market's oracle_pubkey)
        oracle::verify_oracle_signature(
            &env,
            market_id,
            outcome,
            &signature,
            &market.oracle_pubkey,
        )?;

        // Step 3: Update market (status, outcome, persist)
        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);
        storage::set_market(&env, market_id, &market);

        // Step 4: Record resolution time and emit event
        let resolved_at = env.ledger().timestamp();
        // TODO(#issue): emit resolver identity alongside outcome in MarketResolved event
        events::emit_market_resolved(&env, market_id, outcome, resolved_at);

        Ok(())
    }
}
