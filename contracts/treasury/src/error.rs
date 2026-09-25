//! Error types for the treasury contract.
//!
//! Stable error codes are part of the public ABI: clients (and the
//! `distribute_proptest` property tests) match on these discriminants, so
//! existing variants must not be renumbered or removed.

use soroban_sdk::contracterror;

/// Errors returned by the treasury contract entrypoints.
///
/// Codes are grouped by surface:
/// - `1..=9`   : initialization / configuration
/// - `10..=19` : authorization / policy
/// - `20..=29` : `distribute` money path
/// - `30..=39` : accounting / invariants
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Contract has not been initialized yet.
    NotInitialized = 2,
    /// Caller is not the configured admin.
    Unauthorized = 10,
    /// Caller holds a role that is not permitted for this entrypoint.
    WrongRole = 11,
    /// The privileged surface is disabled by the kill-switch / feature flag.
    Disabled = 12,
    /// `distribute` was called with an empty recipient set.
    EmptyDistribution = 20,
    /// A recipient address appears more than once in a single distribution.
    DuplicateRecipient = 21,
    /// A per-recipient amount is zero or negative.
    InvalidAmount = 22,
    /// The requested distribution exceeds the available treasury balance.
    InsufficientBalance = 23,
    /// The distribution id has already been consumed (replay / double-spend).
    AlreadyDistributed = 24,
    /// Arithmetic overflow while summing or transferring amounts.
    Overflow = 30,
    /// Arithmetic underflow while debiting the treasury balance.
    Underflow = 31,
    /// Post-transfer accounting invariant was violated (conservation check).
    InvariantViolated = 32,
}
