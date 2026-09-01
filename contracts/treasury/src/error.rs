use soroban_sdk::contracterror;

/// Error codes for the Vatix Treasury contract.
///
/// Ranges:
/// - Initialization errors: 1–9  (NotInitialized=2, AlreadyInitialized=42 legacy alias)
/// - Amount / balance errors: 20–29
/// - Validation errors: 30–39
/// - Authorization errors: 40–49
/// - Arithmetic errors: 60–69
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    // ── Upgrade / migration (10–19) ───────────────────────────────────────────
    /// The on-chain storage version does not match the compiled contract version.
    /// A migration step must be performed before any storage reads will succeed.
    UpgradeRequired = 10,

    // ── Amount / balance (20–29) ──────────────────────────────────────────────
    /// The treasury does not hold enough of `token` to satisfy the withdrawal.
    InsufficientBalance = 21,

    // ── Validation (30–39) ────────────────────────────────────────────────────
    /// `fee_amount` or `amount` is zero or negative.
    InvalidAmount = 31,

    /// Stakeholder share list is empty, or the `share_bps` values do not sum
    /// to exactly 10_000 (100%).
    InvalidStakeholderWeights = 32,

    /// The treasury has not been initialized yet.
    NotInitialized = 33,

    /// `distribute_fees` was called but no stakeholder list has been
    /// configured via `execute_stakeholders`.
    NoStakeholdersConfigured = 34,

    /// `execute_stakeholders` / `cancel_stakeholders` was called but no
    /// stakeholder change is currently pending (Issue #689).
    NoPendingStakeholderChange = 35,

    // ── Authorization (40–49) ─────────────────────────────────────────────────
    /// `collect_fee` was invoked by an address that is not the registered
    /// market contract.
    CallerNotMarket = 40,

    /// Caller is not the treasury admin.
    Unauthorized = 41,

    /// `initialize` was called with a contract address as admin.
    /// Admin must be a user account (Ed25519 key), not a deployed contract.
    /// Mirrors `vatix_market_contract::ContractError::InvalidAdmin`.
    InvalidAdmin = 43,

    /// `initialize` has already been called.
    AlreadyInitialized = 42,

    // ── Pause (50–59) ─────────────────────────────────────────────────────────
    /// The treasury is paused; fee collection and withdrawals are suspended.
    ContractPaused = 50,

    /// The operation is blocked by the current emergency mode (Issue #662).
    /// Check [`crate::storage::get_emergency_mode`] for the active mode.
    EmergencyModeActive = 51,

    // ── Arithmetic (60–69) ────────────────────────────────────────────────────
    /// Arithmetic operation overflowed.
    ArithmeticOverflow = 60,

    // ── Timelock (70–79) ──────────────────────────────────────────────────────
    /// A pending timelocked change's `effective_at` has not been reached yet.
    TimelockNotElapsed = 70,
}

#[cfg(test)]
mod tests {
    use super::TreasuryError;

    #[test]
    fn discriminants_are_stable() {
        assert_eq!(TreasuryError::UpgradeRequired as u32, 10);
        assert_eq!(TreasuryError::InsufficientBalance as u32, 21);
        assert_eq!(TreasuryError::InvalidAmount as u32, 31);
        assert_eq!(TreasuryError::InvalidStakeholderWeights as u32, 32);
        assert_eq!(TreasuryError::NotInitialized as u32, 33);
        assert_eq!(TreasuryError::NoStakeholdersConfigured as u32, 34);
        assert_eq!(TreasuryError::CallerNotMarket as u32, 40);
        assert_eq!(TreasuryError::Unauthorized as u32, 41);
        assert_eq!(TreasuryError::AlreadyInitialized as u32, 42);
        assert_eq!(TreasuryError::ContractPaused as u32, 50);
        assert_eq!(TreasuryError::EmergencyModeActive as u32, 51);
        assert_eq!(TreasuryError::ArithmeticOverflow as u32, 60);
    }
}
