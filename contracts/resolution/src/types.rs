use soroban_sdk::{contracttype, Address, BytesN, String};

/// Local mirror of `vatix_market::types::MarketStatus` (Issue #497).
///
/// The resolution crate cannot depend on the market crate directly (the
/// market crate already depends on this one), so this enum must be kept in
/// variant-for-variant sync with the market contract's `MarketStatus` for
/// cross-contract calls to `get_market_status` to decode correctly.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MarketStatus {
    Active,
    Resolved,
    Canceled,
}

/// Mirrored emergency mode coordinated with the Market contract (Issue #662).
/// Kept as a local mirror for the same reason as [`MarketStatus`] — the
/// resolution crate cannot depend on the market crate directly.
///
/// Defaults to `Normal` when never explicitly set. Only the admin may change
/// this value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum EmergencyMode {
    Normal,
    TradingHalted,
    SettleOnly,
    GlobalFreeze,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CandidateStatus {
    Proposed,
    Challenged,
    Finalized,
    /// Terminal state reached via `void_market`: the dispute exhausted
    /// `MAX_APPEAL_ROUNDS` and admin arbitration could not safely attest to
    /// either side's outcome on-chain, so the underlying market was voided
    /// (`MarketStatus::Canceled`) instead of resolved.
    Voided,
}

/// A single challenger's locked bond against a candidate. Appended to the
/// candidate's challenger list on every `challenge` call (including those
/// superseded by a later `appeal`), so that every party who disputed the
/// candidate across its whole lifecycle — not just the most recent one — is
/// accounted for when the dispute finally reaches a terminal state.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ChallengeRecord {
    pub challenger: Address,
    pub bond: i128,
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
    /// Number of times this candidate has been re-proposed via `appeal`
    /// after a challenge. Capped at `MAX_APPEAL_ROUNDS`.
    pub appeal_round: u32,
    /// Bond posted by the proposer (in the market's collateral token),
    /// locked in this contract and refunded to the proposer on finalize.
    pub bond_amount: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ResolutionConfig {
    pub admin: Address,
    pub factory: Address,
    pub market_contract: Address,
    /// Default challenge window in seconds. Must be within
    /// `MIN_CHALLENGE_WINDOW_SECONDS..=MAX_CHALLENGE_WINDOW_SECONDS`.
    /// (Renamed from `default_challenge_window_seconds` — Soroban enforces a
    /// 30-character maximum on struct field names.)
    pub challenge_window_secs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PendingAddressChange {
    pub new_address: Address,
    pub effective_at: u64,
}
