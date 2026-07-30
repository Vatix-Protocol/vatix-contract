#![no_std]
#![warn(clippy::all)]

//! # Resolution Contract
//!
//! Provides a challenge-based resolution lifecycle for Vatix prediction markets.
//! Proposers submit signed oracle outcomes; challengers can dispute them within
//! a configurable window; after the window closes an unchallenged candidate can
//! be finalized, which triggers `resolve_market` on the registered market
//! contract.
//!
//! Both proposer and challenger must post a bond in the market's collateral
//! token; whichever side loses the dispute has its bond forfeited and split
//! (`REWARD_BPS` to the winner, `BURN_BPS` burned, remainder to the
//! configured treasury) rather than refunded, so disputing — or repeatedly
//! challenging to grief resolution — costs real money.
//!
//! ## Lifecycle
//!
//! ```text
//!  Propose (signed outcome + evidence + bond)
//!      │
//!      ├── (window passes) ──► Finalize ──► refund proposer, slash challengers ──► market.resolve_market()
//!      │
//!      └── Challenge (+ bond) ──► status = Challenged (cannot finalize)
//!               │
//!               ├── Appeal (re-propose, new window) ──► back to Proposed
//!               │        │
//!               │        └── appeal_round >= MAX_APPEAL_ROUNDS, still Challenged, timelock elapsed:
//!               │             ├── arbitrate_uphold_proposer ──► same settlement as Finalize
//!               │             └── void_market ──► slash proposer, refund challengers, market.void_market()
//!               │
//!               └── (never appealed) ──► same terminal admin paths above once appeal_round reaches the cap
//! ```
//!
//! ## Storage layout
//!
//! | Key                            | Type                     | Description                                   |
//! |--------------------------------|--------------------------|-----------------------------------------------|
//! | `Config`                       | `ResolutionConfig`       | Admin, factory, and market contract addresses |
//! | `CandidateCounter`             | `u32`                    | Auto-increment counter for candidate IDs      |
//! | `Candidate(u32)`               | `ResolutionCandidate`    | Per-candidate resolution data                  |
//! | `CandidateByMarket(u32)`       | `u32`                    | Maps market_id → candidate_id (latest)        |
//! | `Challengers(u32)`             | `Vec<ChallengeRecord>`   | Every bonded challenger for a candidate        |
//! | `Treasury`                     | `Option<Address>`        | Optional recipient for the treasury bond cut   |

mod error;
mod events;
mod storage;
pub mod types;

#[cfg(test)]
mod test;

use crate::error::ContractError;
use crate::types::{CandidateStatus, EmergencyMode, MarketStatus, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};
use soroban_sdk::{IntoVal, Symbol, Val, Vec};

const MIN_CHALLENGE_WINDOW_SECONDS: u64 = 60;
const MAX_CHALLENGE_WINDOW_SECONDS: u64 = 14 * 24 * 60 * 60;
const MAX_URI_BYTES: u32 = 512;

/// Maximum number of times a candidate may be re-proposed via `appeal`
/// after being challenged. Once a `Challenged` candidate's `appeal_round`
/// reaches this cap, further appeals are rejected and the dispute becomes
/// eligible for terminal admin arbitration (`arbitrate_uphold_proposer`) or
/// voiding (`void_market`) — see the timelock below.
const MAX_APPEAL_ROUNDS: u32 = 3;

/// Minimum bond a proposer must post (in the market's collateral token,
/// stroops) when calling `propose`. Locked in this contract and refunded on
/// successful finalize.
const MIN_BOND_AMOUNT: i128 = 10_000_000;

/// Minimum bond a challenger must post (in the market's collateral token,
/// stroops) when calling `challenge`. Locked in this contract alongside the
/// proposer's bond. Requiring a bond (rather than a free challenge) is what
/// makes griefing — repeatedly challenging to indefinitely delay resolution
/// — economically costly instead of free.
const MIN_CHALLENGE_BOND_AMOUNT: i128 = 10_000_000;

/// Delay, in seconds, that must elapse after a candidate's last challenge
/// deadline before admin arbitration (`arbitrate_uphold_proposer`) or
/// voiding (`void_market`) may be executed. Mirrors
/// `vatix_market_contract::FEE_RATE_TIMELOCK_SECONDS`: it gives the
/// community a public window to react before an admin can unilaterally
/// settle a maximally-appealed dispute.
const ARBITRATION_TIMELOCK_SECONDS: u64 = 172_800;

/// Basis-point split applied whenever a bond is forfeited: `REWARD_BPS` goes
/// to the winning counterparty, `BURN_BPS` is burned (removed from supply),
/// and the remainder goes to the configured treasury (or stays in this
/// contract's balance if no treasury is registered). Documented split for
/// the "challenger reward, burn, treasury" requirement.
const REWARD_BPS: i128 = 5_000;
const BURN_BPS: i128 = 2_500;
const BPS_DENOMINATOR: i128 = 10_000;

#[contract]
pub struct ResolutionContract;

#[contractimpl]
impl ResolutionContract {
    /// Register the resolution lifecycle contract with its factory and market.
    ///
    /// `challenge_window_secs` is stored as the contract-wide default.
    pub fn initialize(
        env: Env,
        admin: Address,
        factory: Address,
        market_contract: Address,
        challenge_window_secs: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        if storage::has_config(&env) {
            return Err(ContractError::AlreadyInitialized);
        }
        validate_challenge_window(challenge_window_secs)?;
        storage::set_config(
            &env,
            &ResolutionConfig {
                admin,
                factory: factory.clone(),
                market_contract: market_contract.clone(),
                challenge_window_secs,
            },
        );
        events::emit_resolution_registered(&env, &factory, &market_contract);
        Ok(())
    }

    pub fn get_default_challenge_window(env: Env) -> u64 {
        storage::get_config(&env).challenge_window_secs
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
        config.challenge_window_secs = seconds;
        storage::set_config(&env, &config);
        Ok(())
    }

    pub fn get_config(env: Env) -> ResolutionConfig {
        storage::get_config(&env)
    }

    pub const ADDRESS_TIMELOCK_SECONDS: u64 = 172_800;

    pub fn propose_factory(env: Env, admin: Address, factory: Address) -> Result<(), ContractError> {
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        let effective_at = env.ledger().timestamp() + Self::ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_factory(
            &env,
            &crate::types::PendingAddressChange {
                new_address: factory.clone(),
                effective_at,
            },
        );
        events::emit_factory_proposed(&env, &factory, effective_at);
        Ok(())
    }

    pub fn execute_factory(env: Env) -> Result<Address, ContractError> {
        let pending = storage::get_pending_factory(&env).ok_or(ContractError::Unauthorized)?;
        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::Unauthorized);
        }
        let mut config = storage::get_config(&env);
        config.factory = pending.new_address.clone();
        storage::set_config(&env, &config);
        storage::clear_pending_factory(&env);
        events::emit_factory_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_factory(env: Env, admin: Address) -> Result<(), ContractError> {
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::clear_pending_factory(&env);
        Ok(())
    }

    pub fn propose_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        let effective_at = env.ledger().timestamp() + Self::ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_market_contract(
            &env,
            &crate::types::PendingAddressChange {
                new_address: market_contract.clone(),
                effective_at,
            },
        );
        events::emit_market_contract_proposed(&env, &market_contract, effective_at);
        Ok(())
    }

    pub fn execute_market_contract(env: Env) -> Result<Address, ContractError> {
        let pending = storage::get_pending_market_contract(&env).ok_or(ContractError::Unauthorized)?;
        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::Unauthorized);
        }
        let mut config = storage::get_config(&env);
        config.market_contract = pending.new_address.clone();
        storage::set_config(&env, &config);
        storage::clear_pending_market_contract(&env);
        events::emit_market_contract_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_market_contract(env: Env, admin: Address) -> Result<(), ContractError> {
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::clear_pending_market_contract(&env);
        Ok(())
    }

    /// Propose a signed resolution candidate for a market.
    ///
    /// The proposer must post a `bond_amount` (>= `MIN_BOND_AMOUNT`) in the
    /// market's collateral token, transferred from the proposer to this
    /// contract and locked until the candidate (or one of its appeals)
    /// finalizes, at which point it is refunded to the proposer.
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
        bond_amount: i128,
    ) -> Result<u32, ContractError> {
        proposer.require_auth();
        let config = storage::get_config(&env);
        // Emergency mode: resolution proposals are blocked unless mode is Normal
        require_emergency_mode_allows(
            &env,
            &[EmergencyMode::Normal],
        )?;
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;
        if bond_amount < MIN_BOND_AMOUNT {
            return Err(ContractError::InsufficientBond);
        }
        if storage::get_candidate_id_for_market(&env, market_id).is_some() {
            return Err(ContractError::CandidateAlreadyExists);
        }

        // Reject proposals for a market that is already resolved or canceled
        // (Issue #497). `CandidateAlreadyExists` above only catches markets
        // this contract itself has already finalized — a market resolved
        // through some other path (e.g. an admin-forced `resolve_market`)
        // would otherwise have no local candidate record to block a
        // now-meaningless new proposal.
        require_market_active(&env, &config, market_id)?;

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

        // Lock the proposer's bond in this contract's collateral-token
        // balance. Uses the same token as the market's collateral so a
        // single `finalize` refund path can rely on it.
        let collateral_token = get_collateral_token(&env, &config, market_id);
        let token_client = TokenClient::new(&env, &collateral_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &bond_amount);

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
            appeal_round: 0,
            bond_amount,
        };

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_proposed(&env, &candidate);
        Ok(candidate.id)
    }

    /// Challenge a candidate while its challenge window is still open.
    ///
    /// The challenger must post a `bond_amount` (>= `MIN_CHALLENGE_BOND_AMOUNT`)
    /// in the market's collateral token, transferred from the challenger to
    /// this contract and locked until the dispute reaches a terminal state
    /// (`finalize`, `arbitrate_uphold_proposer`, or `void_market`). Requiring
    /// a bond — rather than allowing free challenges — is what makes
    /// repeatedly challenging to grief resolution economically costly: an
    /// incorrect challenger loses this bond (see `finalize` /
    /// `arbitrate_uphold_proposer`).
    ///
    /// Every challenge across a candidate's whole appeal lifecycle is
    /// recorded (bounded by `MAX_APPEAL_ROUNDS + 1` entries), so a
    /// superseded challenger from an earlier round is still settled once the
    /// dispute terminates.
    pub fn challenge(
        env: Env,
        challenger: Address,
        candidate_id: u32,
        challenge_uri: String,
        bond_amount: i128,
    ) -> Result<(), ContractError> {
        challenger.require_auth();
        // Emergency mode: challenges are blocked in SettleOnly and GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                EmergencyMode::Normal,
                EmergencyMode::TradingHalted,
            ],
        )?;
        validate_uri(&challenge_uri)?;
        if bond_amount < MIN_CHALLENGE_BOND_AMOUNT {
            return Err(ContractError::InsufficientChallengeBond);
        }

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        if candidate.status == CandidateStatus::Finalized {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Voided {
            return Err(ContractError::CandidateAlreadyFinalized);
        }
        if candidate.status == CandidateStatus::Challenged {
            return Err(ContractError::CandidateAlreadyChallenged);
        }
        if env.ledger().timestamp() > candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowClosed);
        }

        let config = storage::get_config(&env);
        let collateral_token = get_collateral_token(&env, &config, candidate.market_id);
        let this = env.current_contract_address();
        TokenClient::new(&env, &collateral_token).transfer(&challenger, &this, &bond_amount);

        candidate.status = CandidateStatus::Challenged;
        candidate.challenged_by = Some(challenger.clone());
        candidate.challenge_uri = Some(challenge_uri.clone());
        storage::set_candidate(&env, &candidate);
        storage::append_challenger(&env, candidate_id, &challenger, bond_amount);
        events::emit_candidate_challenged(
            &env,
            candidate_id,
            candidate.market_id,
            &challenger,
            &challenge_uri,
            bond_amount,
        );
        Ok(())
    }

    /// Re-propose a challenged candidate, resetting it to `Proposed` with a
    /// fresh challenge window so it can be finalized (or challenged again).
    ///
    /// Only usable while the candidate is `Challenged`, and capped at
    /// `MAX_APPEAL_ROUNDS` total appeals per candidate — once the cap is
    /// reached, the dispute can no longer be advanced through this contract
    /// and must be resolved by the admin/factory out of band. The proposer's
    /// original bond stays locked and carries over across appeals; it is
    /// only refunded once the candidate is ultimately finalized.
    pub fn appeal(
        env: Env,
        proposer: Address,
        candidate_id: u32,
        outcome: bool,
        signature: BytesN<64>,
        evidence_uri: String,
        challenge_window_seconds: u64,
    ) -> Result<(), ContractError> {
        proposer.require_auth();
        let config = storage::get_config(&env);
        // Emergency mode: appeals are blocked unless mode is Normal
        require_emergency_mode_allows(
            &env,
            &[EmergencyMode::Normal],
        )?;
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        if candidate.status != CandidateStatus::Challenged {
            return Err(ContractError::CandidateNotChallenged);
        }
        if candidate.appeal_round >= MAX_APPEAL_ROUNDS {
            return Err(ContractError::AppealLimitExceeded);
        }

        // Defense in depth (Issue #497): the market should be unresolved for
        // any Challenged candidate under normal operation, but re-check in
        // case it was resolved/canceled out of band since the challenge.
        require_market_active(&env, &config, candidate.market_id)?;

        // Re-verify the new signed outcome the same way `propose` does.
        let args: Vec<Val> = soroban_sdk::vec![&env,
            candidate.market_id.into_val(&env),
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
        candidate.outcome = outcome;
        candidate.signature = signature;
        candidate.evidence_uri = evidence_uri;
        candidate.proposed_at = proposed_at;
        candidate.challenge_deadline = proposed_at + challenge_window_seconds;
        candidate.status = CandidateStatus::Proposed;
        candidate.challenged_by = None;
        candidate.challenge_uri = None;
        candidate.appeal_round += 1;

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_appealed(&env, &candidate);
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
        // Emergency mode: finalization is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                EmergencyMode::Normal,
                EmergencyMode::TradingHalted,
                EmergencyMode::SettleOnly,
            ],
        )?;
        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;

        if candidate.status == CandidateStatus::Finalized
            || candidate.status == CandidateStatus::Voided
        {
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
        // Persist the Finalized status to storage BEFORE the cross-contract
        // call so that any re-entrant or concurrent second finalize attempt
        // on the same candidate_id is immediately rejected by the
        // CandidateAlreadyFinalized guard above (exactly-once guarantee,
        // Issue #577).
        storage::set_candidate(&env, &candidate);

        // Double-check: re-read from storage to confirm the Finalized status
        // was persisted before we proceed to the cross-contract call. This is
        // defense-in-depth — if storage::set_candidate ever failed silently
        // the cross-contract call would be skipped rather than firing twice.
        {
            let stored = storage::get_candidate(&env, candidate_id)
                .ok_or(ContractError::CandidateNotFound)?;
            if stored.status != CandidateStatus::Finalized {
                return Err(ContractError::CandidateNotFound);
            }
        }

        // Refund the proposer's locked bond now that the candidate has
        // finalized successfully.
        let collateral_token = get_collateral_token(&env, &config, candidate.market_id);
        let token_client = TokenClient::new(&env, &collateral_token);
        let this = env.current_contract_address();
        if candidate.bond_amount > 0 {
            token_client.transfer(&this, &candidate.proposer, &candidate.bond_amount);
        }

        // The proposer's outcome stood unchallenged (or survived every
        // challenge it faced via `appeal`), so every challenger who ever
        // disputed this candidate was wrong — slash their bonds.
        settle_challengers_as_losers(&env, candidate_id, &candidate, &collateral_token);

        events::emit_candidate_finalized(&env, &candidate);

        // Cross-contract callback: resolve the market with the finalized outcome.
        // The Finalized status is already persisted above, so a second call to
        // finalize(candidate_id) will be rejected before reaching this point.
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
        // Emergency mode: collateral deposits are blocked unless mode is Normal
        require_emergency_mode_allows(
            &env,
            &[EmergencyMode::Normal],
        )?;
        if amount <= 0 {
            return Err(ContractError::InvalidCollateral);
        }
        TokenClient::new(&env, &collateral_token).transfer(
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
        TokenClient::new(&env, &collateral_token).transfer(
            &env.current_contract_address(),
            &recipient,
            &amount,
        );
        Ok(amount)
    }

    pub fn get_proposer_collateral(env: Env, proposer: Address) -> i128 {
        storage::get_proposer_collateral(&env, &proposer)
    }

    // ── Terminal arbitration / void path (dispute-game economics) ──────────────

    /// Register (or replace) the treasury address that receives the
    /// treasury-cut share of slashed bonds. Optional — while unset, that
    /// share simply stays in this contract's own collateral-token balance.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::set_treasury(&env, &treasury);
        Ok(())
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        storage::get_treasury(&env)
    }

    pub fn get_challengers(env: Env, candidate_id: u32) -> Vec<crate::types::ChallengeRecord> {
        storage::get_challengers(&env, candidate_id)
    }

    /// Admin-only, timelocked: uphold the proposer's currently-disputed
    /// outcome once `MAX_APPEAL_ROUNDS` have been exhausted and
    /// `ARBITRATION_TIMELOCK_SECONDS` have elapsed since the last challenge
    /// deadline. Settles bonds exactly like `finalize` (proposer refunded,
    /// every recorded challenger slashed) and invokes `resolve_market` with
    /// the candidate's existing signed outcome — this is the "arbitration"
    /// half of the checklist's "arbitrate or void" terminal path.
    pub fn arbitrate_uphold_proposer(
        env: Env,
        admin: Address,
        candidate_id: u32,
    ) -> Result<ResolutionCandidate, ContractError> {
        admin.require_auth();
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        require_arbitrable(&env, &candidate)?;

        candidate.status = CandidateStatus::Finalized;
        candidate.finalized_at = Some(env.ledger().timestamp());
        storage::set_candidate(&env, &candidate);

        let collateral_token = get_collateral_token(&env, &config, candidate.market_id);
        let token_client = TokenClient::new(&env, &collateral_token);
        let this = env.current_contract_address();
        if candidate.bond_amount > 0 {
            token_client.transfer(&this, &candidate.proposer, &candidate.bond_amount);
        }
        settle_challengers_as_losers(&env, candidate_id, &candidate, &collateral_token);

        events::emit_candidate_arbitrated(&env, candidate_id, candidate.market_id, candidate.outcome);

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

    /// Admin-only, timelocked: void the market once `MAX_APPEAL_ROUNDS` have
    /// been exhausted and `ARBITRATION_TIMELOCK_SECONDS` have elapsed, for
    /// disputes where arbitration cannot safely attest to either side's
    /// outcome on-chain (this contract has no valid signature for the
    /// challenger's claimed outcome, so it cannot call `resolve_market` in
    /// their favor). Rather than leaving the market stuck non-terminal
    /// forever, this slashes the proposer's bond (rewarding the disputing
    /// challenger(s) per the documented split), refunds every recorded
    /// challenger their own bond, and calls the market contract's
    /// `void_market` to move it to `Canceled` — unsticking it so users can
    /// reclaim collateral via `withdraw_canceled_collateral`.
    pub fn void_market(
        env: Env,
        admin: Address,
        candidate_id: u32,
    ) -> Result<ResolutionCandidate, ContractError> {
        admin.require_auth();
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;

        let mut candidate =
            storage::get_candidate(&env, candidate_id).ok_or(ContractError::CandidateNotFound)?;
        require_arbitrable(&env, &candidate)?;

        candidate.status = CandidateStatus::Voided;
        candidate.finalized_at = Some(env.ledger().timestamp());
        storage::set_candidate(&env, &candidate);

        let collateral_token = get_collateral_token(&env, &config, candidate.market_id);
        let token_client = TokenClient::new(&env, &collateral_token);
        let challengers = storage::get_challengers(&env, candidate_id);

        // The proposer's outcome could not withstand the final challenge, so
        // their bond is forfeited — split per the documented reward/burn/
        // treasury rule, rewarding whichever challenger's dispute stands
        // (the most recent one on record).
        if candidate.bond_amount > 0 {
            let reward_recipient = challengers
                .last()
                .map(|record| record.challenger)
                .unwrap_or_else(|| candidate.proposer.clone());
            split_bond(
                &env,
                candidate_id,
                candidate.market_id,
                &collateral_token,
                &candidate.proposer,
                &reward_recipient,
                candidate.bond_amount,
            );
        }

        // Voiding means "no side could be safely vindicated on-chain", not
        // "the challenger loses" — refund every recorded challenger in full.
        let this = env.current_contract_address();
        for record in challengers.iter() {
            if record.bond > 0 {
                token_client.transfer(&this, &record.challenger, &record.bond);
                events::emit_bond_refunded(
                    &env,
                    candidate_id,
                    candidate.market_id,
                    &record.challenger,
                    record.bond,
                );
            }
        }
        storage::clear_challengers(&env, candidate_id);

        events::emit_market_voided(&env, candidate_id, candidate.market_id);

        let args: Vec<Val> = soroban_sdk::vec![
            &env,
            env.current_contract_address().into_val(&env),
            candidate.market_id.into_val(&env),
        ];
        let _: () = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "void_market"),
            args,
        );

        Ok(candidate)
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

/// Look up a market's collateral token via a cross-contract call to the
/// registered market contract's `get_collateral_token`, used to lock and
/// refund proposer bonds in the same token as the market itself.
fn get_collateral_token(env: &Env, config: &ResolutionConfig, market_id: u32) -> Address {
    env.invoke_contract(
        &config.market_contract,
        &Symbol::new(env, "get_collateral_token"),
        soroban_sdk::vec![env, market_id.into_val(env)],
    )
}

/// Reject the call if `market_id` is not `Active` on the registered market
/// contract (Issue #497). Guards against accepting a new resolution proposal
/// or appeal for a market that has already been resolved or canceled through
/// some other path (e.g. an admin-forced `resolve_market` call that bypassed
/// this contract's `finalize`).
fn require_market_active(
    env: &Env,
    config: &ResolutionConfig,
    market_id: u32,
) -> Result<(), ContractError> {
    let status: MarketStatus = env.invoke_contract(
        &config.market_contract,
        &Symbol::new(env, "get_market_status"),
        soroban_sdk::vec![env, market_id.into_val(env)],
    );
    if status != MarketStatus::Active {
        return Err(ContractError::MarketAlreadyResolved);
    }
    Ok(())
}

/// Guard shared by `arbitrate_uphold_proposer` and `void_market`: both
/// require the candidate to still be `Challenged` (an active, unresolved
/// dispute — never true for `Proposed`, which the normal `finalize` path
/// already handles) and `ARBITRATION_TIMELOCK_SECONDS` to have elapsed since
/// its last challenge deadline.
///
/// Deliberately does *not* additionally require `appeal_round >=
/// MAX_APPEAL_ROUNDS`: the primary intended trigger is a candidate that
/// exhausted every appeal it was entitled to, but a proposer can also simply
/// abandon a `Challenged` candidate without ever calling `appeal` at
/// round 0 — that dispute would otherwise sit `Challenged` forever with no
/// path to a terminal state. Gating on the timelock alone (rather than the
/// appeal count) closes that gap too, while the multi-day delay still gives
/// the community time to react before an admin can act unilaterally.
fn require_arbitrable(env: &Env, candidate: &ResolutionCandidate) -> Result<(), ContractError> {
    if candidate.status != CandidateStatus::Challenged {
        return Err(ContractError::NotArbitrable);
    }
    let eta = candidate.challenge_deadline + ARBITRATION_TIMELOCK_SECONDS;
    if env.ledger().timestamp() < eta {
        return Err(ContractError::ArbitrationTimelockNotElapsed);
    }
    Ok(())
}

/// Forfeit `total` (a bond in `token`) and split it per the documented rule:
/// `REWARD_BPS` to `winner`, `BURN_BPS` burned, and the remainder to the
/// configured treasury (or left in this contract's balance if none is
/// registered). Emits `BondSlashed` for observability.
#[allow(clippy::too_many_arguments)]
fn split_bond(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    token: &Address,
    loser: &Address,
    winner: &Address,
    total: i128,
) {
    if total <= 0 {
        return;
    }
    let token_client = TokenClient::new(env, token);
    let this = env.current_contract_address();

    let reward = total * REWARD_BPS / BPS_DENOMINATOR;
    let burned = total * BURN_BPS / BPS_DENOMINATOR;
    // Remainder (covers rounding dust) is the treasury cut, so
    // reward + burned + treasury_cut always sums to exactly `total`.
    let treasury_cut = total - reward - burned;

    if reward > 0 {
        token_client.transfer(&this, winner, &reward);
    }
    if burned > 0 {
        token_client.burn(&this, &burned);
    }
    if treasury_cut > 0 {
        if let Some(treasury) = storage::get_treasury(env) {
            token_client.transfer(&this, &treasury, &treasury_cut);
        }
    }

    events::emit_bond_slashed(
        env,
        candidate_id,
        market_id,
        loser,
        winner,
        total,
        reward,
        burned,
        treasury_cut,
    );
}

/// Settle every challenger recorded against `candidate` as having lost the
/// dispute — the proposer's outcome ultimately stood (via `finalize` or
/// `arbitrate_uphold_proposer`). Each challenger's bond is forfeited and
/// split per `split_bond`, rewarding the proposer.
fn settle_challengers_as_losers(
    env: &Env,
    candidate_id: u32,
    candidate: &ResolutionCandidate,
    collateral_token: &Address,
) {
    let challengers = storage::get_challengers(env, candidate_id);
    if challengers.is_empty() {
        return;
    }
    for record in challengers.iter() {
        split_bond(
            env,
            candidate_id,
            candidate.market_id,
            collateral_token,
            &record.challenger,
            &candidate.proposer,
            record.bond,
        );
    }
    storage::clear_challengers(env, candidate_id);
}
