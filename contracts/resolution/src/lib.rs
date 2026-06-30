#![no_std]
#![deny(clippy::all)]

//! # Resolution Contract
//!
//! Provides a challenge-based resolution lifecycle for Vatix prediction markets.
//! Proposers submit signed oracle outcomes; challengers can dispute them within
//! a configurable window; after the window closes an unchallenged candidate can
//! be finalized, which triggers `resolve_market` on the registered market
//! contract.
//!
//! ## Lifecycle
//!
//! ```text
//!  Propose (signed outcome + evidence URI)
//!      │
//!      ├── (window passes) ──► Finalize ──► market.resolve_market()
//!      │
//!      └── Challenge ──► status = Challenged (cannot finalize)
//! ```
//!
//! ## Storage layout
//!
//! | Key                            | Type                  | Description                                   |
//! |--------------------------------|-----------------------|-----------------------------------------------|
//! | `Config`                       | `ResolutionConfig`    | Admin, factory, and market contract addresses |
//! | `CandidateCounter`             | `u32`                 | Auto-increment counter for candidate IDs      |
//! | `Candidate(u32)`               | `ResolutionCandidate` | Per-candidate resolution data                  |
//! | `CandidateByMarket(u32)`       | `u32`                 | Maps market_id → candidate_id (latest)        |

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{CandidateStatus, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, String};
use soroban_sdk::{IntoVal, Symbol, Val, Vec};

const MIN_CHALLENGE_WINDOW_SECONDS: u64 = 60;
const MAX_CHALLENGE_WINDOW_SECONDS: u64 = 14 * 24 * 60 * 60;
const MAX_URI_BYTES: u32 = 512;

#[contract]
pub struct ResolutionContract;

#[contractimpl]
impl ResolutionContract {
    /// Register the resolution lifecycle contract with its factory and market.
    ///
    /// `default_challenge_window_seconds` is stored as the contract-wide default.
    pub fn initialize(
        env: Env,
        admin: Address,
        factory: Address,
        market_contract: Address,
        default_challenge_window_seconds: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(ContractError::AlreadyInitialized);
        }
        validate_challenge_window(default_challenge_window_seconds)?;
        storage::set_config(
            &env,
            &ResolutionConfig {
                admin,
                factory: factory.clone(),
                market_contract: market_contract.clone(),
                default_challenge_window_seconds,
            },
        );
        events::emit_resolution_registered(&env, &factory, &market_contract);
        Ok(())
    }

    pub fn get_default_challenge_window(env: Env) -> u64 {
        storage::get_config(&env).default_challenge_window_seconds
    }

    pub fn set_default_challenge_window(
        env: Env,
        admin: Address,
        seconds: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        validate_challenge_window(seconds)?;
        config.default_challenge_window_seconds = seconds;
        storage::set_config(&env, &config);
        Ok(())
    }

    pub fn get_config(env: Env) -> ResolutionConfig {
        storage::get_config(&env)
    }

    /// Update the registered factory address.
    pub fn set_factory(env: Env, admin: Address, factory: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        config.factory = factory.clone();
        storage::set_config(&env, &config);
        events::emit_resolution_registered(&env, &factory, &config.market_contract);
        Ok(())
    }

    /// Update the market contract address that finalized candidates target.
    pub fn set_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        config.market_contract = market_contract.clone();
        storage::set_config(&env, &config);
        events::emit_resolution_registered(&env, &config.factory, &market_contract);
        Ok(())
    }

    /// Propose a signed resolution candidate for a market.
    ///
    /// The returned candidate is the on-chain anchor for the backend
    /// `ResolutionCandidate`: off-chain services may display the same
    /// `challenge_deadline` and evidence URI while listening for challenge and
    /// finalize events.
    pub fn propose(
        env: Env,
        proposer: Address,
        market_id: u32,
        outcome: bool,
        signature: BytesN<64>,
        signature_expiry: u64,
        evidence_uri: String,
        challenge_window_seconds: u64,
    ) -> Result<u32, ContractError> {
        proposer.require_auth();
        let config = storage::get_config(&env);
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;
        if storage::get_candidate_id_for_market(&env, market_id).is_some() {
            return Err(ContractError::CandidateAlreadyExists);
        }

        // Verify the provided oracle signature by delegating to the market
        // contract's `verify_signature` entrypoint. This ensures proposals are
        // rejected early if the signature does not verify.
        let args: Vec<Val> = soroban_sdk::vec![&env,
            market_id.into_val(&env),
            outcome.into_val(&env),
            signature.clone().into_val(&env),
        ];
        let verification: Result<(), ContractError> = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "verify_signature"),
            args,
        );
        verification?;

        let proposed_at = env.ledger().timestamp();
        // Validate signature expiry must be in the future (at or after proposed_at)
        if signature_expiry < proposed_at {
            return Err(ContractError::InvalidSignatureExpiry);
        }
        let candidate = ResolutionCandidate {
            id: storage::increment_candidate_id(&env),
            market_id,
            outcome,
            signature,
            signature_expiry,
            proposer,
            evidence_uri,
            proposed_at,
            challenge_deadline: proposed_at + challenge_window_seconds,
            status: CandidateStatus::Proposed,
            challenged_by: None,
            challenge_uri: None,
            finalized_at: None,
        };

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_proposed(&env, &candidate);
        Ok(candidate.id)
    }

    /// Challenge a candidate while its challenge window is still open.
    pub fn challenge(
        env: Env,
        challenger: Address,
        candidate_id: u32,
        challenge_uri: String,
    ) -> Result<(), ContractError> {
        challenger.require_auth();
        validate_uri(&challenge_uri)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        if candidate.status == CandidateStatus::Finalized {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Challenged {
            return Err(ContractError::CandidateAlreadyChallenged);
        }
        if env.ledger().timestamp() > candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowClosed);
        }

        candidate.status = CandidateStatus::Challenged;
        candidate.challenged_by = Some(challenger.clone());
        candidate.challenge_uri = Some(challenge_uri.clone());
        storage::set_candidate(&env, &candidate);
        events::emit_candidate_challenged(
            &env,
            candidate_id,
            candidate.market_id,
            &challenger,
            &challenge_uri,
        );
        Ok(())
    }

    /// Finalize an unchallenged candidate after its challenge window closes.
    ///
    /// After marking the candidate as `Finalized`, immediately invokes
    /// `resolve_market(market_id, outcome, signature)` on the registered
    /// market contract so the market state is settled atomically.
    pub fn finalize(
        env: Env,
        finalizer: Address,
        candidate_id: u32,
    ) -> Result<ResolutionCandidate, ContractError> {
        finalizer.require_auth();
        let config = storage::get_config(&env);
        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;

        if candidate.status == CandidateStatus::Finalized {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Challenged {
            return Err(ContractError::CandidateAlreadyChallenged);
        }
        if env.ledger().timestamp() <= candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowOpen);
        }

        // The signed outcome must still be within its expiry deadline.
        if env.ledger().timestamp() > candidate.signature_expiry {
            return Err(ContractError::SignatureExpired);
        }

        candidate.status = CandidateStatus::Finalized;
        candidate.finalized_at = Some(env.ledger().timestamp());
        storage::set_candidate(&env, &candidate);
        events::emit_candidate_finalized(&env, &candidate);

        // Cross-contract callback: resolve the market with the finalized outcome.
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
            candidate.market_id.into_val(&env),
            candidate.outcome.into_val(&env),
            candidate.signature.clone().into_val(&env),
        ];
        let _: () = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "resolve_market"),
            args,
        );

        Ok(candidate)
    }

    pub fn get_candidate(env: Env, candidate_id: u32) -> Option<ResolutionCandidate> {
        storage::get_candidate(&env, candidate_id)
    }

    pub fn get_candidate_id_for_market(env: Env, market_id: u32) -> Option<u32> {
        storage::get_candidate_id_for_market(&env, market_id)
    }

    // ── #381: Proposer collateral ──────────────────────────────────────────────

    pub fn deposit_collateral(
        env: Env,
        proposer: Address,
        collateral_token: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        proposer.require_auth();
        if amount <= 0 {
            return Err(ContractError::InvalidCollateral);
        }
        token::Client::new(&env, &collateral_token).transfer(
            &proposer,
            &env.current_contract_address(),
            &amount,
        );
        let prev = storage::get_proposer_collateral(&env, &proposer);
        storage::set_proposer_collateral(&env, &proposer, prev + amount);
        Ok(())
    }

    /// Slash the full collateral of an incorrect proposer (admin only).
    pub fn slash_collateral(
        env: Env,
        admin: Address,
        proposer: Address,
        collateral_token: Address,
        recipient: Address,
    ) -> Result<i128, ContractError> {
        admin.require_auth();
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        let amount = storage::get_proposer_collateral(&env, &proposer);
        if amount <= 0 {
            return Err(ContractError::InsufficientCollateral);
        }
        storage::set_proposer_collateral(&env, &proposer, 0);
        token::Client::new(&env, &collateral_token).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );
        Ok(amount)
    }

    pub fn get_proposer_collateral(env: Env, proposer: Address) -> i128 {
        storage::get_proposer_collateral(&env, &proposer)
    }
}

fn require_admin(admin: &Address, config: &ResolutionConfig) -> Result<(), ContractError> {
    if admin != &config.admin {
        return Err(ContractError::NotAdmin);
    }
    Ok(())
}

fn validate_challenge_window(seconds: u64) -> Result<(), ContractError> {
    if !(MIN_CHALLENGE_WINDOW_SECONDS..=MAX_CHALLENGE_WINDOW_SECONDS).contains(&seconds) {
        return Err(ContractError::InvalidChallengeWindow);
    }
    Ok(())
}

fn validate_uri(uri: &String) -> Result<(), ContractError> {
    let len = uri.len();
    if len == 0 || len > MAX_URI_BYTES {
        return Err(ContractError::InvalidEvidenceUri);
    }
    Ok(())
}
