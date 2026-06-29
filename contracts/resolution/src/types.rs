use soroban_sdk::{contracttype, Address, BytesN, String};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CandidateStatus {
    Proposed,
    Challenged,
    Finalized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ResolutionCandidate {
    pub id: u32,
    pub market_id: u32,
    pub outcome: bool,
    pub signature: BytesN<64>,
    pub signature_expiry: u64,
    pub proposer: Address,
    pub evidence_uri: String,
    pub proposed_at: u64,
    pub challenge_deadline: u64,
    pub status: CandidateStatus,
    pub challenged_by: Option<Address>,
    pub challenge_uri: Option<String>,
    pub finalized_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ResolutionConfig {
    pub admin: Address,
    pub factory: Address,
    pub market_contract: Address,
    /// Default challenge window in seconds. Must be within
    /// `MIN_CHALLENGE_WINDOW_SECONDS..=MAX_CHALLENGE_WINDOW_SECONDS`.
    pub default_challenge_window_seconds: u64,
}
