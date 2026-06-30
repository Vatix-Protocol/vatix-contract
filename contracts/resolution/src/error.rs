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
    Unauthorized = 40,
    NotAdmin = 41,
    AlreadyInitialized = 42,
}
