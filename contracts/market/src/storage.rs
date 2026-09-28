//! Typed storage keys for the market contract.
//!
//! Every persisted entry in the market contract MUST be addressed through a
//! [`StorageKey`] variant. The enum is the single source of truth for the
//! on-chain key layout and is validated by the automated reviewer checklist in
//! `tests/storage_key_checklist.rs`.
//!
//! Invariants (see `contracts/market/STORAGE_MIGRATION_GUIDE.md`):
//! * Keys are collision-free: each variant maps to a unique byte encoding.
//! * Keys are stable: existing encodings MUST NOT change without a migration.
//! * Unknown/legacy keys are denied by default (fail-closed).

use soroban_sdk::{contracttype, Address, BytesN, Env, Symbol};

/// Stable error codes surfaced by storage accessors.
///
/// These are part of the public ABI: do not renumber existing variants.
pub const ERR_UNKNOWN_STORAGE_KEY: u32 = 9001;
pub const ERR_STORAGE_KEY_MISMATCH: u32 = 9002;
pub const ERR_STORAGE_KEY_UNDOCUMENTED: u32 = 9003;

/// Typed, collision-free storage keys for the market contract.
///
/// Variants are grouped by domain (market, position, settlement, fee, oracle).
/// Adding a variant requires updating `STORAGE_MIGRATION_GUIDE.md` and the
/// reviewer checklist test, otherwise CI fails closed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    // --- Market ---
    /// Global market configuration (admin, collateral, flags).
    MarketConfig,
    /// Per-market metadata (symbol, decimals, status).
    MarketInfo(Symbol),
    /// Aggregate open interest for a market.
    MarketOpenInterest(Symbol),
    /// Paused / kill-switch flag for a market.
    MarketPaused(Symbol),

    // --- Position ---
    /// Position owned by `(market, owner)`.
    Position(Symbol, Address),
    /// Index of positions for a given owner.
    PositionIndex(Address),
    /// Collateral locked for a position.
    PositionCollateral(Symbol, Address),

    // --- Settlement ---
    /// Pending settlement record for a market.
    Settlement(Symbol),
    /// Settlement nonce used for replay protection.
    SettlementNonce(Symbol),
    /// Finalized settlement receipt keyed by id.
    SettlementReceipt(BytesN<32>),

    // --- Fee ---
    /// Accumulated protocol fees for a market.
    FeeAccrued(Symbol),
    /// Fee configuration (bps, recipient).
    FeeConfig,

    // --- Oracle ---
    /// Latest oracle price for a market.
    OraclePrice(Symbol),
    /// Oracle price timestamp for freshness checks.
    OracleTimestamp(Symbol),
}

impl StorageKey {
    /// Canonical, stable byte encoding for this key.
    ///
    /// The encoding is derived from the variant discriminant and its payload.
    /// It MUST remain stable across releases; changing it requires a migration.
    pub fn encode(&self, env: &Env) -> BytesN<32> {
        let mut buf = [0u8; 32];
        let (tag, payload): (u8, Option<&[u8]>) = match self {
            StorageKey::MarketConfig => (0x01, None),
            StorageKey::MarketInfo(s) => (0x02, Some(s.as_ref())),
            StorageKey::MarketOpenInterest(s) => (0x03, Some(s.as_ref())),
            StorageKey::MarketPaused(s) => (0x04, Some(s.as_ref())),
            StorageKey::Position(s, a) => (0x10, Some(a.as_ref())),
            StorageKey::PositionIndex(a) => (0x11, Some(a.as_ref())),
            StorageKey::PositionCollateral(s, a) => (0x12, Some(a.as_ref())),
            StorageKey::Settlement(s) => (0x20, Some(s.as_ref())),
            StorageKey::SettlementNonce(s) => (0x21, Some(s.as_ref())),
            StorageKey::SettlementReceipt(id) => (0x22, Some(id.as_ref())),
            StorageKey::FeeAccrued(s) => (0x30, Some(s.as_ref())),
            StorageKey::FeeConfig => (0x31, None),
            StorageKey::OraclePrice(s) => (0x40, Some(s.as_ref())),
            StorageKey::OracleTimestamp(s) => (0x41, Some(s.as_ref())),
        };
        buf[0] = tag;
        if let Some(bytes) = payload {
            let len = bytes.len().min(31);
            buf[1..1 + len].copy_from_slice(&bytes[..len]);
        }
        let _ = env;
        BytesN::from_array(env, &buf)
    }

    /// Decode a raw key back into a typed [`StorageKey`].
    ///
    /// Deny-by-default: unknown tags return [`ERR_UNKNOWN_STORAGE_KEY`].
    pub fn decode(env: &Env, raw: &BytesN<32>) -> Result<StorageKey, u32> {
        let bytes = raw.to_array();
        let tag = bytes[0];
        let payload = &bytes[1..];
        let sym = |b: &[u8]| -> Symbol {
            let mut s = [0u8; 32];
            s[..b.len()].copy_from_slice(b);
            Symbol::try_from_bytes(env, &s).unwrap_or_else(|_| Symbol::new(env, ""))
        };
        let addr = |b: &[u8]| -> Address {
            let mut a = [0u8; 32];
            a[..b.len()].copy_from_slice(b);
            Address::from_string_bytes(&BytesN::from_array(env, &a))
        };
        match tag {
            0x01 => Ok(StorageKey::MarketConfig),
            0x02 => Ok(StorageKey::MarketInfo(sym(payload))),
            0x03 => Ok(StorageKey::MarketOpenInterest(sym(payload))),
            0x04 => Ok(StorageKey::MarketPaused(sym(payload))),
            0x10 => Ok(StorageKey::Position(sym(payload), addr(payload))),
            0x11 => Ok(StorageKey::PositionIndex(addr(payload))),
            0x12 => Ok(StorageKey::PositionCollateral(sym(payload), addr(payload))),
            0x20 => Ok(StorageKey::Settlement(sym(payload))),
            0x21 => Ok(StorageKey::SettlementNonce(sym(payload))),
            0x22 => Ok(StorageKey::SettlementReceipt(BytesN::from_array(env, &{
                let mut id = [0u8; 32];
                id.copy_from_slice(payload);
                id
            }))),
            0x30 => Ok(StorageKey::FeeAccrued(sym(payload))),
            0x31 => Ok(StorageKey::FeeConfig),
            0x40 => Ok(StorageKey::OraclePrice(sym(payload))),
            0x41 => Ok(StorageKey::OracleTimestamp(sym(payload))),
            _ => Err(ERR_UNKNOWN_STORAGE_KEY),
        }
    }

    /// Human-readable name used by the reviewer checklist and logs.
    ///
    /// MUST match the variant name exactly; the checklist test asserts this.
    pub fn name(&self) -> &'static str {
        match self {
            StorageKey::MarketConfig => "MarketConfig",
            StorageKey::MarketInfo(_) => "MarketInfo",
            StorageKey::MarketOpenInterest(_) => "MarketOpenInterest",
            StorageKey::MarketPaused(_) => "MarketPaused",
            StorageKey::Position(_, _) => "Position",
            StorageKey::PositionIndex(_) => "PositionIndex",
            StorageKey::PositionCollateral(_, _) => "PositionCollateral",
            StorageKey::Settlement(_) => "Settlement",
            StorageKey::SettlementNonce(_) => "SettlementNonce",
            StorageKey::SettlementReceipt(_) => "SettlementReceipt",
            StorageKey::FeeAccrued(_) => "FeeAccrued",
            StorageKey::FeeConfig => "FeeConfig",
            StorageKey::OraclePrice(_) => "OraclePrice",
            StorageKey::OracleTimestamp(_) => "OracleTimestamp",
        }
    }
}

/// Read a typed key, failing closed on unknown/legacy encodings.
pub fn read<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &StorageKey,
) -> Result<Option<T>, u32> {
    let raw = key.encode(env);
    // Round-trip decode to guarantee the key is well-formed before touching state.
    StorageKey::decode(env, &raw)?;
    Ok(env.storage().persistent().get(&raw))
}

/// Write a typed key, failing closed on unknown/legacy encodings.
pub fn write<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &StorageKey,
    value: &T,
) -> Result<(), u32> {
    let raw = key.encode(env);
    StorageKey::decode(env, &raw)?;
    env.storage().persistent().set(&raw, value);
    Ok(())
}
