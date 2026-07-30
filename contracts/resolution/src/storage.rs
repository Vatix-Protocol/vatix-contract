use crate::types::{ChallengeRecord, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::{contracttype, Address, Env, Vec};

#[contracttype]
pub enum StorageKey {
    Config,
    CandidateCounter,
    Candidate(u32),
    CandidateByMarket(u32),
    ProposerCollateral(Address),
    /// Every bonded challenger for a candidate across its whole appeal
    /// lifecycle (bounded by `MAX_APPEAL_ROUNDS + 1` entries).
    Challengers(u32),
    /// Optional treasury address that receives the treasury-cut share of
    /// slashed bonds. Unset by default; slashed treasury shares stay in the
    /// contract's own balance until an admin registers one (mirrors the
    /// market contract's "fee retained, no treasury" pattern).
    Treasury,
}

pub fn has_config(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Config)
}

pub fn get_config(env: &Env) -> ResolutionConfig {
    env.storage()
        .persistent()
        .get(&StorageKey::Config)
        .expect("Resolution config not set")
}

pub fn set_config(env: &Env, config: &ResolutionConfig) {
    env.storage().persistent().set(&StorageKey::Config, config);
}

pub fn get_pending_factory(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage().persistent().get(&StorageKey::PendingFactory)
}

pub fn set_pending_factory(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage().persistent().set(&StorageKey::PendingFactory, pending);
}

pub fn clear_pending_factory(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingFactory);
}

pub fn get_pending_market_contract(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage().persistent().get(&StorageKey::PendingMarketContract)
}

pub fn set_pending_market_contract(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage().persistent().set(&StorageKey::PendingMarketContract, pending);
}

pub fn clear_pending_market_contract(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingMarketContract);
}

pub fn increment_candidate_id(env: &Env) -> u32 {
    let next = env
        .storage()
        .persistent()
        .get(&StorageKey::CandidateCounter)
        .unwrap_or(0u32)
        + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::CandidateCounter, &next);
    next
}

pub fn get_candidate(env: &Env, candidate_id: u32) -> Option<ResolutionCandidate> {
    env.storage()
        .persistent()
        .get(&StorageKey::Candidate(candidate_id))
}

pub fn set_candidate(env: &Env, candidate: &ResolutionCandidate) {
    env.storage()
        .persistent()
        .set(&StorageKey::Candidate(candidate.id), candidate);
    env.storage().persistent().set(
        &StorageKey::CandidateByMarket(candidate.market_id),
        &candidate.id,
    );
}

pub fn get_candidate_id_for_market(env: &Env, market_id: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&StorageKey::CandidateByMarket(market_id))
}

pub fn get_proposer_collateral(env: &Env, proposer: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::ProposerCollateral(proposer.clone()))
        .unwrap_or(0i128)
}

pub fn set_proposer_collateral(env: &Env, proposer: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::ProposerCollateral(proposer.clone()), &amount);
}

pub fn get_challengers(env: &Env, candidate_id: u32) -> Vec<ChallengeRecord> {
    env.storage()
        .persistent()
        .get(&StorageKey::Challengers(candidate_id))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn append_challenger(env: &Env, candidate_id: u32, challenger: &Address, bond: i128) {
    let mut challengers = get_challengers(env, candidate_id);
    challengers.push_back(ChallengeRecord {
        challenger: challenger.clone(),
        bond,
    });
    env.storage()
        .persistent()
        .set(&StorageKey::Challengers(candidate_id), &challengers);
}

pub fn clear_challengers(env: &Env, candidate_id: u32) {
    env.storage()
        .persistent()
        .remove(&StorageKey::Challengers(candidate_id));
}

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().persistent().set(&StorageKey::Treasury, treasury);
}
