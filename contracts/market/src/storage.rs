use crate::error::ContractError;
use crate::types::{EmergencyMode, Market, PendingFeeRateChange, Position};
use soroban_sdk::{contracttype, Address, BytesN, Env, Vec};

/// Bump this constant whenever the storage layout changes in a breaking way.
/// `initialize()` writes this value; every storage accessor asserts it.
///
/// # Migration Guide
///
/// **IMPORTANT:** See `STORAGE_MIGRATION_GUIDE.md` for comprehensive documentation
/// on when and how to bump this version, including:
/// - When to increment the version
/// - Step-by-step migration procedures for testnet and mainnet
/// - Testing strategies
/// - Rollback and recovery procedures
/// - Common pitfalls and how to avoid them
///
/// # Quick Reference
///
/// ## Always bump version when:
/// - Adding/removing fields in storage types (Market, Position, etc.)
/// - Changing field types or semantics
/// - Adding new StorageKey variants
/// - Changing how existing data is computed or interpreted
///
/// ## Migration procedure (testnet):
/// 1. Increment `STORAGE_VERSION` in this file
/// 2. Document the change in `MIGRATION.md`
/// 3. Build the contract: `stellar contract build`
/// 4. Deploy: `stellar contract deploy --wasm <path> --network testnet`
/// 5. Initialize: `stellar contract invoke ... -- initialize --admin <addr>`
/// 6. Verify old deployment returns `UpgradeRequired` error
///
/// ## Current version: 6
///
/// ### Version history:
/// - **v6:** Added `CollateralBalance(Address)` and
///   `TotalLockedCollateral(Address)` — the protocol-wide, user-scoped
///   collateral ledger introduced by ADR-002 (issue #685). See
///   `docs/adr-002-protocol-wide-collateral.md`.
/// - **v5:** Added `EmergencyMode` storage for coordinated emergency mode (#662)
/// - **v4:** Added per-adapter-type `AdapterEnabled` flag for the Reflector/Pyth
///   Ed25519 fallback path (#488)
/// - **v3:** Added Treasury, Outcome Token, Resolution Contract, Threshold Signers
/// - **v2:** Fixed locked_collateral semantics (#262)
/// - **v1:** Initial storage layout
///
/// See `STORAGE_MIGRATION_GUIDE.md` and `MIGRATION.md` for detailed history.
pub const STORAGE_VERSION: u32 = 6;

#[contracttype]
pub enum StorageKey {
    StorageVersion,
    Market(u32),
    Position(u32, Address),
    Admin,
    PendingAdmin,
    MarketCounter,
    /// Address of the deployed treasury contract that protocol fees are routed
    /// to. Optional — fees are only forwarded when this is populated and the
    /// computed fee_amount is greater than zero.
    Treasury,
    /// Withdrawal fee rate in basis points (0–10_000). Read in the withdraw
    /// path to compute the protocol fee; defaults to 0 when unset.
    FeeRateBps,
    /// Address of the deployed outcome-token contract. When set, `update_position`
    /// mints/burns outcome tokens to reflect share balance changes.
    OutcomeTokenContract,
    /// Address of the deployed resolution contract that gates resolve_market.
    ResolutionContract,
    /// Ordered list of oracle public keys forming the multi-signer quorum (#378).
    ThresholdSigners,
    /// Minimum number of valid signatures required to resolve a market (#378).
    ThresholdQuorum,
    /// Flag indicating the contract is paused for emergency maintenance.
    /// When true, all state-mutating operations are rejected.
    Paused,
    /// Whether the Reflector/Pyth adapter for a given [`AdapterType`] is live.
    /// Defaults to `false` (disabled) when unset — see #488: while disabled,
    /// `resolve_market` falls back to direct Ed25519 verification against the
    /// market's `oracle_pubkey` instead of routing through the adapter.
    AdapterEnabled(crate::types::AdapterType),
    /// Reentrancy lock for `deposit_collateral` (Issue #501). Set while a
    /// deposit's external token transfer is in flight so a reentrant call
    /// back into `deposit_collateral` from that transfer is rejected.
    DepositLock,
    /// Ordered list of every distinct address that has ever held a position
    /// in a market (Issue #495). Enables paginated settlement of markets with
    /// too many positions to settle — or even enumerate off-chain — in a
    /// single transaction.
    MarketParticipants(u32),
    /// Pending fee-rate change awaiting its timelock delay (Issue #496).
    PendingFeeRate,
    /// Timestamp of the last deposit made by a user in a market (issue #413).
    /// Used to enforce the withdrawal cooldown period.
    LastDepositTime(u32, Address),
    /// Flag indicating a pending admin renounce proposal (issue #414).
    /// Set when an admin initiates a renounce; cleared on confirm or cancel.
    PendingRenounce,
    /// Admin-managed list of addresses exempt from withdrawal fees (Issue #483).
    FeeWaivers,
    /// Hard upper bound on the withdrawal fee rate in basis points.
    /// Prevents the admin from setting a fee rate above this cap.
    FeeCap,
    /// Ordered list of all market IDs ever created (append-only).
    /// Used by off-chain indexers to enumerate all markets.
    MarketIds,
    PendingTreasury,
    PendingOutcomeToken,
    PendingResolution,
    PendingMarketOracle(u32),
    /// Flag indicating whether legacy V1 oracle signatures are disabled.
    /// Admin-controlled toggle for mainnet security compliance.
    OracleV1Disabled,
    /// Pending threshold signers and quorum update awaiting timelock delay (#665).
    PendingThresholdSigners,
    /// Per-market threshold signers override (#665).
    MarketThresholdSigners(u32),
    /// Per-market threshold quorum override (#665).
    MarketThresholdQuorum(u32),
    /// Protocol-wide collateral balance for a user, scoped by user only —
    /// **not** by market (ADR-002, issue #685). Deposits made via
    /// `deposit_collateral` credit this balance regardless of which market
    /// they were deposited against, and it is the shared pool checked
    /// against when a trade in *any* market would increase that market's
    /// locked collateral. This replaces the old per-market silo where a
    /// user had to re-deposit collateral separately for every market.
    CollateralBalance(Address),
    /// Aggregate `locked_collateral` across every market for a user
    /// (ADR-002, issue #685). Kept in sync by
    /// `MarketContract::update_position` so the protocol-wide invariant
    /// (`sum of locked_collateral across all markets <= CollateralBalance`)
    /// can be checked in O(1) instead of iterating every market the user
    /// has ever traded in.
    TotalLockedCollateral(Address),
}

pub fn get_pending_threshold_signers(
    env: &Env,
) -> Option<crate::types::PendingThresholdSignersChange> {
    env.storage()
        .persistent()
        .get(&StorageKey::PendingThresholdSigners)
}

pub fn set_pending_threshold_signers(
    env: &Env,
    pending: &crate::types::PendingThresholdSignersChange,
) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingThresholdSigners, pending);
}

pub fn clear_pending_threshold_signers(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingThresholdSigners);
}

pub fn get_market_threshold_signers(env: &Env, market_id: u32) -> Option<Vec<BytesN<32>>> {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketThresholdSigners(market_id))
}

pub fn set_market_threshold_signers(env: &Env, market_id: u32, signers: &Vec<BytesN<32>>) {
    env.storage()
        .persistent()
        .set(&StorageKey::MarketThresholdSigners(market_id), signers);
}

pub fn get_market_threshold_quorum(env: &Env, market_id: u32) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketThresholdQuorum(market_id))
}

pub fn set_market_threshold_quorum(env: &Env, market_id: u32, quorum: u32) {
    env.storage()
        .persistent()
        .set(&StorageKey::MarketThresholdQuorum(market_id), &quorum);
}

pub fn get_threshold_signers(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&StorageKey::ThresholdSigners)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_threshold_signers(env: &Env, signers: &Vec<BytesN<32>>) {
    env.storage()
        .persistent()
        .set(&StorageKey::ThresholdSigners, signers);
}

pub fn get_threshold_quorum(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&StorageKey::ThresholdQuorum)
        .unwrap_or(0)
}

pub fn set_threshold_quorum(env: &Env, quorum: u32) {
    env.storage()
        .persistent()
        .set(&StorageKey::ThresholdQuorum, &quorum);
}

pub fn set_oracle_v1_disabled(env: &Env, disabled: bool) {
    env.storage()
        .instance()
        .set(&StorageKey::OracleV1Disabled, &disabled);
}

pub fn is_oracle_v1_disabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::OracleV1Disabled)
        .unwrap_or(false)
}

// --- Version helpers ---

pub fn set_version(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

pub fn assert_version(env: &Env) -> Result<(), ContractError> {
    let on_chain: Option<u32> = env.storage().persistent().get(&StorageKey::StorageVersion);
    if on_chain != Some(STORAGE_VERSION) {
        return Err(ContractError::UpgradeRequired);
    }
    Ok(())
}

// --- Market Storage ---

pub fn get_market(env: &Env, market_id: u32) -> Result<Option<Market>, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Market(market_id)))
}

pub fn set_market(env: &Env, market_id: u32, market: &Market) -> Result<(), ContractError> {
    assert_version(env)?;
    crate::validation::validate_outcome_count(market.outcome_count)?;
    env.storage()
        .persistent()
        .set(&StorageKey::Market(market_id), market);
    Ok(())
}

pub fn has_market(env: &Env, market_id: u32) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .has(&StorageKey::Market(market_id)))
}

// --- Position Storage ---

pub fn get_position(
    env: &Env,
    market_id: u32,
    user: &Address,
) -> Result<Option<Position>, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Position(market_id, user.clone())))
}

pub fn set_position(
    env: &Env,
    market_id: u32,
    user: &Address,
    position: &Position,
) -> Result<(), ContractError> {
    assert_version(env)?;
    env.storage()
        .persistent()
        .set(&StorageKey::Position(market_id, user.clone()), position);
    Ok(())
}

pub fn has_position(env: &Env, market_id: u32, user: &Address) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .has(&StorageKey::Position(market_id, user.clone())))
}

// --- Protocol-wide Collateral (ADR-002, Issue #685) ---

/// Return `user`'s protocol-wide collateral balance (the sum of everything
/// they have deposited across every market via `deposit_collateral`).
/// Defaults to `0` when the user has never deposited.
pub fn get_collateral_balance(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::CollateralBalance(user.clone()))
        .unwrap_or(0)
}

/// Set `user`'s protocol-wide collateral balance.
pub fn set_collateral_balance(env: &Env, user: &Address, balance: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::CollateralBalance(user.clone()), &balance);
}

/// Return the aggregate `locked_collateral` across every market for `user`.
/// Defaults to `0` when the user has no open positions.
pub fn get_total_locked_collateral(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::TotalLockedCollateral(user.clone()))
        .unwrap_or(0)
}

/// Set the aggregate `locked_collateral` across every market for `user`.
pub fn set_total_locked_collateral(env: &Env, user: &Address, locked: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::TotalLockedCollateral(user.clone()), &locked);
}

// --- Market Participants (Issue #495 / Issue #768) ---

/// Maximum allowed participants per market to bound storage and execution limits (#768).
pub const MAX_MARKET_PARTICIPANTS: u32 = 1000;

/// Return the ordered list of every address that has ever held a position
/// in `market_id`. Empty if the market has no positions yet.
pub fn get_market_participants(env: &Env, market_id: u32) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketParticipants(market_id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Record `user` as a participant of `market_id` if not already tracked and
/// under the storage limit (`MAX_MARKET_PARTICIPANTS`).
/// Idempotent — safe to call on every position update.
pub fn add_market_participant(env: &Env, market_id: u32, user: &Address) {
    let mut participants = get_market_participants(env, market_id);
    if !participants.iter().any(|p| &p == user) {
        if participants.len() < MAX_MARKET_PARTICIPANTS {
            participants.push_back(user.clone());
            env.storage()
                .persistent()
                .set(&StorageKey::MarketParticipants(market_id), &participants);
        }
    }
}

/// Number of distinct addresses that have ever held a position in `market_id`.
pub fn get_market_participant_count(env: &Env, market_id: u32) -> u32 {
    get_market_participants(env, market_id).len()
}

// --- Admin Storage ---

pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Admin)
        .expect("Admin not set"))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&StorageKey::Admin, admin);
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Admin)
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingAdmin, admin);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingAdmin);
}

// --- Market Counter ---

pub fn get_next_market_id(env: &Env) -> Result<u32, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::MarketCounter)
        .unwrap_or(0))
}

pub fn increment_market_id(env: &Env) -> Result<u32, ContractError> {
    let next_id = get_next_market_id(env)? + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::MarketCounter, &next_id);
    Ok(next_id)
}

// --- Treasury Storage ---

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::Treasury)
}

/// Register (or replace) the treasury contract address for protocol fee routing.
pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::Treasury, treasury);
}

pub fn has_treasury(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Treasury)
}

pub fn get_pending_treasury(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage().persistent().get(&StorageKey::PendingTreasury)
}

pub fn set_pending_treasury(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingTreasury, pending);
}

pub fn clear_pending_treasury(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingTreasury);
}

// --- Outcome Token Storage ---

pub fn get_outcome_token_contract(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::OutcomeTokenContract)
}

pub fn set_outcome_token_contract(env: &Env, contract: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::OutcomeTokenContract, contract);
}

pub fn get_pending_outcome_token_contract(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage()
        .persistent()
        .get(&StorageKey::PendingOutcomeToken)
}

pub fn set_pending_outcome_token_contract(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingOutcomeToken, pending);
}

pub fn clear_pending_outcome_token_contract(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingOutcomeToken);
}

// --- Deposit Timestamp Storage (issue #413) ---

pub fn get_last_deposit_time(env: &Env, market_id: u32, user: &Address) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::LastDepositTime(market_id, user.clone()))
}

pub fn set_last_deposit_time(env: &Env, market_id: u32, user: &Address, timestamp: u64) {
    env.storage().persistent().set(
        &StorageKey::LastDepositTime(market_id, user.clone()),
        &timestamp,
    );
}

// --- Pending Renounce Storage (issue #414) ---

pub fn has_pending_renounce(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::PendingRenounce)
}

pub fn set_pending_renounce(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingRenounce, &true);
}

pub fn clear_pending_renounce(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingRenounce);
}

pub fn clear_admin(env: &Env) {
    env.storage().persistent().remove(&StorageKey::Admin);
}

// --- Fee Config Storage ---

pub fn get_fee_rate_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::FeeRateBps)
        .unwrap_or(DEFAULT_FEE_RATE_BPS)
}

pub fn set_fee_rate_bps(env: &Env, fee_rate_bps: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::FeeRateBps, &fee_rate_bps);
}

// --- Pending Fee Rate Change / Timelock (Issue #496) ---

pub fn get_pending_fee_rate_change(env: &Env) -> Option<PendingFeeRateChange> {
    env.storage().persistent().get(&StorageKey::PendingFeeRate)
}

pub fn set_pending_fee_rate_change(env: &Env, pending: &PendingFeeRateChange) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingFeeRate, pending);
}

pub fn clear_pending_fee_rate_change(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingFeeRate);
}

// --- Pause Storage ---

/// Check whether the contract is in a paused state.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

/// Pause or unpause the contract (emergency halt).
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().persistent().set(&StorageKey::Paused, &paused);
}

// --- Emergency Mode (Issue #662) ---

/// Return the current coordinated emergency mode. Defaults to `Normal` when
/// unset (freshly initialized contract has never had the mode changed).
pub fn get_emergency_mode(env: &Env) -> EmergencyMode {
    env.storage()
        .persistent()
        .get(&StorageKey::EmergencyMode)
        .unwrap_or(EmergencyMode::Normal)
}

/// Set the coordinated emergency mode. Only the admin may call this (enforced
/// in `lib.rs`).
pub fn set_emergency_mode(env: &Env, mode: &EmergencyMode) {
    env.storage()
        .persistent()
        .set(&StorageKey::EmergencyMode, mode);
}

// --- Oracle Adapter Enabled Flag (#488) ---

pub fn get_pending_market_oracle(
    env: &Env,
    market_id: u32,
) -> Option<crate::types::PendingBytesNChange> {
    env.storage()
        .persistent()
        .get(&StorageKey::PendingMarketOracle(market_id))
}

pub fn set_pending_market_oracle(
    env: &Env,
    market_id: u32,
    pending: &crate::types::PendingBytesNChange,
) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingMarketOracle(market_id), pending);
}

pub fn clear_pending_market_oracle(env: &Env, market_id: u32) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingMarketOracle(market_id));
}

/// Whether the given adapter type is live for resolution.
///
/// Defaults to `false` (disabled) when never explicitly configured, which is
/// the correct default today since the Reflector/Pyth on-chain integration is
/// not yet wired into `resolve_market` (tracked under #139). While disabled,
/// callers fall back to direct Ed25519 verification — see
/// [`crate::oracle::verify_market_outcome`].
pub fn is_adapter_enabled(env: &Env, adapter_type: &crate::types::AdapterType) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::AdapterEnabled(adapter_type.clone()))
        .unwrap_or(false)
}

/// Enable or disable the given adapter type (admin-gated in `lib.rs`).
pub fn set_adapter_enabled(env: &Env, adapter_type: &crate::types::AdapterType, enabled: bool) {
    env.storage()
        .persistent()
        .set(&StorageKey::AdapterEnabled(adapter_type.clone()), &enabled);
}

// --- Per-market Adapter Config (#681) ---

/// Fetch the stored Reflector/Pyth adapter config for `market_id`, if any.
///
/// Returns `None` when the admin has not configured an adapter for this
/// market yet — callers should fall back to Ed25519 verification rather than
/// treating this as an error (see `oracle::verify_market_outcome`).
/// Only available when the `oracle-adapter` feature is compiled in (#778).
#[cfg(feature = "oracle-adapter")]
pub fn get_market_adapter_config(
    env: &Env,
    market_id: u32,
) -> Option<crate::types::MarketAdapterConfig> {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketAdapterConfig(market_id))
}

/// Set (or replace) the Reflector/Pyth adapter config for `market_id`
/// (admin-gated in `lib.rs`).
/// Only available when the `oracle-adapter` feature is compiled in (#778).
#[cfg(feature = "oracle-adapter")]
pub fn set_market_adapter_config(
    env: &Env,
    market_id: u32,
    config: &crate::types::MarketAdapterConfig,
) {
    env.storage()
        .persistent()
        .set(&StorageKey::MarketAdapterConfig(market_id), config);
}

// --- Deposit Reentrancy Lock (Issue #501) ---

/// Check whether the deposit reentrancy lock is currently held.
pub fn is_deposit_locked(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get(&StorageKey::DepositLock)
        .unwrap_or(false)
}

/// Acquire or release the deposit reentrancy lock.
pub fn set_deposit_locked(env: &Env, locked: bool) {
    env.storage()
        .persistent()
        .set(&StorageKey::DepositLock, &locked);
}

// --- Resolution Contract Storage ---

pub fn get_resolution_contract(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::ResolutionContract)
}

pub fn set_resolution_contract(env: &Env, contract: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::ResolutionContract, contract);
}

pub fn get_pending_resolution_contract(env: &Env) -> Option<crate::types::PendingAddressChange> {
    env.storage()
        .persistent()
        .get(&StorageKey::PendingResolution)
}

pub fn set_pending_resolution_contract(env: &Env, pending: &crate::types::PendingAddressChange) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingResolution, pending);
}

pub fn clear_pending_resolution_contract(env: &Env) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingResolution);
}

// --- Fee Waiver Storage (Issue #483) ---

/// Default withdrawal fee rate in basis points (50 bps = 0.5%).
pub const DEFAULT_FEE_RATE_BPS: i128 = 50;

/// Maximum withdrawal fee rate in basis points (10_000 bps = 100%).
pub const MAX_FEE_RATE_BPS: i128 = 10_000;

/// Return the list of addresses currently exempt from withdrawal fees.
pub fn get_fee_waivers(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::FeeWaivers)
        .unwrap_or_else(|| Vec::new(env))
}

/// Store the full fee waiver list (replaces the previous list).
pub fn set_fee_waivers(env: &Env, waivers: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&StorageKey::FeeWaivers, waivers);
}

/// Return whether `account` is currently on the fee waiver list.
pub fn is_fee_waived(env: &Env, account: &Address) -> bool {
    let waivers = get_fee_waivers(env);
    waivers.contains(account)
}

/// Add `account` to the fee waiver list (idempotent).
pub fn add_fee_waiver(env: &Env, account: &Address) {
    let mut waivers = get_fee_waivers(env);
    if !waivers.contains(account) {
        waivers.push_back(account.clone());
        set_fee_waivers(env, &waivers);
    }
}

/// Remove `account` from the fee waiver list (no-op if not present).
pub fn remove_fee_waiver(env: &Env, account: &Address) {
    let waivers = get_fee_waivers(env);
    let mut new_waivers = Vec::new(env);
    for w in waivers.iter() {
        if &w != account {
            new_waivers.push_back(w);
        }
    }
    set_fee_waivers(env, &new_waivers);
}

// --- Fee Cap Storage ---

/// Hard upper bound on the fee rate (defaults to `MAX_FEE_RATE_BPS` when unset).
pub fn get_fee_cap_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::FeeCap)
        .unwrap_or(MAX_FEE_RATE_BPS)
}

/// Set the hard upper bound on the fee rate (admin-gated in `lib.rs`).
pub fn set_fee_cap_bps(env: &Env, cap_bps: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::FeeCap, &cap_bps);
}

/// Alias for `get_all_market_ids` used by `list_markets`.
pub fn get_market_ids(env: &Env) -> Vec<u32> {
    get_all_market_ids(env)
}

/// Append `market_id` to the global ordered market-ID list.
///
/// Used by off-chain indexers that iterate over all markets via
/// `get_all_market_ids`. The list is append-only; market IDs are never
/// removed so the ordering matches creation order.
pub fn append_market_id(env: &Env, market_id: u32) {
    let mut ids: Vec<u32> = env
        .storage()
        .persistent()
        .get(&StorageKey::MarketIds)
        .unwrap_or_else(|| Vec::new(env));
    ids.push_back(market_id);
    env.storage().persistent().set(&StorageKey::MarketIds, &ids);
}

/// Return the ordered list of all market IDs ever created.
pub fn get_all_market_ids(env: &Env) -> Vec<u32> {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketIds)
        .unwrap_or_else(|| Vec::new(env))
}

// --- Oracle Adapter Support ---
// CRITICAL: When oracle adapters are enabled (non-empty), Ed25519 fallback MUST be disabled.
// This enforces a fail-closed security model. See ADR-002 in docs/.

/// Check if any oracle adapters are registered.
/// If true, Ed25519 signature verification should not be used as fallback.
pub fn has_oracle_adapters(env: &Env) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::OracleAdapters)
}

/// Register that oracle adapters are enabled for this market contract.
/// This disables Ed25519 fallback (fail-closed).
pub fn enable_oracle_adapters(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::OracleAdapters, &true);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::{AdapterType, MarketStatus};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::String;

    fn init_versioned(env: &Env, contract_id: &Address) {
        env.as_contract(contract_id, || set_version(env));
    }

    #[test]
    fn test_wrong_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_missing_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    /// Lightweight CI guard: `STORAGE_MIGRATION_GUIDE.md` must document the
    /// current `STORAGE_VERSION` with a `### Version {N} (Current)` heading,
    /// and exactly one entry may be marked current.
    ///
    /// Storage version bump checklist (do this whenever `STORAGE_VERSION` in
    /// this file is incremented):
    /// 1. Add a new `### Version {N} (Current)` section to
    ///    `STORAGE_MIGRATION_GUIDE.md`'s "Version History", describing what
    ///    changed, the migration path, and whether it's breaking.
    /// 2. Demote the previous `(Current)` entry to a plain `### Version {N}`
    ///    heading (no `(Current)` suffix).
    /// 3. Update the version history comment block above `STORAGE_VERSION`
    ///    in this file to match.
    /// 4. Re-run this test — bumping the constant without touching the guide
    ///    fails the build here, before it fails in review.
    #[test]
    fn test_storage_version_documented_in_migration_guide() {
        extern crate std;
        use std::format;

        const GUIDE: &str = include_str!("../STORAGE_MIGRATION_GUIDE.md");

        let current_heading = format!("### Version {} (Current)", STORAGE_VERSION);
        assert!(
            GUIDE.contains(current_heading.as_str()),
            "STORAGE_MIGRATION_GUIDE.md is missing a '{}' heading — document \
             the breaking change and demote the previous (Current) entry \
             whenever STORAGE_VERSION is bumped",
            current_heading
        );

        // A stale duplicate "(Current)" marker left behind from a prior bump
        // would otherwise let the check above pass without the guide
        // actually being updated for the new version.
        let current_count = GUIDE.matches("(Current)").count();
        assert_eq!(
            current_count, 1,
            "exactly one Version History entry should be marked (Current); found {}",
            current_count
        );
    }

    #[test]
    fn test_admin_storage() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert!(!has_admin(&env));
            set_admin(&env, &admin);
            assert!(has_admin(&env));
            assert_eq!(get_admin(&env).unwrap(), admin);
        });
    }

    #[test]
    fn test_fee_rate_bps_defaults_to_50() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            assert_eq!(
                get_fee_rate_bps(&env),
                DEFAULT_FEE_RATE_BPS,
                "fee rate must default to 50 bps"
            );
        });
    }

    #[test]
    fn test_fee_rate_bps_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            set_fee_rate_bps(&env, 100);
            assert_eq!(get_fee_rate_bps(&env), 100);

            set_fee_rate_bps(&env, 0);
            assert_eq!(get_fee_rate_bps(&env), 0);

            set_fee_rate_bps(&env, MAX_FEE_RATE_BPS);
            assert_eq!(get_fee_rate_bps(&env), MAX_FEE_RATE_BPS);
        });
    }

    #[test]
    fn test_market_id_counter() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert_eq!(get_next_market_id(&env).unwrap(), 0);
            assert_eq!(increment_market_id(&env).unwrap(), 1);
            assert_eq!(increment_market_id(&env).unwrap(), 2);
        });
    }

    #[test]
    fn test_market_storage_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        let market_id = 1u32;
        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Will it rain?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(&env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token: Address::generate(&env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
            closed_to_deposits: false,
        };
        env.as_contract(&contract_id, || {
            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());
            let saved = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(saved.id, market.id);
        });
    }

    #[test]
    fn test_threshold_signers_default_empty() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert_eq!(get_threshold_signers(&env).len(), 0);
            assert_eq!(get_threshold_quorum(&env), 0);
        });
    }

    #[test]
    fn test_threshold_signers_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let collateral_token = Address::generate(&env);
        let market_id = 7u32;

        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Storage layout test question?"),
            end_time: 9_999_999_999u64,
            oracle_pubkey: BytesN::from_array(&env, &[0xABu8; 32]),
            status: crate::types::MarketStatus::Active,
            result: Some(true),
            creator: admin.clone(),
            created_at: 1_000_000u64,
            collateral_token: collateral_token.clone(),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
            closed_to_deposits: false,
        };

        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 250,
            no_shares: 75,
            locked_collateral: 325,
            total_deposited: 400,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            set_admin(&env, &admin);
            increment_market_id(&env).unwrap();

            assert_eq!(get_admin(&env).unwrap(), admin);
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());

            assert_eq!(get_admin(&env).unwrap(), admin);
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            let m = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m.id, market.id);
            assert_eq!(m.question, market.question);
            assert_eq!(m.end_time, market.end_time);
            assert_eq!(m.oracle_pubkey, market.oracle_pubkey);
            assert_eq!(m.result, market.result);
            assert_eq!(m.creator, market.creator);
            assert_eq!(m.created_at, market.created_at);
            assert_eq!(m.collateral_token, market.collateral_token);

            assert!(!has_position(&env, market_id, &user).unwrap());
            set_position(&env, market_id, &user, &position).unwrap();
            assert!(has_position(&env, market_id, &user).unwrap());

            let other_user = Address::generate(&env);
            assert!(!has_position(&env, market_id, &other_user).unwrap());

            let p = get_position(&env, market_id, &user).unwrap().unwrap();
            assert_eq!(p.market_id, position.market_id);
            assert_eq!(p.user, position.user);
            assert_eq!(p.yes_shares, position.yes_shares);
            assert_eq!(p.no_shares, position.no_shares);
            assert_eq!(p.locked_collateral, position.locked_collateral);
            assert_eq!(p.total_deposited, position.total_deposited);
            assert_eq!(p.is_settled, position.is_settled);

            let m2 = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m2.id, market.id);
        });
    }

    #[test]
    fn migration_missing_version_blocks_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let user = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
            assert_eq!(get_market(&env, 1), Err(ContractError::UpgradeRequired));
            assert_eq!(
                get_position(&env, 1, &user),
                Err(ContractError::UpgradeRequired)
            );
        });
    }

    #[test]
    fn migration_after_set_version_storage_is_accessible() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: 1,
            question: String::from_str(&env, "post-migration market?"),
            end_time: 9_000_000,
            oracle_pubkey: BytesN::from_array(&env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token,
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
            closed_to_deposits: false,
        };

        env.as_contract(&contract_id, || {
            set_version(&env);
            assert_eq!(assert_version(&env), Ok(()));

            set_market(&env, 1, &market).unwrap();
            let m = get_market(&env, 1).unwrap().unwrap();
            assert_eq!(m.id, 1);
        });
    }

    #[test]
    fn migration_future_version_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &(STORAGE_VERSION + 1));
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    // ── Treasury storage helpers ──────────────────────────────────────────────

    #[test]
    fn test_treasury_storage_set_and_get() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let treasury = Address::generate(&env);
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert!(!has_treasury(&env));
            assert_eq!(get_treasury(&env), None);

            set_treasury(&env, &treasury);
            assert!(has_treasury(&env));
            assert_eq!(get_treasury(&env), Some(treasury.clone()));
        });
    }

    // ── Resolution contract storage helpers ───────────────────────────────────

    #[test]
    fn test_resolution_contract_storage_set_and_get() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let resolution = Address::generate(&env);
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert_eq!(get_resolution_contract(&env), None);

            set_resolution_contract(&env, &resolution);
            assert_eq!(get_resolution_contract(&env), Some(resolution.clone()));
        });
    }

    // ── Pause storage helpers ─────────────────────────────────────────────

    #[test]
    fn test_pause_defaults_to_false() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert!(!is_paused(&env), "fresh contract should not be paused");
        });
    }

    #[test]
    fn test_pause_can_be_set_and_cleared() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            set_paused(&env, true);
            assert!(is_paused(&env));
            set_paused(&env, false);
            assert!(!is_paused(&env));
        });
    }

    #[test]
    fn test_pause_toggle_returns_to_unpaused() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            set_paused(&env, true);
            assert!(is_paused(&env));
            set_paused(&env, true);
            assert!(is_paused(&env), "pausing again should stay paused");
            set_paused(&env, false);
            assert!(!is_paused(&env));
        });
    }

    /// Canary test for issue #764 — ensures that the four modules whose
    /// dead-code suppression is required only due to `#[contractimpl]` macro
    /// hiding call-sites (`positions`, `settlement`, `storage`, `validation`)
    /// use the scoped `#[cfg_attr(not(test), allow(dead_code))]` form rather
    /// than a blanket `#[allow(dead_code)]`.
    ///
    /// A blanket `#[allow(dead_code)]` on any of these modules would silently
    /// hide genuinely unused admin entrypoints or storage helpers during
    /// non-audit code reviews.  This test fails immediately if the pattern
    /// regresses back to the unscoped form, catching it in `cargo test`
    /// before it reaches a review or audit.
    ///
    /// HOW THIS WORKS:
    ///   - `include_str!` embeds the raw source of `lib.rs` at compile time.
    ///   - We assert that the string `#[allow(dead_code)]` does not appear as
    ///     a standalone module-level attribute on the four guarded modules
    ///     (by confirming the *only* dead_code attributes for those modules
    ///     are the cfg_attr-scoped variants).
    ///   - A false positive here means someone deliberately removed the
    ///     cfg_attr wrapper — that needs an explicit, documented reason.
    #[test]
    fn test_no_bare_module_level_allow_dead_code_on_guarded_modules() {
        extern crate std;
        use std::string::ToString;

        const LIB_SRC: &str = include_str!("lib.rs");

        // The four modules that must use cfg_attr instead of a bare allow.
        // Each entry is (module_name, explanatory_substring) — the second
        // field confirms the cfg_attr annotation is present and correctly
        // references the module.
        let guarded = [
            "positions",
            "settlement",
            "storage",
            "validation",
        ];

        for module in guarded {
            // Confirm the scoped cfg_attr form IS present.
            let cfg_attr_form = "#[cfg_attr(not(test), allow(dead_code))]";
            assert!(
                LIB_SRC.contains(cfg_attr_form),
                "lib.rs is missing the scoped '{}' attribute — \
                 at least one of the guarded modules ({}) requires it; \
                 do not replace it with a bare #[allow(dead_code)] (#764)",
                cfg_attr_form,
                module
            );

            // Confirm there is no bare `#[allow(dead_code)]` immediately
            // followed (within 3 lines) by `mod {module}`.  We scan each
            // line window rather than the whole file so we only flag the
            // attribute when it directly precedes the module declaration.
            let lines: std::vec::Vec<&str> = LIB_SRC.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                // Check if this line is a bare (non-cfg_attr) dead_code allow
                let trimmed = line.trim();
                if trimmed == "#[allow(dead_code)]" {
                    // Look ahead up to 3 lines for the module declaration
                    let window_end = (i + 4).min(lines.len());
                    for ahead in &lines[i + 1..window_end] {
                        let ahead_trimmed = ahead.trim();
                        let mod_decl = std::format!("mod {};", module);
                        let pub_mod_decl = std::format!("pub mod {};", module);
                        if ahead_trimmed == mod_decl || ahead_trimmed == pub_mod_decl {
                            panic!(
                                "lib.rs has a bare #[allow(dead_code)] on `mod {}` \
                                 (line {}). Replace it with \
                                 `#[cfg_attr(not(test), allow(dead_code))]` \
                                 to scope the suppression to non-test builds only. \
                                 See issue #764.",
                                module,
                                i + 1
                            );
                        }
                    }
                }
            }
        }
    }
}
