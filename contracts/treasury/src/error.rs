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

    /// The treasury has not been initialized yet.
    NotInitialized = 33,

    // ── Authorization (40–49) ─────────────────────────────────────────────────
    /// `collect_fee` was invoked by an address that is not the registered
    /// market contract.
    CallerNotMarket = 40,

    /// Caller is not the treasury admin.
    Unauthorized = 41,

    /// `initialize` has already been called.
    AlreadyInitialized = 42,

    // ── Pause (50–59) ─────────────────────────────────────────────────────────
    /// The treasury is paused; fee collection and withdrawals are suspended.
    ContractPaused = 50,

    // ── Arithmetic (60–69) ────────────────────────────────────────────────────
    /// Arithmetic operation overflowed.
    ArithmeticOverflow = 60,
}

#[cfg(test)]
mod tests {
    use super::TreasuryError;

    #[test]
    fn discriminants_are_stable() {
        assert_eq!(TreasuryError::UpgradeRequired as u32, 10);
        assert_eq!(TreasuryError::InsufficientBalance as u32, 21);
        assert_eq!(TreasuryError::InvalidAmount as u32, 31);
        assert_eq!(TreasuryError::NotInitialized as u32, 33);
        assert_eq!(TreasuryError::CallerNotMarket as u32, 40);
        assert_eq!(TreasuryError::Unauthorized as u32, 41);
        assert_eq!(TreasuryError::AlreadyInitialized as u32, 42);
        assert_eq!(TreasuryError::ArithmeticOverflow as u32, 60);
        assert_eq!(TreasuryError::ContractPaused as u32, 50);
    }
}
