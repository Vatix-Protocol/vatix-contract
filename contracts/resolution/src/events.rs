use soroban_sdk::{contractevent, Address, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct ResolutionRegistered {
    #[topic]
    pub factory: Address,
    pub market_contract: Address,
    pub registered_at: u64,
}

pub fn emit_resolution_registered(env: &Env, factory: &Address, market_contract: &Address) {
    ResolutionRegistered {
        factory: factory.clone(),
        market_contract: market_contract.clone(),
        registered_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateProposed {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub proposer: Address,
    pub evidence_uri: String,
    pub challenge_deadline: u64,
    pub signature_expiry: u64,
    pub bond_amount: i128,
}

pub fn emit_candidate_proposed(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateProposed {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        proposer: candidate.proposer.clone(),
        evidence_uri: candidate.evidence_uri.clone(),
        challenge_deadline: candidate.challenge_deadline,
        signature_expiry: candidate.signature_expiry,
        bond_amount: candidate.bond_amount,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateChallenged {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub challenger: Address,
    pub challenge_uri: String,
    pub bond_amount: i128,
    pub challenged_at: u64,
}

pub fn emit_candidate_challenged(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    challenger: &Address,
    challenge_uri: &String,
    bond_amount: i128,
) {
    CandidateChallenged {
        candidate_id,
        market_id,
        challenger: challenger.clone(),
        challenge_uri: challenge_uri.clone(),
        bond_amount,
        challenged_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// A bond (proposer's or a challenger's) was forfeited and split into a
/// reward to the winning party, a burned portion, and a treasury portion
/// (Issue: dispute-game economics). `loser` is whoever posted the bond;
/// `winner` receives the reward share.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BondSlashed {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub loser: Address,
    pub winner: Address,
    pub total: i128,
    pub reward: i128,
    pub burned: i128,
    pub treasury_cut: i128,
    pub slashed_at: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn emit_bond_slashed(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    loser: &Address,
    winner: &Address,
    total: i128,
    reward: i128,
    burned: i128,
    treasury_cut: i128,
) {
    BondSlashed {
        candidate_id,
        market_id,
        loser: loser.clone(),
        winner: winner.clone(),
        total,
        reward,
        burned,
        treasury_cut,
        slashed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// A bond was refunded in full (no fault determined), e.g. every recorded
/// challenger's bond when a market is voided.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BondRefunded {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub recipient: Address,
    pub amount: i128,
    pub refunded_at: u64,
}

pub fn emit_bond_refunded(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    recipient: &Address,
    amount: i128,
) {
    BondRefunded {
        candidate_id,
        market_id,
        recipient: recipient.clone(),
        amount,
        refunded_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Admin arbitration upheld the proposer's disputed outcome after
/// `MAX_APPEAL_ROUNDS` were exhausted (Issue: dispute-game economics).
#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateArbitrated {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub arbitrated_at: u64,
}

pub fn emit_candidate_arbitrated(env: &Env, candidate_id: u32, market_id: u32, outcome: bool) {
    CandidateArbitrated {
        candidate_id,
        market_id,
        outcome,
        arbitrated_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// The underlying market was voided after `MAX_APPEAL_ROUNDS` were exhausted
/// with no safely-attestable outcome (Issue: dispute-game economics).
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketVoided {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub voided_at: u64,
}

pub fn emit_market_voided(env: &Env, candidate_id: u32, market_id: u32) {
    MarketVoided {
        candidate_id,
        market_id,
        voided_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FactoryProposed {
    #[topic]
    pub factory: Address,
    pub effective_at: u64,
}

pub fn emit_factory_proposed(env: &Env, factory: &Address, effective_at: u64) {
    FactoryProposed {
        factory: factory.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FactorySet {
    #[topic]
    pub factory: Address,
    pub set_at: u64,
}

pub fn emit_factory_set(env: &Env, factory: &Address) {
    FactorySet {
        factory: factory.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractProposed {
    #[topic]
    pub market_contract: Address,
    pub effective_at: u64,
}

pub fn emit_market_contract_proposed(env: &Env, market_contract: &Address, effective_at: u64) {
    MarketContractProposed {
        market_contract: market_contract.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractSet {
    #[topic]
    pub market_contract: Address,
    pub set_at: u64,
}

pub fn emit_market_contract_set(env: &Env, market_contract: &Address) {
    MarketContractSet {
        market_contract: market_contract.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

/// Emitted when an admin proposes a new treasury address for the slashed-bond
/// treasury cut (Issue #687). Does not take effect until `effective_at` and
/// `execute_treasury` is called.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryProposed {
    #[topic]
    pub treasury: Address,
    pub effective_at: u64,
}

pub fn emit_treasury_proposed(env: &Env, treasury: &Address, effective_at: u64) {
    TreasuryProposed {
        treasury: treasury.clone(),
        effective_at,
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasurySet {
    #[topic]
    pub treasury: Address,
    pub set_at: u64,
}

pub fn emit_treasury_set(env: &Env, treasury: &Address) {
    TreasurySet {
        treasury: treasury.clone(),
        set_at: env.ledger().timestamp(),
    }.publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateFinalized {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub finalized_at: u64,
}

pub fn emit_candidate_finalized(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateFinalized {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        finalized_at: candidate.finalized_at.unwrap_or(env.ledger().timestamp()),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateAppealed {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub proposer: Address,
    pub appeal_round: u32,
    pub evidence_uri: String,
    pub challenge_deadline: u64,
    pub appealed_at: u64,
}

pub fn emit_candidate_appealed(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateAppealed {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        proposer: candidate.proposer.clone(),
        appeal_round: candidate.appeal_round,
        evidence_uri: candidate.evidence_uri.clone(),
        challenge_deadline: candidate.challenge_deadline,
        appealed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Emergency Mode (Issue #662) ──────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyModeChanged {
    #[topic]
    pub new_mode: crate::types::EmergencyMode,
    pub admin: Address,
    pub changed_at: u64,
}

pub fn emit_emergency_mode_changed(env: &Env, new_mode: &crate::types::EmergencyMode, admin: &Address) {
    EmergencyModeChanged {
        new_mode: new_mode.clone(),
        admin: admin.clone(),
        changed_at: env.ledger().timestamp(),
    }
    .publish(env);
}
