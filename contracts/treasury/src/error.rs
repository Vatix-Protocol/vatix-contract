use soroban_sdk::contracterror;

/// Error codes for the Vatix Treasury contract.
///
/// Grouped by category with reserved number ranges mirroring the market
/// contract's convention:
/// - Authorization Errors: 40-49
/// - Validation Errors:    30-39
/// - Arithmetic Errors:    60-69
///
/// # Example
/// ```ignore
/// match result {
///     Err(TreasuryError::Unauthorized)      => println!("Not the market contract"),
///     Err(TreasuryError::AlreadyInitialized) => println!("Init was already called"),
///     Ok(_) => {}
/// }
/// ```
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    // ========== Authorization Errors (40-49) ==========
    /// Caller is not the registered market contract.
    ///
    /// `collect_fee` may only be invoked by the address stored as
    /// `authorized_market` during initialization.
    Unauthorized = 40,

    /// Caller is not the treasury admin.
    ///
    /// `withdraw_fees` and admin-only operations require the stored admin.
    NotAdmin = 41,

    /// `initialize` has already been called.
    ///
    /// The treasury can only be bootstrapped once. Replaying it would allow
    /// an attacker to hijack the admin slot after initial deploy.
    AlreadyInitialized = 42,

    // ========== Validation Errors (30-39) ==========
    /// Amount is zero or negative.
    ///
    /// All fee amounts must be strictly positive.
    InvalidAmount = 31,

    /// The treasury has not been initialized yet.
    ///
    /// Call `initialize` before invoking any other entry point.
    NotInitialized = 33,

    // ========== Arithmetic Errors (60-69) ==========
    /// Arithmetic operation overflowed.
    ///
    /// The cumulative fee amount exceeded `i128::MAX`.
    ArithmeticOverflow = 60,
}

#[cfg(test)]
mod tests {
    use super::TreasuryError;

    #[test]
    fn discriminants_are_stable() {
        assert_eq!(TreasuryError::Unauthorized as u32, 40);
        assert_eq!(TreasuryError::NotAdmin as u32, 41);
        assert_eq!(TreasuryError::AlreadyInitialized as u32, 42);
        assert_eq!(TreasuryError::InvalidAmount as u32, 31);
        assert_eq!(TreasuryError::NotInitialized as u32, 33);
        assert_eq!(TreasuryError::ArithmeticOverflow as u32, 60);
    }
}
