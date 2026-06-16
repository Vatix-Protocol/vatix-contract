#![no_std]

mod deposit;
mod error;
mod events;
mod oracle;
#[allow(dead_code)]
mod positions;
#[allow(dead_code)]
mod settlement;

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
    /// One-time contract bootstrap. Must be called exactly once after WASM deploy.
    ///
    /// Sets the contract admin, which is required before any market can be created.
    /// Subsequent calls are rejected with [`ContractError::AlreadyInitialized`] to
    /// prevent admin hijacking.
    ///
    /// # Arguments
    /// * `env` - Soroban contract environment
    /// * `admin` - Address that will become the contract admin (must authorize)
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// - [`ContractError::AlreadyInitialized`] – contract was already initialized
    ///
    /// # Events
    /// Emits [`ContractInitializedEvent`] with `admin` as both topic and payload.
    ///
    /// # Example
    /// ```ignore
    /// client.initialize(&admin);
    /// // subsequent calls return AlreadyInitialized
    /// ```
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        // 1. Require the admin's authorization (cryptographic proof of ownership)
        admin.require_auth();

        // 2. Guard against re-initialization — prevent admin hijack on replay
        if storage::has_admin(&env) {
            return Err(ContractError::AlreadyInitialized);
        }

        // 3. Persist the admin address
        storage::set_admin(&env, &admin);

        // 4. Emit event so indexers and frontends can confirm bootstrap
        events::emit_contract_initialized(&env, &admin);

        Ok(())
    }

    /// Initialize a new market
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
            return Err(ContractError::Unauthorized);
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

    /// Resolve a market with oracle-signed outcome
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market to resolve
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
        market_id: u32,
        outcome: bool,
        signature: BytesN<64>,
    ) -> Result<(), ContractError> {
        // 1. Load and validate market
        let mut market =
            storage::get_market(&env, market_id).ok_or(ContractError::MarketNotFound)?;

        // 2. Check market is not already resolved
        if market.status == MarketStatus::Resolved {
            return Err(ContractError::MarketAlreadyResolved);
        }

        // 3. Verify oracle signature
        // Note: verify_oracle_signature may panic on invalid signatures, which will
        // be caught as a contract error. We use the market's stored oracle_pubkey.
        oracle::verify_oracle_signature(
            &env,
            market_id,
            outcome,
            &signature,
            &market.oracle_pubkey,
        )?;

        // 4. Update market status and store outcome
        market.status = MarketStatus::Resolved;
        market.result = Some(outcome);

        // 5. Store updated market
        storage::set_market(&env, market_id, &market);

        // 6. Record resolution time and emit event
        let resolved_at = env.ledger().timestamp();
        events::emit_market_resolved(&env, market_id, outcome, resolved_at);

        Ok(())
    }
}
