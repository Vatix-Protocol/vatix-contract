use soroban_sdk::{contractevent, Address, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct ResolutionRegisteredEvent {
    #[topic]
    pub factory: Address,
    pub market_contract: Address,
    pub registered_at: u64,
}

pub fn emit_resolution_registered(env: &Env, factory: &Address, market_contract: &Address) {
    ResolutionRegisteredEvent {
        factory: factory.clone(),
        market_contract: market_contract.clone(),
        registered_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateProposedEvent {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub proposer: Address,
    pub evidence_uri: String,
    pub challenge_deadline: u64,
}

pub fn emit_candidate_proposed(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateProposedEvent {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        proposer: candidate.proposer.clone(),
        evidence_uri: candidate.evidence_uri.clone(),
        challenge_deadline: candidate.challenge_deadline,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateChallengedEvent {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub challenger: Address,
    pub challenge_uri: String,
    pub challenged_at: u64,
}

pub fn emit_candidate_challenged(
    env: &Env,
    candidate_id: u32,
    market_id: u32,
    challenger: &Address,
    challenge_uri: &String,
) {
    CandidateChallengedEvent {
        candidate_id,
        market_id,
        challenger: challenger.clone(),
        challenge_uri: challenge_uri.clone(),
        challenged_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CandidateFinalizedEvent {
    #[topic]
    pub candidate_id: u32,
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub finalized_at: u64,
}

pub fn emit_candidate_finalized(env: &Env, candidate: &crate::types::ResolutionCandidate) {
    CandidateFinalizedEvent {
        candidate_id: candidate.id,
        market_id: candidate.market_id,
        outcome: candidate.outcome,
        finalized_at: candidate.finalized_at.unwrap_or(env.ledger().timestamp()),
    }
    .publish(env);
}
