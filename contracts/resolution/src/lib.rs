// Issue #765: Required no_std attribute for Soroban WASM contract execution
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
//! | `StorageVersion`               | `u32`                    | Schema version guard (#696)                   |
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
#[cfg(test)]
mod bond_split_proptest;

use crate::error::ContractError;
use crate::types::{
    CandidateStatus, EmergencyMode, MarketStatus, ResolutionCandidate, ResolutionConfig,
};
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
pub const MIN_BOND_AMOUNT: i128 = 10_000_000;

/// Minimum bond a challenger must post (in the market's collateral token,
/// stroops) when calling `challenge`. Locked in this contract alongside the
/// proposer's bond. Requiring a bond (rather than a free challenge) is what
/// makes griefing — repeatedly challenging to indefinitely delay resolution
/// — economically costly instead of free.
pub const MIN_CHALLENGE_BOND_AMOUNT: i128 = 10_000_000;

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
        // Reject contract addresses as admin: a contract admin can be called
        // without a real key owner's consent, which would allow privilege
        // escalation. Mirrors the check in `vatix_market_contract::initialize`.
        if admin.executable().is_some() {
            return Err(ContractError::InvalidAdmin);
        }
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
        storage::set_version(&env);
        events::emit_resolution_registered(&env, &factory, &market_contract);
        Ok(())
    }

    pub fn get_default_challenge_window(env: Env) -> u64 {
        storage::get_config(&env).challenge_window_secs
    }

    /// Update the contract-wide *default* challenge window (#723).
    ///
    /// # Why this is not timelocked
    ///
    /// Every other privileged address/config mutator in this codebase that
    /// takes effect immediately for *existing* on-chain state goes through a
    /// propose/execute timelock (`ADDRESS_TIMELOCK_SECONDS`, 48h) — see
    /// `propose_factory`/`propose_market_contract` in this file. This setter
    /// is deliberately the exception, because `challenge_window_secs` has no
    /// binding effect on any candidate, past or present:
    ///
    /// - [`Self::propose`], [`Self::propose_v2`], and [`Self::appeal`] each
    ///   take their own `challenge_window_seconds` argument, independently
    ///   bounded by [`validate_challenge_window`] against the fixed
    ///   `MIN_CHALLENGE_WINDOW_SECONDS`/`MAX_CHALLENGE_WINDOW_SECONDS`
    ///   constants — never read from `config.challenge_window_secs`.
    /// - A candidate's `challenge_deadline` is computed once, at proposal
    ///   time, from that caller-supplied argument, and stored immutably on
    ///   the `ResolutionCandidate`. Changing the default afterwards cannot
    ///   move it.
    /// - A malicious admin shrinking the default to `MIN_CHALLENGE_WINDOW_SECONDS`
    ///   (60s) grants no capability an attacker didn't already have: any
    ///   proposer can already choose a 60s window on their own initiative,
    ///   with or without the admin's help, since `propose`/`propose_v2`
    ///   never require the caller to use the default.
    ///
    /// In short: this value is advisory metadata for off-chain callers
    /// deciding what window to *pass*, not an on-chain enforcement
    /// parameter — so instant, non-timelocked updates carry none of the
    /// "instant window shrink can brick challenges" risk the timelock
    /// pattern exists to prevent elsewhere. See
    /// `shrinking_default_does_not_move_existing_candidate_deadline` and
    /// `shrinking_default_does_not_constrain_a_new_proposals_chosen_window`
    /// in `test.rs` for the regression coverage proving both halves of this.
    pub fn set_default_challenge_window(
        env: Env,
        admin: Address,
        seconds: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
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

    /// Return the registered factory address (#752).
    ///
    /// The factory is stored in `ResolutionConfig` at `initialize` time and
    /// may later be rotated via the `propose_factory` / `execute_factory`
    /// timelock. Backend oracle services need a direct getter rather than
    /// deserializing the full `ResolutionConfig` struct, both for
    /// convenience and because the full config type may grow over time.
    pub fn get_factory(env: Env) -> Address {
        storage::get_config(&env).factory
    }

    /// Return the registered market contract address.
    ///
    /// Symmetric to [`get_factory`] — backend services that manage the
    /// resolution→market relationship need both addresses independently.
    pub fn get_market_contract(env: Env) -> Address {
        storage::get_config(&env).market_contract
    }

    /// Return the registered admin address.
    pub fn get_admin(env: Env) -> Address {
        storage::get_config(&env).admin
    }

    pub const ADDRESS_TIMELOCK_SECONDS: u64 = 172_800;

    pub fn propose_factory(env: Env, admin: Address, factory: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
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
        storage::assert_version(&env)?;
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
        admin.require_auth();
        storage::assert_version(&env)?;
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
        admin.require_auth();
        storage::assert_version(&env)?;
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
        storage::assert_version(&env)?;
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
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::clear_pending_market_contract(&env);
        Ok(())
    }

    /// Update the registered market contract address directly (admin only).
    pub fn set_market_contract(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let mut config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        config.market_contract = market_contract.clone();
        storage::set_config(&env, &config);
        events::emit_market_contract_set(&env, &market_contract);
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
        require_not_paused(&env)?;
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        // Emergency mode: resolution proposals are blocked unless mode is Normal
        require_emergency_mode_allows(&env, &[EmergencyMode::Normal])?;
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
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
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

        // Resolve the bond token, but defer the actual transfer until after
        // every state write below (Checks-Effects-Interactions, Issue #695).
        // Previously this transfer ran before `storage::set_candidate`, so a
        // reentrant call back into `propose` for the same `market_id` from
        // inside a malicious collateral token's `transfer` could have slipped
        // past the `CandidateAlreadyExists` guard above (which only sees a
        // candidate once one has actually been persisted) and posted a
        // second, inconsistent proposal before the first had recorded its own.
        let collateral_token = get_collateral_token(&env, &config, market_id);

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
            proposer: proposer.clone(),
            evidence_uri,
            proposed_at,
            challenge_deadline: proposed_at + challenge_window_seconds,
            status: CandidateStatus::Proposed,
            challenged_by: None,
            challenge_uri: None,
            finalized_at: None,
            appeal_round: 0,
            bond_amount,
            epoch: 0,
            passphrase_hash: None,
        };

        storage::set_candidate(&env, &candidate);
        events::emit_candidate_proposed(&env, &candidate);

        // Lock the proposer's bond in this contract's collateral-token
        // balance after all state writes (CEI, Issue #695).
        TokenClient::new(&env, &collateral_token).transfer(
            &proposer,
            env.current_contract_address(),
            &bond_amount,
        );

        Ok(candidate.id)
    }

    /// V2 counterpart of [`propose`] (#701): verifies the signed outcome via
    /// the market contract's `verify_signature_v2` — binding the signature to
    /// the network passphrase and a market resolution epoch, and carrying its
    /// own `valid_until` expiry — instead of the legacy V1 `verify_signature`.
    ///
    /// This is the path resolution operators must use once a market contract
    /// disables V1 (the default on a fresh deployment, #701): `finalize`
    /// (and admin arbitration) will call `resolve_market_v2` for any
    /// candidate proposed here, mirroring exactly the verification this
    /// function already performed.
    // Issue #765: propose_v2 requires 10 explicit parameters for Soroban contract specification
    #[allow(clippy::too_many_arguments)]
    pub fn propose_v2(
        env: Env,
        proposer: Address,
        market_id: u32,
        outcome: bool,
        signature: BytesN<64>,
        valid_until: u64,
        epoch: u32,
        passphrase_hash: BytesN<32>,
        evidence_uri: String,
        challenge_window_seconds: u64,
        bond_amount: i128,
    ) -> Result<u32, ContractError> {
        proposer.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_emergency_mode_allows(&env, &[EmergencyMode::Normal])?;
        validate_uri(&evidence_uri)?;
        validate_challenge_window(challenge_window_seconds)?;
        if bond_amount < MIN_BOND_AMOUNT {
            return Err(ContractError::InsufficientBond);
        }
        if storage::get_candidate_id_for_market(&env, market_id).is_some() {
            return Err(ContractError::CandidateAlreadyExists);
        }
        require_market_active(&env, &config, market_id)?;

        // Verify the provided V2 oracle signature by delegating to the
        // market contract's `verify_signature_v2` entrypoint.
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
            passphrase_hash.into_val(&env),
            market_id.into_val(&env),
            outcome.into_val(&env),
            valid_until.into_val(&env),
            epoch.into_val(&env),
            signature.clone().into_val(&env),
        ];
        let verification: Result<(), ContractError> = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(&env, "verify_signature_v2"),
            args,
        );
        verification?;

        let collateral_token = get_collateral_token(&env, &config, market_id);

        let proposed_at = env.ledger().timestamp();
        if valid_until < proposed_at {
            return Err(ContractError::InvalidSignatureExpiry);
        }
        let candidate = ResolutionCandidate {
            id: storage::increment_candidate_id(&env),
            market_id,
            outcome,
            signature,
            signature_expiry: valid_until,
            proposer: proposer.clone(),
            evidence_uri,
            proposed_at,
            challenge_deadline: proposed_at + challenge_window_seconds,
            status: CandidateStatus::Proposed,
            challenged_by: None,
            challenge_uri: None,
            finalized_at: None,
            appeal_round: 0,
            bond_amount,
            epoch,
            passphrase_hash: Some(passphrase_hash),
        };

        // Effects: persist the new candidate (status = Proposed) *before*
        // making any external call (CEI — issue #686, see
        // docs/reentrancy-cei-audit.md). Previously the bond transfer below
        // ran first, leaving a window where a malicious/upgraded token
        // contract's `transfer` callback could re-enter this contract while
        // no candidate record existed yet for this market.
        storage::set_candidate(&env, &candidate);
        events::emit_candidate_proposed(&env, &candidate);

        // Lock the proposer's bond in this contract's collateral-token
        // balance. Uses the same token as the market's collateral so a
        // single `finalize` refund path can rely on it. Ordered after every
        // state write above (Checks-Effects-Interactions, Issue #695).
        let token_client = TokenClient::new(&env, &collateral_token);
        token_client.transfer(&candidate.proposer, env.current_contract_address(), &bond_amount);

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
        require_not_paused(&env)?;
        storage::assert_version(&env)?;
        // Emergency mode: challenges are blocked in SettleOnly and GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[EmergencyMode::Normal, EmergencyMode::TradingHalted],
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
        if env.ledger().timestamp() >= candidate.challenge_deadline {
            return Err(ContractError::ChallengeWindowClosed);
        }

        let config = storage::get_config(&env);
        let collateral_token = get_collateral_token(&env, &config, candidate.market_id);

        // Effects before Interactions (Issue #695): persist the Challenged
        // status and the new challenger record BEFORE the external bond
        // transfer below. Previously the transfer ran first while
        // `candidate.status` was still e.g. `Proposed`, so a reentrant call
        // back into `challenge` for the same `candidate_id` from inside a
        // malicious collateral token's `transfer` could have slipped past
        // the `CandidateAlreadyChallenged` guard above and posted a second,
        // inconsistent challenge before the first had recorded its own.
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
        require_not_paused(&env)?;
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        // Emergency mode: appeals are blocked unless mode is Normal
        require_emergency_mode_allows(&env, &[EmergencyMode::Normal])?;
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
        // `appeal` only re-verifies via the legacy V1 `verify_signature`
        // (#701). A candidate originally proposed via `propose_v2` must not
        // be silently downgraded to a V1-verified signature while
        // `finalize`/arbitration still treat it as V2 (its `passphrase_hash`
        // would stay set from the original proposal) — reject outright
        // rather than allow that inconsistency.
        if candidate.passphrase_hash.is_some() {
            return Err(ContractError::Unauthorized);
        }

        // Defense in depth (Issue #497): the market should be unresolved for
        // any Challenged candidate under normal operation, but re-check in
        // case it was resolved/canceled out of band since the challenge.
        require_market_active(&env, &config, candidate.market_id)?;

        // Re-verify the new signed outcome the same way `propose` does.
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
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
        require_not_paused(&env)?;
        // Storage-version guard (Issue #696): a stale/partially-upgraded
        // deployment must fail closed here rather than let `finalize` run
        // its bond-settlement and `resolve_market` callback against a
        // storage layout the compiled contract no longer understands.
        storage::assert_version(&env)?;
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
        if env.ledger().timestamp() < candidate.challenge_deadline {
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
        // For a V1 candidate (`passphrase_hash.is_none()`) call `resolve_market`;
        // for a V2 candidate call `resolve_market_v2` with the stored
        // passphrase_hash, valid_until, epoch, and signature (#701).
        if let Some(passphrase_hash) = candidate.passphrase_hash.clone() {
            let args: Vec<Val> = soroban_sdk::vec![
                &env,
                env.current_contract_address().into_val(&env),
                market_id_to_string(&env, candidate.market_id).into_val(&env),
                candidate.outcome.into_val(&env),
                candidate.signature_expiry.into_val(&env),
                candidate.epoch.into_val(&env),
                candidate.signature.clone().into_val(&env),
                passphrase_hash.into_val(&env),
            ];
            let _: () = env.invoke_contract(
                &config.market_contract,
                &Symbol::new(&env, "resolve_market_v2"),
                args,
            );
        } else {
            let args: Vec<Val> = soroban_sdk::vec![
                &env,
                env.current_contract_address().into_val(&env),
                market_id_to_string(&env, candidate.market_id).into_val(&env),
                candidate.outcome.into_val(&env),
                candidate.signature.clone().into_val(&env),
                candidate.signature_expiry.into_val(&env),
            ];
            let _: () = env.invoke_contract(
                &config.market_contract,
                &Symbol::new(&env, "resolve_market"),
                args,
            );
        }

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
        storage::assert_version(&env)?;
        // Emergency mode: collateral deposits are blocked unless mode is Normal
        require_emergency_mode_allows(&env, &[EmergencyMode::Normal])?;
        if amount <= 0 {
            return Err(ContractError::InvalidCollateral);
        }
        // Effects before Interactions (Issue #695): persist the increased
        // collateral balance BEFORE the external transfer below. Previously
        // the transfer ran first while `prev` (read before it) was stale, so
        // a reentrant call back into `deposit_collateral` from inside a
        // malicious collateral token's `transfer` could have read the same
        // stale `prev` and overwritten rather than accumulated one of the
        // two deposits.
        let prev = storage::get_proposer_collateral(&env, &proposer);
        storage::set_proposer_collateral(&env, &proposer, prev + amount);
        TokenClient::new(&env, &collateral_token).transfer(
            &proposer,
            env.current_contract_address(),
            &amount,
        );
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
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        let amount = storage::get_proposer_collateral(&env, &proposer);
        if amount <= 0 {
            return Err(ContractError::InsufficientCollateral);
        }
        // Effects before Interactions (CEI, Issue #695): zero the balance
        // before transferring so a reentrant call back into slash_collateral
        // would see amount == 0 and return InsufficientCollateral.
        storage::set_proposer_collateral(&env, &proposer, 0);
        events::emit_collateral_slashed(&env, &proposer, &recipient, amount);
        let this = env.current_contract_address();
        TokenClient::new(&env, &collateral_token).transfer(
            &this,
            &recipient,
            &amount,
        );
        Ok(amount)
    }

    pub fn get_proposer_collateral(env: Env, proposer: Address) -> i128 {
        storage::get_proposer_collateral(&env, &proposer)
    }

    // ── Terminal arbitration / void path (dispute-game economics) ──────────────

    /// Propose registering (or replacing) the treasury address that
    /// receives the treasury-cut share of slashed bonds, subject to the same
    /// `ADDRESS_TIMELOCK_SECONDS` delay used by [`Self::propose_factory`] /
    /// [`Self::propose_market_contract`] (Issue #687). This prevents an
    /// admin from redirecting the slash treasury cut instantly. Call
    /// [`Self::execute_treasury`] once the timelock has elapsed to apply it.
    pub fn propose_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        let effective_at = env.ledger().timestamp() + Self::ADDRESS_TIMELOCK_SECONDS;
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

    /// Apply a previously-proposed treasury change once its timelock has
    /// elapsed (Issue #687). Callable by anyone — the timelock itself is the
    /// access control.
    pub fn execute_treasury(env: Env) -> Result<Address, ContractError> {
        storage::assert_version(&env)?;
        let pending = storage::get_pending_treasury(&env).ok_or(ContractError::Unauthorized)?;
        if env.ledger().timestamp() < pending.effective_at {
            return Err(ContractError::Unauthorized);
        }
        storage::set_treasury(&env, &pending.new_address);
        storage::clear_pending_treasury(&env);
        events::emit_treasury_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    /// Cancel a pending treasury address change before it takes effect.
    pub fn cancel_treasury(env: Env, admin: Address) -> Result<(), ContractError> {
        storage::assert_version(&env)?;
        admin.require_auth();
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::clear_pending_treasury(&env);
        Ok(())
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        storage::get_treasury(&env)
    }

    pub fn get_challengers(env: Env, candidate_id: u32) -> Vec<crate::types::ChallengeRecord> {
        storage::get_challengers(&env, candidate_id)
    }

    // ── Emergency Mode (Issue #662) ─────────────────────────────────────────

    /// Set the mirrored emergency mode (admin only). Operators should keep
    /// this value in sync with the Market and Treasury contracts for
    /// coordinated behaviour.
    pub fn set_emergency_mode(
        env: Env,
        admin: Address,
        new_mode: EmergencyMode,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::set_emergency_mode(&env, &new_mode);
        events::emit_emergency_mode_changed(&env, &new_mode, &admin);
        Ok(())
    }

    /// Return the current mirrored emergency mode.
    pub fn get_emergency_mode(env: Env) -> EmergencyMode {
        storage::get_emergency_mode(&env)
    }

    /// Pause the resolution contract, blocking all state-mutating operations
    /// until `unpause` is called.
    ///
    /// Only the stored admin may call this. Emits a `resolution_paused` event
    /// so off-chain indexers can track the pause state.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::set_paused(&env, true);
        events::emit_resolution_paused(&env, &admin);
        Ok(())
    }

    /// Unpause the resolution contract, restoring normal operation.
    ///
    /// Only the stored admin may call this. Emits a `resolution_unpaused`
    /// event so off-chain indexers can track the pause state.
    ///
    /// # Errors
    /// - [`ContractError::NotAdmin`] — `admin` is not the stored admin.
    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        storage::assert_version(&env)?;
        let config = storage::get_config(&env);
        require_admin(&admin, &config)?;
        storage::set_paused(&env, false);
        events::emit_resolution_unpaused(&env, &admin);
        Ok(())
    }

    /// Return whether the resolution contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
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
        require_not_paused(&env)?;
        storage::assert_version(&env)?;
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

        // Same ABI fix as `finalize` (#683) — see the comment there.
        let args: Vec<Val> = soroban_sdk::vec![
            &env,
            env.current_contract_address().into_val(&env),
            market_id_to_string(&env, candidate.market_id).into_val(&env),
            candidate.outcome.into_val(&env),
            candidate.signature.clone().into_val(&env),
            candidate.signature_expiry.into_val(&env),
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
        require_not_paused(&env)?;
        storage::assert_version(&env)?;
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

fn invoke_resolve_market(env: &Env, config: &ResolutionConfig, candidate: &ResolutionCandidate) {
    if candidate.passphrase_hash.is_some() {
        let args: Vec<Val> = soroban_sdk::vec![
            env,
            env.current_contract_address().into_val(env),
            market_id_to_string(env, candidate.market_id).into_val(env),
            candidate.outcome.into_val(env),
            candidate.signature_expiry.into_val(env),
            candidate.epoch.into_val(env),
            candidate.signature.clone().into_val(env),
            candidate.passphrase_hash.clone().unwrap().into_val(env),
        ];
        let _: () = env.invoke_contract(
            &config.market_contract,
            &Symbol::new(env, "resolve_market_v2"),
            args,
        );
        return;
    }

    let args: Vec<Val> = soroban_sdk::vec![
        env,
        env.current_contract_address().into_val(env),
        market_id_to_string(env, candidate.market_id).into_val(env),
        candidate.outcome.into_val(env),
        candidate.signature.clone().into_val(env),
        candidate.signature_expiry.into_val(env),
    ];
    let _: () = env.invoke_contract(
        &config.market_contract,
        &Symbol::new(env, "resolve_market"),
        args,
    );
}

/// Guard: reject state-mutating operations when the contract is paused.
///
/// # Errors
/// - [`ContractError::ContractPaused`] – the contract is in emergency halt.
fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if storage::is_paused(env) {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

/// Guard: reject operations that are not permitted under the current
/// emergency mode (Issue #662).
///
/// `allowed_modes` specifies the set of modes under which the guarded
/// operation is permitted. If the current mode is not in this set, the call
/// is rejected with [`ContractError::EmergencyModeActive`].
fn require_emergency_mode_allows(
    env: &Env,
    allowed_modes: &[EmergencyMode],
) -> Result<(), ContractError> {
    let current = storage::get_emergency_mode(env);
    if !allowed_modes.contains(&current) {
        return Err(ContractError::EmergencyModeActive);
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

/// Format `market_id` as its base-10 ASCII representation.
///
/// `Market::resolve_market` takes `market_id` as a `String` (parsed back to
/// `u32` via `validation::parse_market_id`), while every other market
/// entrypoint this contract calls (`verify_signature`,
/// `get_collateral_token`, `get_market_status`) takes the raw `u32`. This
/// bridges the two so `resolve_market`'s cross-contract call uses the real
/// ABI instead of a bare `u32` (#683).
fn market_id_to_string(env: &Env, market_id: u32) -> String {
    let mut buf = [0u8; 10];
    let mut n = market_id;
    let mut i = buf.len();
    if n == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while n > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    let s = core::str::from_utf8(&buf[i..]).unwrap_or("0");
    String::from_str(env, s)
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
// Issue #765: split_bond helper requires 7 explicit parameters for bond distribution calculation
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
