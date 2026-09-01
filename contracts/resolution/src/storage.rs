use crate::error::ContractError;
use crate::types::{ChallengeRecord, ResolutionCandidate, ResolutionConfig};
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Bump this constant whenever the resolution storage layout changes in a
/// breaking way. `initialize()` writes this value; state-mutating entry
/// points assert it via [`assert_version`] before touching storage — mirrors
/// the pattern already used by `contracts/market/src/storage.rs` and
/// `contracts/treasury/src/storage.rs` (Issue #696). Previously this crate
/// had no storage-version guard at all, so a partial cross-contract upgrade
/// could silently brick `finalize` against a stale on-chain layout instead
/// of failing closed with `UpgradeRequired`.
///
/// ## Version history
/// - **v1:** Initial versioned storage layout (Issue #696). No layout change
///   from the pre-versioning schema — this just adds the guard itself.
pub const STORAGE_VERSION: u32 = 1;

#[contracttype]
pub enum StorageKey {
    /// Written by `initialize`; used to detect stale or uninitialized deployments.
    StorageVersion,
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
    /// Pending treasury address awaiting its timelock delay.
    PendingTreasury,
    /// Pending factory address awaiting its timelock delay.
    PendingFactory,
    /// Pending market contract address awaiting its timelock delay.
    PendingMarketContract,
    /// Pending treasury address awaiting its timelock delay (Issue #687).
    PendingTreasury,
    /// Mirrored emergency mode (Issue #662).
    EmergencyMode,
}

// ── Version ───────────────────────────────────────────────────────────────

pub fn set_version(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn get_version(env: &Env) -> Option<u32> {
    env.storage().persistent().get(&StorageKey::StorageVersion)
}

/// Guard state-mutating entry points against a stale/pre-migration
/// deployment. Returns [`ContractError::UpgradeRequired`] when the on-chain
/// schema version does not match the compiled contract version.
pub fn assert_version(env: &Env) -> Result<(), ContractError> {
    if get_version(env) != Some(STORAGE_VERSION) {
        return Err(ContractError::UpgradeRequired);
    }
    Ok(())
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

pub fn get_pending_treasury(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage().persistent().get(&StorageKey::PendingTreasury)
}

pub fn set_pending_treasury(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage().persistent().set(&StorageKey::PendingTreasury, pending);
}

pub fn clear_pending_treasury(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingTreasury);
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

pub fn get_pending_treasury(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage().persistent().get(&StorageKey::PendingTreasury)
}

pub fn set_pending_treasury(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage().persistent().set(&StorageKey::PendingTreasury, pending);
}

pub fn clear_pending_treasury(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingTreasury);
}

pub fn get_emergency_mode(env: &Env) -> crate::types::EmergencyMode {
    env.storage()
        .persistent()
        .get(&StorageKey::EmergencyMode)
        .unwrap_or(crate::types::EmergencyMode::Normal)
}

pub fn set_emergency_mode(env: &Env, mode: &crate::types::EmergencyMode) {
    env.storage().persistent().set(&StorageKey::EmergencyMode, mode);
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_assert_version_passes_when_current() {
        let env = Env::default();
        let contract_id = env.register(crate::ResolutionContract, ());
        env.as_contract(&contract_id, || {
            set_version(&env);
            assert!(assert_version(&env).is_ok());
        });
    }

    #[test]
    fn test_assert_version_fails_when_stale() {
        let env = Env::default();
        let contract_id = env.register(crate::ResolutionContract, ());
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_assert_version_fails_when_missing() {
        let env = Env::default();
        let contract_id = env.register(crate::ResolutionContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_treasury_storage_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::ResolutionContract, ());
        let treasury = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(get_treasury(&env), None);
            set_treasury(&env, &treasury);
            assert_eq!(get_treasury(&env), Some(treasury.clone()));
        });
    }
}
