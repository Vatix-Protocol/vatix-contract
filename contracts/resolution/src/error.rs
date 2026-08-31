use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    CandidateNotFound = 1,
    CandidateAlreadyExists = 2,
    CandidateAlreadyChallenged = 3,
    CandidateAlreadyFinalized = 4,
    ChallengeWindowOpen = 5,
    ChallengeWindowClosed = 6,
    InvalidChallengeWindow = 7,
    InvalidEvidenceUri = 8,
    /// The provided signature has expired and can no longer be finalized.
    SignatureExpired = 9,
    /// The provided signature expiry timestamp is invalid (e.g. in the past).
    InvalidSignatureExpiry = 10,
    /// The underlying market is already resolved (or canceled) and can no
    /// longer accept a new resolution proposal (Issue #497).
    MarketAlreadyResolved = 11,
    /// `appeal` was called on a candidate that is not currently `Challenged`.
    CandidateNotChallenged = 12,
    /// A candidate has already been re-proposed `MAX_APPEAL_ROUNDS` times.
    AppealLimitExceeded = 13,
    /// `propose`'s `bond_amount` is below `MIN_BOND_AMOUNT`.
    InsufficientBond = 14,
    /// A proposer/challenger does not have enough deposited collateral for
    /// the requested operation.
    InsufficientCollateral = 15,
    /// A collateral amount is invalid (e.g. zero or negative).
    InvalidCollateral = 16,
    /// `challenge`'s `bond_amount` is below `MIN_CHALLENGE_BOND_AMOUNT`.
    InsufficientChallengeBond = 17,
    /// `arbitrate_uphold_proposer` / `void_market` was called on a candidate
    /// that is not `Challenged`, or has not yet exhausted
    /// `MAX_APPEAL_ROUNDS`.
    NotArbitrable = 18,
    /// `arbitrate_uphold_proposer` / `void_market` was called before the
    /// arbitration timelock elapsed.
    ArbitrationTimelockNotElapsed = 19,
    Unauthorized = 40,
    NotAdmin = 41,
    AlreadyInitialized = 42,
    /// `initialize` was called with a contract address as admin.
    /// Admin must be a user account (Ed25519 key), not a deployed contract.
    /// Mirrors `vatix_market_contract::ContractError::InvalidAdmin`.
    InvalidAdmin = 43,
    /// The operation is blocked by the current emergency mode (Issue #662).
    /// Check [`crate::storage::get_emergency_mode`] for the active mode.
    EmergencyModeActive = 50,
    /// The resolution contract is paused; state-mutating operations are suspended.
    ContractPaused = 52,
    /// The on-chain storage schema version does not match the version this
    /// contract build expects (Issue #696). Mirrors
    /// `vatix_market_contract::ContractError::UpgradeRequired` /
    /// `vatix_treasury_contract::TreasuryError::UpgradeRequired` — a partial
    /// cross-contract upgrade that leaves this deployment's storage on a
    /// stale schema fails closed here instead of silently corrupting state
    /// (e.g. via `finalize`).
    UpgradeRequired = 51,
}
