// TODO(#139): The oracle module currently uses a simple Ed25519 signature
// scheme with a single trusted pubkey stored per market. This needs to be
// replaced with a decentralised oracle integration (e.g. a Reflector or
// Pyth price-feed adapter) so that market resolution does not rely on a
// single off-chain signer. Tracked in:
// https://github.com/Vatix-Protocol/vatix-contract/issues/139
//
// CRITICAL SAFETY NOTE (Upgrade Order):
// Oracle adapters MUST be registered in this order across the protocol:
//   1. outcome-token contract (initializes)
//   2. treasury contract (initializes fee routes)
//   3. resolution contract (initializes adapters)
//   4. market contract (THIS CONTRACT - mints outcomes)
//
// Wrong order during upgrade bricks minting. See scripts/upgrade/UPGRADE_PLAYBOOK.md.
//
// SECURITY MODEL: When oracle adapters are enabled, Ed25519 verification
// is DISABLED (fail-closed). This prevents silent fallback during incomplete upgrades.

//! # Canonical Oracle Message Format (V2)
//!
//! The exact bytes the oracle must sign in V2 are:
//!
//! ```text
//! message = keccak256(domain_separator || network_passphrase_hash || market_id_be || outcome_byte || valid_until_be || epoch_be)
//! ```
//!
//! | Field                     | Encoding                          | Width   |
//! |---------------------------|-----------------------------------|---------|
//! | `domain_separator`        | ASCII `"VATIX_ORACLE_V2"`         | 15 bytes|
//! | `network_passphrase_hash` | `[u8; 32]` network identifier     | 32 bytes|
//! | `market_id_be`            | `u32` as **big-endian** bytes     | 4 bytes |
//! | `outcome_byte`            | `0x01` = YES / `0x00` = NO        | 1 byte  |
//! | `valid_until_be`          | `u64` as **big-endian** bytes     | 8 bytes |
//! | `epoch_be`                | `u32` as **big-endian** bytes     | 4 bytes |
//!
//! The V2 preimage is always exactly [`ORACLE_PREIMAGE_LEN_V2`] bytes (64 bytes).
//!
//! **Backend alignment**: the backend signer MUST prepend `VATIX_ORACLE_V2`,
//! concatenate all fields in sequence, and keccak256-hash the result.
//! Use `test-vectors/oracle-message.json` to verify signing integration.

use crate::error::ContractError;
use crate::types::{AdapterType, Market};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use soroban_sdk::{Bytes, BytesN, Env};

/// Domain separator prepended to legacy V1 oracle preimages before hashing.
pub const ORACLE_DOMAIN_SEPARATOR: &[u8] = b"VATIX_ORACLE_V1";

/// Domain separator prepended to V2 oracle preimages before hashing.
///
/// Binds a signature to *this* V2 layout (including network, valid_until expiry, and market epoch)
/// so signatures produced for testnet or other market states cannot be replayed.
pub const ORACLE_DOMAIN_SEPARATOR_V2: &[u8] = b"VATIX_ORACLE_V2";

/// Exact width, in bytes, of the legacy V1 oracle preimage before hashing:
/// `domain_separator (15) || market_id_be (4) || outcome_byte (1)`.
pub const ORACLE_PREIMAGE_LEN: usize = ORACLE_DOMAIN_SEPARATOR.len() + 4 + 1;

/// Exact width, in bytes, of the V2 oracle preimage before hashing:
/// `domain_separator (15) || passphrase_hash (32) || market_id_be (4) || outcome_byte (1) || valid_until_be (8) || epoch_be (4)`.
pub const ORACLE_PREIMAGE_LEN_V2: usize =
    ORACLE_DOMAIN_SEPARATOR_V2.len() + 32 + 4 + 1 + 8 + 4;

/// Construct the legacy V1 message that the oracle signs.
///
/// Message format: `keccak256(domain_separator || market_id_be || outcome_byte)`
/// - `domain_separator`: fixed ASCII tag `VATIX_ORACLE_V1`
/// - `market_id`: u32 big-endian (4 bytes)
/// - `outcome_byte`: `0x01` = YES, `0x00` = NO
pub fn construct_oracle_message(env: &Env, market_id: u32, outcome: bool) -> BytesN<32> {
    let preimage = build_oracle_preimage(env, market_id, outcome);
    env.crypto().keccak256(&preimage).into()
}

/// Construct the V2 message that the oracle signs.
///
/// Message format: `keccak256(domain_separator || network_passphrase_hash || market_id_be || outcome_byte || valid_until_be || epoch_be)`
/// - `domain_separator`: fixed ASCII tag `VATIX_ORACLE_V2` (15 bytes)
/// - `network_passphrase_hash`: 32 bytes SHA-256 / Keccak-256 hash of network passphrase (or network_id)
/// - `market_id`: u32 big-endian (4 bytes)
/// - `outcome_byte`: `0x01` = YES, `0x00` = NO (1 byte)
/// - `valid_until`: u64 big-endian timestamp in seconds (8 bytes)
/// - `epoch`: u32 big-endian market resolution epoch/nonce (4 bytes)
pub fn construct_oracle_message_v2(
    env: &Env,
    passphrase_hash: &BytesN<32>,
    market_id: u32,
    outcome: bool,
    valid_until: u64,
    epoch: u32,
) -> BytesN<32> {
    let preimage = build_oracle_preimage_v2(
        env,
        passphrase_hash,
        market_id,
        outcome,
        valid_until,
        epoch,
    );
    env.crypto().keccak256(&preimage).into()
}

/// Build the raw (pre-hash) V2 oracle preimage bytes.
pub fn build_oracle_preimage_v2(
    env: &Env,
    passphrase_hash: &BytesN<32>,
    market_id: u32,
    outcome: bool,
    valid_until: u64,
    epoch: u32,
) -> Bytes {
    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from_slice(env, ORACLE_DOMAIN_SEPARATOR_V2));
    preimage.append(&Bytes::from_slice(env, passphrase_hash.to_array().as_slice()));
    preimage.append(&Bytes::from_slice(env, &market_id.to_be_bytes()));
    preimage.append(&Bytes::from_slice(env, &[u8::from(outcome)]));
    preimage.append(&Bytes::from_slice(env, &valid_until.to_be_bytes()));
    preimage.append(&Bytes::from_slice(env, &epoch.to_be_bytes()));
    preimage
}

/// Build the raw (pre-hash) oracle preimage bytes.
fn build_oracle_preimage(env: &Env, market_id: u32, outcome: bool) -> Bytes {
    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from_slice(env, ORACLE_DOMAIN_SEPARATOR));
    preimage.append(&Bytes::from_slice(env, &market_id.to_be_bytes()));
    preimage.append(&Bytes::from_slice(env, &[u8::from(outcome)]));
    preimage
}

/// Validate that a raw oracle preimage is exactly [`ORACLE_PREIMAGE_LEN`]
/// bytes wide before it is hashed and verified.
///
/// This is a defense-in-depth bound: every preimage produced internally by
/// [`build_oracle_preimage`] already satisfies this by construction, but any
/// future entrypoint that accepts a caller-supplied preimage (rather than
/// typed `market_id`/`outcome` fields) MUST route through this check first.
/// Truncated or oversized input is rejected with a typed error instead of
/// being hashed and silently mismatching, or trapping the host.
///
/// # Errors
/// - [`ContractError::InvalidSignature`] if `preimage.len()` does not equal
///   [`ORACLE_PREIMAGE_LEN`].
pub fn validate_oracle_preimage_len(preimage: &Bytes) -> Result<(), ContractError> {
    if preimage.len() as usize != ORACLE_PREIMAGE_LEN {
        return Err(ContractError::InvalidSignature);
    }
    Ok(())
}

/// Hash a caller-supplied oracle preimage, rejecting malformed
/// (truncated/oversized) input instead of panicking.
///
/// Bounds-checks `preimage` via [`validate_oracle_preimage_len`] before
/// hashing. Intended for future entrypoints that accept a raw preimage;
/// [`construct_oracle_message`] is the canonical path for the typed
/// `(market_id, outcome)` construction used by `resolve_market`.
///
/// # Errors
/// - [`ContractError::InvalidSignature`] if `preimage` is not exactly
///   [`ORACLE_PREIMAGE_LEN`] bytes.
pub fn hash_oracle_preimage_checked(
    env: &Env,
    preimage: &Bytes,
) -> Result<BytesN<32>, ContractError> {
    validate_oracle_preimage_len(preimage)?;
    Ok(env.crypto().keccak256(preimage).into())
}

/// Verify an ed25519 signature without panicking on invalid input.
///
/// `env.crypto().ed25519_verify` traps the host (an unrecoverable WASM trap,
/// not a catchable error) when the signature fails to verify, so it cannot
/// be used here — any failure must surface as a typed [`ContractError`].
/// Verification is therefore done in pure Rust via `ed25519-dalek`, which
/// reports failure as a `Result` instead of trapping.
///
/// Returns `false` if `pubkey` does not decode to a valid curve point or if
/// the signature does not verify against `message`.
fn verify_ed25519_safe(pubkey: &BytesN<32>, message: &BytesN<32>, signature: &BytesN<64>) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pubkey.to_array()) else {
        return false;
    };
    let signature = Signature::from_bytes(&signature.to_array());
    verifying_key
        .verify(&message.to_array(), &signature)
        .is_ok()
}

/// Verify that an oracle signature is valid for a market resolution.
///
/// # Fail-Closed Behavior
/// - If oracle adapters are enabled, Ed25519 verification is REJECTED.
/// - This prevents silent fallback during incomplete upgrades.
/// - See UPGRADE_PLAYBOOK.md for cross-contract upgrade order.
///
/// # Errors
/// - [`ContractError::UnauthorizedOracle`] if `oracle_pubkey` is the zero key.
/// - [`ContractError::UnauthorizedOracle`] if adapters are enabled (fail-closed).
/// - Panics if the Ed25519 signature is invalid (SDK limitation).
///
/// # Security
/// Uses Ed25519 signature verification via the Soroban crypto module.
/// CRITICAL: Do NOT silently accept Ed25519 when adapters exist.

/// - [`ContractError::InvalidSignature`] if the signature does not verify
///   against `construct_oracle_message(env, market_id, outcome)`.
///
/// # Security
/// Uses Ed25519 signature verification, performed in pure Rust (see
/// [`verify_ed25519_safe`]) rather than the host's `ed25519_verify`, so that
/// an invalid signature returns a typed error instead of trapping the host.
pub fn verify_oracle_signature(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signature: &BytesN<64>,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    // Fail-closed: reject Ed25519 if adapters are enabled
    if crate::storage::has_oracle_adapters(env) {
        return Err(ContractError::UnauthorizedOracle);
    }

    if oracle_pubkey == &BytesN::from_array(env, &[0u8; 32]) {
        return Err(ContractError::UnauthorizedOracle);
    }

    let message = construct_oracle_message(env, market_id, outcome);
    if !verify_ed25519_safe(oracle_pubkey, &message, signature) {
        return Err(ContractError::InvalidSignature);
    }

    Ok(())
}

/// Verify that a V2 oracle signature is valid for a market resolution.
///
/// Checks:
/// 1. `oracle_pubkey` is non-zero.
/// 2. `env.ledger().timestamp() <= valid_until` (signature has not expired).
/// 3. Ed25519 signature verifies against `construct_oracle_message_v2(...)`.
///
/// # Errors
/// - [`ContractError::UnauthorizedOracle`] if `oracle_pubkey` is the zero key.
/// - [`ContractError::InvalidSignature`] if signature expired or ed25519 verification fails.
pub fn verify_oracle_signature_v2(
    env: &Env,
    passphrase_hash: &BytesN<32>,
    market_id: u32,
    outcome: bool,
    valid_until: u64,
    epoch: u32,
    signature: &BytesN<64>,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    if oracle_pubkey == &BytesN::from_array(env, &[0u8; 32]) {
        return Err(ContractError::UnauthorizedOracle);
    }

    if env.ledger().timestamp() > valid_until {
        return Err(ContractError::InvalidSignature);
    }

    let message = construct_oracle_message_v2(
        env,
        passphrase_hash,
        market_id,
        outcome,
        valid_until,
        epoch,
    );

    if !verify_ed25519_safe(oracle_pubkey, &message, signature) {
        return Err(ContractError::InvalidSignature);
    }

    Ok(())
}

/// Check whether `oracle_pubkey` is authorised to resolve `market`.
///
/// MVP: pubkey must match `market.oracle_pubkey` exactly.
/// Post-MVP: could check against a registry of approved oracles.
///
/// # Errors
/// - [`ContractError::UnauthorizedOracle`] if the pubkey doesn't match.
#[allow(dead_code)]
pub fn validate_oracle_authorization(
    market: &Market,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    if market.oracle_pubkey == *oracle_pubkey {
        Ok(())
    } else {
        Err(ContractError::UnauthorizedOracle)
    }
}

/// Verify that the market outcome is valid according to the configured oracle adapter.
///
/// For `AdapterType::Ed25519`, this verifies the provided signature against the
/// market's `oracle_pubkey`.
///
/// For `AdapterType::Reflector` / `AdapterType::Pyth` the full on-chain adapter
/// integration (price fetch + comparison) is tracked separately under #139 and
/// is not wired into this dispatch. Rather than leaving markets configured with
/// one of these adapters permanently unresolvable, we check the per-adapter
/// `enabled` flag (#488):
/// - If the adapter has been explicitly marked enabled by the admin, we still
///   fail closed with `UnauthorizedOracle` — this dispatch has no code path to
///   actually query Reflector/Pyth yet, so silently succeeding would be unsafe.
/// - If the adapter is disabled (the default), we fall back to verifying
///   `proof` as a direct Ed25519 signature over the same canonical payload
///   (`construct_oracle_message`) against the market's `oracle_pubkey`, using
///   the identical [`verify_oracle_signature`] path as `AdapterType::Ed25519`.
///   This lets a market keep resolving via the trusted single-signer key while
///   its richer oracle adapter is unavailable, instead of getting stuck.
pub fn verify_market_outcome(
    env: &Env,
    market_id: u32,
    market: &Market,
    adapter_type: AdapterType,
    outcome: bool,
    proof: &BytesN<64>,
) -> Result<(), ContractError> {
    match adapter_type {
        AdapterType::Ed25519 => verify_oracle_signature(env, market_id, outcome, proof, &market.oracle_pubkey),
        AdapterType::Reflector => verify_via_reflector(env, market_id, market, outcome, proof),
        AdapterType::Pyth => {
            if crate::storage::is_adapter_enabled(env, &adapter_type) {
                // Pyth needs a raw Wormhole VAA proof (`Bytes`), not the
                // fixed 64-byte Ed25519 signature this entrypoint accepts —
                // full wiring is a follow-up (#680 wires Reflector first).
                Err(ContractError::UnauthorizedOracle)
            } else {
                // Adapter disabled/unavailable — fall back to raw Ed25519 verification.
                verify_oracle_signature(env, market_id, outcome, proof, &market.oracle_pubkey)
            }
        }
    }
}

/// Dispatch to the on-chain Reflector adapter when it is enabled and
/// configured for `market_id`; otherwise fall back to Ed25519 (#680).
///
/// Requires a `MarketAdapterConfig` to have been set via
/// `MarketContract::set_market_adapter_config` (#681, admin-only) — if the
/// adapter is enabled but this market has no config, that is a
/// misconfiguration and fails closed with `OraclePriceUnavailable` rather
/// than silently falling back to Ed25519.
///
/// # Fail-closed contract (#778)
///
/// When the `oracle-adapter` Cargo feature is **not** compiled in:
/// - If the Reflector adapter is **disabled** (the default), we fall back to
///   Ed25519, exactly as before.
/// - If the Reflector adapter is **enabled** but the feature gate is off,
///   the contract fails closed with `UnauthorizedOracle` rather than
///   silently letting an Ed25519 signature stand in for a missing adapter.
///   This mirrors the existing Pyth arm and prevents a misconfigured release
///   build (adapter enabled, feature omitted) from quietly resolving a market
///   via the weaker path.
///
/// The `oracle-adapter` feature must **not** appear in `[features] default`
/// (see `Cargo.toml`).  CI enforces this with a dedicated
/// `oracle-adapter-not-default` job and a compile-time `#[test]` in this
/// module.
fn verify_via_reflector(
    env: &Env,
    market_id: u32,
    market: &Market,
    outcome: bool,
    proof: &BytesN<64>,
) -> Result<(), ContractError> {
    if !crate::storage::is_adapter_enabled(env, &AdapterType::Reflector) {
        // Adapter disabled: fall back to direct Ed25519 verification.
        return verify_oracle_signature(env, market_id, outcome, proof, &market.oracle_pubkey);
    }

    // Adapter is enabled — from here we MUST use the real on-chain adapter.
    // If the `oracle-adapter` feature was not compiled in we fail closed
    // instead of silently accepting an Ed25519 signature (#778).
    #[cfg(feature = "oracle-adapter")]
    {
        let config = crate::storage::get_market_adapter_config(env, market_id)
            .ok_or(ContractError::OraclePriceUnavailable)?;
        use crate::oracle_adapter::OracleAdapter as _;
        let adapter = crate::oracle_adapter::ReflectorAdapter {
            contract_id: config.oracle_contract,
            asset: config.asset,
            resolution_price: config.resolution_price,
        };
        return adapter.verify_outcome(env, market_id, outcome, &Bytes::new(env));
    }

    // `oracle-adapter` feature not compiled in — fail closed (#778).
    // The Reflector adapter was explicitly enabled by the admin but the
    // on-chain integration is not present in this build.  Returning
    // UnauthorizedOracle here is deliberate: it is a clear, typed error
    // rather than a silent fallback that would let a plain Ed25519 signature
    // resolve a market whose adapter is supposed to be Reflector.
    #[cfg(not(feature = "oracle-adapter"))]
    Err(ContractError::UnauthorizedOracle)
}

/// Verify that the market outcome is valid using V2 oracle signatures.
pub fn verify_market_outcome_v2(
    env: &Env,
    passphrase_hash: &BytesN<32>,
    market_id: u32,
    market: &Market,
    adapter_type: AdapterType,
    outcome: bool,
    valid_until: u64,
    epoch: u32,
    proof: &BytesN<64>,
) -> Result<(), ContractError> {
    match adapter_type {
        AdapterType::Ed25519 => verify_oracle_signature_v2(
            env,
            passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
            proof,
            &market.oracle_pubkey,
        ),
        AdapterType::Reflector => {
            if crate::storage::is_adapter_enabled(env, &adapter_type) {
                // Adapter enabled — must use the real on-chain Reflector.
                // Fail closed if the feature gate is missing (#778).
                #[cfg(feature = "oracle-adapter")]
                {
                    let config = crate::storage::get_market_adapter_config(env, market_id)
                        .ok_or(ContractError::OraclePriceUnavailable)?;
                    use crate::oracle_adapter::OracleAdapter as _;
                    let adapter = crate::oracle_adapter::ReflectorAdapter {
                        contract_id: config.oracle_contract,
                        asset: config.asset,
                        resolution_price: config.resolution_price,
                    };
                    return adapter.verify_outcome(env, market_id, outcome, &Bytes::new(env));
                }
                // `oracle-adapter` feature not compiled in — fail closed (#778).
                #[cfg(not(feature = "oracle-adapter"))]
                return Err(ContractError::UnauthorizedOracle);
            }
            // Adapter disabled/unavailable — fall back to raw Ed25519 V2 verification.
            verify_oracle_signature_v2(
                env,
                passphrase_hash,
                market_id,
                outcome,
                valid_until,
                epoch,
                proof,
                &market.oracle_pubkey,
            )
        }
        AdapterType::Pyth => {
            if crate::storage::is_adapter_enabled(env, &adapter_type) {
                // See `verify_market_outcome` — Pyth needs a raw VAA proof,
                // not fitted by this V2 entrypoint's fixed signature param.
                Err(ContractError::UnauthorizedOracle)
            } else {
                // Adapter disabled/unavailable — fall back to raw Ed25519 V2 verification.
                verify_oracle_signature_v2(
                    env,
                    passphrase_hash,
                    market_id,
                    outcome,
                    valid_until,
                    epoch,
                    proof,
                    &market.oracle_pubkey,
                )
            }
        }
    }
}

/// Verify a quorum of Ed25519 signatures for multi-signer threshold resolution (#378).
///
/// `signatures` is a parallel slice aligned with `signers`: `signatures[i]` is
/// the Ed25519 signature produced by `signers[i]` over
/// `keccak256(market_id_be || outcome_byte)`.  Entries where the caller does
/// not have a valid signature should be zeroed-out (64 zero bytes) — they are
/// counted as failures but do not abort the loop.
///
/// The function counts how many signatures verify and returns `Ok(())` only
/// when that count meets or exceeds `quorum`.
///
/// # Errors
/// - `UnauthorizedOracle` — `signers` is empty or `quorum` is 0.
/// - `InvalidSignature`   — fewer than `quorum` signatures verified.
pub fn verify_threshold_signatures(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signers: &soroban_sdk::Vec<BytesN<32>>,
    signatures: &soroban_sdk::Vec<BytesN<64>>,
    quorum: u32,
) -> Result<(), ContractError> {
    let signers_len = signers.len();
    if signers.is_empty() || quorum == 0 || quorum > signers_len {
        return Err(ContractError::UnauthorizedOracle);
    }

    // Invariant check: reject duplicate signer entries (equivocation / double counting safeguard)
    for i in 0..signers_len {
        for j in (i + 1)..signers_len {
            if signers.get(i).unwrap() == signers.get(j).unwrap() {
                return Err(ContractError::InvalidSignature);
            }
        }
    }

    let message_target = construct_oracle_message(env, market_id, outcome);
    let message_opposite = construct_oracle_message(env, market_id, !outcome);
    let mut valid: u32 = 0;

    let len = signers_len.min(signatures.len() as u32) as usize;
    for i in 0..len {
        let pubkey = signers.get(i as u32).unwrap();
        let sig = signatures.get(i as u32).unwrap();

        // Equivocation check: reject if signer signed the opposite outcome
        if verify_ed25519_safe(&pubkey, &message_opposite, &sig) {
            return Err(ContractError::InvalidSignature);
        }

        if verify_ed25519_safe(&pubkey, &message_target, &sig) {
            valid += 1;
            if valid >= quorum {
                return Ok(());
            }
        }
    }

    Err(ContractError::InvalidSignature)
}

/// Verify a quorum of V2 Ed25519 signatures for multi-signer threshold resolution.
pub fn verify_threshold_signatures_v2(
    env: &Env,
    passphrase_hash: &BytesN<32>,
    market_id: u32,
    outcome: bool,
    valid_until: u64,
    epoch: u32,
    signers: &soroban_sdk::Vec<BytesN<32>>,
    signatures: &soroban_sdk::Vec<BytesN<64>>,
    quorum: u32,
) -> Result<(), ContractError> {
    let signers_len = signers.len();
    if signers.is_empty() || quorum == 0 || quorum > signers_len {
        return Err(ContractError::UnauthorizedOracle);
    }

    if env.ledger().timestamp() > valid_until {
        return Err(ContractError::InvalidSignature);
    }

    // Invariant check: reject duplicate signer entries
    for i in 0..signers_len {
        for j in (i + 1)..signers_len {
            if signers.get(i).unwrap() == signers.get(j).unwrap() {
                return Err(ContractError::InvalidSignature);
            }
        }
    }

    let message_target = construct_oracle_message_v2(
        env,
        passphrase_hash,
        market_id,
        outcome,
        valid_until,
        epoch,
    );
    let message_opposite = construct_oracle_message_v2(
        env,
        passphrase_hash,
        market_id,
        !outcome,
        valid_until,
        epoch,
    );
    let mut valid: u32 = 0;

    let len = signers_len.min(signatures.len() as u32) as usize;
    for i in 0..len {
        let pubkey = signers.get(i as u32).unwrap();
        let sig = signatures.get(i as u32).unwrap();

        // Equivocation check: reject if signer signed opposite outcome
        if verify_ed25519_safe(&pubkey, &message_opposite, &sig) {
            return Err(ContractError::InvalidSignature);
        }

        if verify_ed25519_safe(&pubkey, &message_target, &sig) {
            valid += 1;
            if valid >= quorum {
                return Ok(());
            }
        }
    }

    Err(ContractError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::format;
    use super::*;
    use crate::types::MarketStatus;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _},
        Address, Env, String,
    };

    fn make_market(env: &Env, oracle_pubkey: BytesN<32>) -> Market {
        Market {
            id: 1,
            question: String::from_str(env, "Test market"),
            end_time: 1000,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: Address::generate(env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
            closed_to_deposits: false,
        }
    }

    #[test]
    fn test_construct_oracle_message_yes() {
        let env = Env::default();
        let message = construct_oracle_message(&env, 1u32, true);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_no() {
        let env = Env::default();
        let message = construct_oracle_message(&env, 1u32, false);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_different_outcomes_different_messages() {
        let env = Env::default();
        let msg_yes = construct_oracle_message(&env, 1u32, true);
        let msg_no = construct_oracle_message(&env, 1u32, false);
        assert_ne!(msg_yes, msg_no);
    }

    #[test]
    fn test_construct_oracle_message_deterministic() {
        let env = Env::default();
        let msg1 = construct_oracle_message(&env, 456u32, true);
        let msg2 = construct_oracle_message(&env, 456u32, true);
        assert_eq!(msg1, msg2);
    }

    #[test]
    fn test_different_market_ids_different_messages() {
        let env = Env::default();
        let msg1 = construct_oracle_message(&env, 1u32, true);
        let msg2 = construct_oracle_message(&env, 2u32, true);
        assert_ne!(msg1, msg2);
    }

    #[test]
    fn test_construct_oracle_message_zero_id() {
        let env = Env::default();
        let message = construct_oracle_message(&env, 0u32, true);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_large_id() {
        let env = Env::default();
        let message = construct_oracle_message(&env, u32::MAX, false);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_various_ids() {
        let env = Env::default();
        let msg1 = construct_oracle_message(&env, 100u32, true);
        let msg2 = construct_oracle_message(&env, 1000u32, true);
        let msg3 = construct_oracle_message(&env, 10000u32, true);
        assert_ne!(msg1, msg2);
        assert_ne!(msg2, msg3);
        assert_ne!(msg1, msg3);
        assert_eq!(msg1.len(), 32);
    }

    #[test]
    fn test_validate_oracle_authorized() {
        let env = Env::default();
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let market = make_market(&env, oracle_pubkey.clone());
        assert!(validate_oracle_authorization(&market, &oracle_pubkey).is_ok());
    }

    #[test]
    fn test_validate_oracle_authorization_zero_pubkey() {
        let env = Env::default();
        let zero_pubkey = BytesN::from_array(&env, &[0u8; 32]);
        let market = make_market(&env, BytesN::from_array(&env, &[1u8; 32]));
        let result = validate_oracle_authorization(&market, &zero_pubkey);
        assert_eq!(result, Err(ContractError::UnauthorizedOracle));
    }

    #[test]
    fn test_validate_oracle_unauthorized() {
        let env = Env::default();
        let market = make_market(&env, BytesN::from_array(&env, &[1u8; 32]));
        let wrong_pubkey = BytesN::from_array(&env, &[2u8; 32]);
        let result = validate_oracle_authorization(&market, &wrong_pubkey);
        assert_eq!(result, Err(ContractError::UnauthorizedOracle));
    }

    #[test]
    fn test_verify_signature_rejects_zero_pubkey() {
        let env = Env::default();
        let result = verify_oracle_signature(
            &env,
            1u32,
            true,
            &BytesN::from_array(&env, &[0u8; 64]),
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        assert_eq!(result, Err(ContractError::UnauthorizedOracle));
    }

    #[test]
    fn test_verify_invalid_signature() {
        let env = Env::default();
        let result = verify_oracle_signature(
            &env,
            123u32,
            true,
            &BytesN::random(&env),
            &BytesN::random(&env),
        );
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    /// Generate an ed25519 keypair and sign `construct_oracle_message(market_id, outcome)`.
    fn generate_keypair_and_sign(
        env: &Env,
        market_id: u32,
        outcome: bool,
    ) -> (BytesN<32>, BytesN<64>) {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let message = construct_oracle_message(env, market_id, outcome);
        let signature = signing_key.sign(message.to_array().as_slice());

        (
            BytesN::from_array(env, &signing_key.verifying_key().to_bytes()),
            BytesN::from_array(env, &signature.to_bytes()),
        )
    }

    #[test]
    fn test_verify_valid_signature_succeeds() {
        let env = Env::default();
        let market_id = 1u32;
        let outcome = true;
        let (pubkey, signature) = generate_keypair_and_sign(&env, market_id, outcome);

        let result = verify_oracle_signature(&env, market_id, outcome, &signature, &pubkey);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_verify_signature_wrong_outcome_fails() {
        let env = Env::default();
        let market_id = 1u32;
        let (pubkey, signature) = generate_keypair_and_sign(&env, market_id, true);

        // Signature was produced for outcome=true; verifying against false must fail.
        let result = verify_oracle_signature(&env, market_id, false, &signature, &pubkey);
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn test_verify_signature_wrong_market_id_fails() {
        let env = Env::default();
        let outcome = true;
        let (pubkey, signature) = generate_keypair_and_sign(&env, 1u32, outcome);

        // Signature was produced for market_id=1; verifying against 2 must fail.
        let result = verify_oracle_signature(&env, 2u32, outcome, &signature, &pubkey);
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn test_verify_signature_from_different_keypair_fails() {
        let env = Env::default();
        let market_id = 1u32;
        let outcome = true;
        let (_pubkey, signature) = generate_keypair_and_sign(&env, market_id, outcome);
        let (other_pubkey, _other_signature) = generate_keypair_and_sign(&env, market_id, outcome);

        // Signature was produced by a different keypair than `other_pubkey`.
        let result = verify_oracle_signature(&env, market_id, outcome, &signature, &other_pubkey);
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    /// Generate an ed25519 keypair and sign V2 message `construct_oracle_message_v2`.
    fn generate_keypair_and_sign_v2(
        env: &Env,
        passphrase_hash: &BytesN<32>,
        market_id: u32,
        outcome: bool,
        valid_until: u64,
        epoch: u32,
    ) -> (BytesN<32>, BytesN<64>) {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let message = construct_oracle_message_v2(
            env,
            passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );
        let signature = signing_key.sign(message.to_array().as_slice());

        (
            BytesN::from_array(env, &signing_key.verifying_key().to_bytes()),
            BytesN::from_array(env, &signature.to_bytes()),
        )
    }

    #[test]
    fn test_v2_signature_verification_success() {
        let env = Env::default();
        let passphrase_hash = BytesN::from_array(&env, &[100u8; 32]);
        let market_id = 10u32;
        let outcome = true;
        let valid_until = 1000u64;
        let epoch = 1u32;

        let (pubkey, signature) = generate_keypair_and_sign_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );

        let result = verify_oracle_signature_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
            &signature,
            &pubkey,
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_v2_signature_cross_network_rejection() {
        let env = Env::default();
        let mainnet_passphrase_hash = BytesN::from_array(&env, &[1u8; 32]);
        let testnet_passphrase_hash = BytesN::from_array(&env, &[2u8; 32]);
        let market_id = 10u32;
        let outcome = true;
        let valid_until = 1000u64;
        let epoch = 1u32;

        // Signature generated on testnet
        let (pubkey, signature) = generate_keypair_and_sign_v2(
            &env,
            &testnet_passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );

        // Verify on mainnet must fail
        let result = verify_oracle_signature_v2(
            &env,
            &mainnet_passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
            &signature,
            &pubkey,
        );
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn test_v2_signature_expired_valid_until_rejection() {
        let env = Env::default();
        env.ledger().set_timestamp(500);

        let passphrase_hash = BytesN::from_array(&env, &[1u8; 32]);
        let market_id = 10u32;
        let outcome = true;
        let valid_until = 400u64; // expired relative to ledger timestamp 500
        let epoch = 1u32;

        let (pubkey, signature) = generate_keypair_and_sign_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );

        let result = verify_oracle_signature_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
            &signature,
            &pubkey,
        );
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn test_v2_signature_epoch_mismatch_rejection() {
        let env = Env::default();
        let passphrase_hash = BytesN::from_array(&env, &[1u8; 32]);
        let market_id = 10u32;
        let outcome = true;
        let valid_until = 1000u64;
        let epoch = 1u32;

        // Signature produced for epoch 1
        let (pubkey, signature) = generate_keypair_and_sign_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );

        // Verification with epoch 2 must fail
        let result = verify_oracle_signature_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            2u32,
            &signature,
            &pubkey,
        );
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    /// Export a deterministic test vector so the backend signer can validate
    /// its keccak256 + Ed25519 implementation against the on-chain format.
    ///
    /// Writes `test-vectors/oracle-message.json` at the workspace root.
    /// Run with `cargo test export_oracle_test_vector -- --nocapture`.
    #[test]
    #[cfg(feature = "std")]
    fn export_oracle_test_vector() {
        use ed25519_dalek::{Signer, SigningKey};

        // Fixed seed — deterministic across runs so the vector is stable.
        let seed = [0x42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        let env = Env::default();
        let passphrase_hash = BytesN::from_array(&env, &[0x99u8; 32]);
        let market_id: u32 = 1;
        let outcome = true; // YES
        let valid_until: u64 = 2000000000;
        let epoch: u32 = 1;

        let message = construct_oracle_message_v2(
            &env,
            &passphrase_hash,
            market_id,
            outcome,
            valid_until,
            epoch,
        );
        let message_bytes = message.to_array();
        let signature = signing_key.sign(&message_bytes);

        // Verify the vector is self-consistent before writing.
        assert!(
            verify_oracle_signature_v2(
                &env,
                &passphrase_hash,
                market_id,
                outcome,
                valid_until,
                epoch,
                &BytesN::from_array(&env, &signature.to_bytes()),
                &BytesN::from_array(&env, &verifying_key.to_bytes()),
            )
            .is_ok(),
            "V2 test vector signature must verify on-chain"
        );

        let to_hex = |b: &[u8]| -> std::string::String { b.iter().map(|x| format!("{:02x}", x)).collect() };

        let mut raw = [0u8; ORACLE_PREIMAGE_LEN_V2];
        let mut idx = 0;
        raw[idx..idx + ORACLE_DOMAIN_SEPARATOR_V2.len()].copy_from_slice(ORACLE_DOMAIN_SEPARATOR_V2);
        idx += ORACLE_DOMAIN_SEPARATOR_V2.len();
        raw[idx..idx + 32].copy_from_slice(&passphrase_hash.to_array());
        idx += 32;
        raw[idx..idx + 4].copy_from_slice(&market_id.to_be_bytes());
        idx += 4;
        raw[idx] = u8::from(outcome);
        idx += 1;
        raw[idx..idx + 8].copy_from_slice(&valid_until.to_be_bytes());
        idx += 8;
        raw[idx..idx + 4].copy_from_slice(&epoch.to_be_bytes());

        let json = format!(
            concat!(
                "{{\n",
                "  \"description\": \"Canonical oracle message V2: keccak256(domain_separator || network_passphrase_hash || market_id_be || outcome_byte || valid_until_be || epoch_be)\",\n",
                "  \"domain_separator\": \"VATIX_ORACLE_V2\",\n",
                "  \"network_passphrase_hash_hex\": \"{passphrase_hex}\",\n",
                "  \"market_id\": {market_id},\n",
                "  \"outcome\": \"YES\",\n",
                "  \"valid_until\": {valid_until},\n",
                "  \"epoch\": {epoch},\n",
                "  \"raw_hex\": \"{raw_hex}\",\n",
                "  \"keccak_hex\": \"{keccak_hex}\",\n",
                "  \"pubkey_hex\": \"{pubkey_hex}\",\n",
                "  \"signature_hex\": \"{sig_hex}\"\n",
                "}}\n"
            ),
            passphrase_hex = to_hex(&passphrase_hash.to_array()),
            market_id = market_id,
            valid_until = valid_until,
            epoch = epoch,
            raw_hex = to_hex(&raw),
            keccak_hex = to_hex(&message_bytes),
            pubkey_hex = to_hex(&verifying_key.to_bytes()),
            sig_hex = to_hex(&signature.to_bytes()),
        );

        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-vectors");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-vectors/oracle-message.json");
        std::fs::create_dir_all(dir).expect("create test-vectors dir");
        std::fs::write(path, &json).expect("write oracle-message.json");
    }

    // --- Domain separator + bounded preimage tests ---

    #[test]
    fn test_preimage_has_domain_separator_and_exact_length() {
        let env = Env::default();
        let preimage = build_oracle_preimage(&env, 1u32, true);
        assert_eq!(preimage.len() as usize, ORACLE_PREIMAGE_LEN);

        let mut expected = std::vec::Vec::new();
        expected.extend_from_slice(ORACLE_DOMAIN_SEPARATOR);
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.push(1u8);
        assert_eq!(preimage.to_alloc_vec(), expected);
    }

    #[test]
    fn test_validate_oracle_preimage_len_accepts_exact_width() {
        let env = Env::default();
        let preimage = build_oracle_preimage(&env, 42u32, false);
        assert!(validate_oracle_preimage_len(&preimage).is_ok());
    }

    #[test]
    fn test_validate_oracle_preimage_len_rejects_truncated() {
        let env = Env::default();
        // One byte short of the expected width.
        let short = Bytes::from_slice(&env, &[0u8; ORACLE_PREIMAGE_LEN - 1]);
        assert_eq!(
            validate_oracle_preimage_len(&short),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn test_validate_oracle_preimage_len_rejects_oversized() {
        let env = Env::default();
        // One byte over the expected width.
        let long = Bytes::from_slice(&env, &[0u8; ORACLE_PREIMAGE_LEN + 1]);
        assert_eq!(
            validate_oracle_preimage_len(&long),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn test_validate_oracle_preimage_len_rejects_empty() {
        let env = Env::default();
        let empty = Bytes::new(&env);
        assert_eq!(
            validate_oracle_preimage_len(&empty),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn test_hash_oracle_preimage_checked_never_panics_on_malformed_input() {
        let env = Env::default();
        // A wildly oversized "message" must be rejected, not hashed or panicked on.
        let oversized = Bytes::from_slice(&env, &[0xAAu8; 4096]);
        assert_eq!(
            hash_oracle_preimage_checked(&env, &oversized),
            Err(ContractError::InvalidSignature)
        );

        let truncated = Bytes::from_slice(&env, &[0xBBu8; 1]);
        assert_eq!(
            hash_oracle_preimage_checked(&env, &truncated),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn test_hash_oracle_preimage_checked_matches_construct_oracle_message() {
        let env = Env::default();
        let market_id = 7u32;
        let outcome = true;
        let preimage = build_oracle_preimage(&env, market_id, outcome);
        let hashed = hash_oracle_preimage_checked(&env, &preimage).unwrap();
        assert_eq!(hashed, construct_oracle_message(&env, market_id, outcome));
    }

    /// A signature computed over the legacy (pre-domain-separator) preimage
    /// `market_id_be || outcome_byte` must NOT verify against the current
    /// [`construct_oracle_message`] output — proving the domain separator
    /// actually changes the signed message rather than being cosmetic.
    #[test]
    fn test_legacy_preimage_without_domain_separator_does_not_verify() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let env = Env::default();
        let market_id = 9u32;
        let outcome = true;

        let mut legacy_raw = std::vec::Vec::new();
        legacy_raw.extend_from_slice(&market_id.to_be_bytes());
        legacy_raw.push(u8::from(outcome));
        let legacy_message: [u8; 32] = env
            .crypto()
            .keccak256(&Bytes::from_slice(&env, &legacy_raw))
            .into();

        let signing_key = SigningKey::generate(&mut OsRng);
        let signature = signing_key.sign(&legacy_message);
        let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let sig_bytes = BytesN::from_array(&env, &signature.to_bytes());

        let result = verify_oracle_signature(&env, market_id, outcome, &sig_bytes, &pubkey);
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }
}

#[cfg(test)]
mod threshold_tests {
    use super::*;
    use soroban_sdk::{testutils::BytesN as _, Env, Vec};

    /// Generate a keypair and sign the oracle message for (market_id, outcome).
    fn sign(env: &Env, market_id: u32, outcome: bool) -> (BytesN<32>, BytesN<64>) {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut OsRng);
        let message = construct_oracle_message(env, market_id, outcome);
        let sig = signing_key.sign(message.to_array().as_slice());
        (
            BytesN::from_array(env, &signing_key.verifying_key().to_bytes()),
            BytesN::from_array(env, &sig.to_bytes()),
        )
    }

    #[test]
    fn threshold_2_of_3_meets_quorum() {
        let env = Env::default();
        let (pk1, sig1) = sign(&env, 1, true);
        let (pk2, sig2) = sign(&env, 1, true);
        let (pk3, _) = sign(&env, 1, true);
        // Third signer does NOT provide a valid sig (zero bytes).
        let bad_sig = BytesN::from_array(&env, &[0u8; 64]);

        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1);
        signers.push_back(pk2);
        signers.push_back(pk3);

        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1);
        sigs.push_back(sig2);
        sigs.push_back(bad_sig);

        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 2),
            Ok(())
        );
    }

    #[test]
    fn threshold_only_1_of_3_valid_below_quorum() {
        let env = Env::default();
        let (pk1, sig1) = sign(&env, 1, true);
        let (pk2, _) = sign(&env, 1, true);
        let (pk3, _) = sign(&env, 1, true);
        let bad_sig = BytesN::from_array(&env, &[0u8; 64]);

        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1);
        signers.push_back(pk2);
        signers.push_back(pk3);

        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1);
        sigs.push_back(bad_sig.clone());
        sigs.push_back(bad_sig);

        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 2),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn threshold_empty_signers_returns_unauthorized() {
        let env = Env::default();
        let signers: Vec<BytesN<32>> = Vec::new(&env);
        let sigs: Vec<BytesN<64>> = Vec::new(&env);
        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 2),
            Err(ContractError::UnauthorizedOracle)
        );
    }

    #[test]
    fn threshold_quorum_zero_returns_unauthorized() {
        let env = Env::default();
        let (pk1, sig1) = sign(&env, 1, true);
        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1);
        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1);
        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 0),
            Err(ContractError::UnauthorizedOracle)
        );
    }

    #[test]
    fn threshold_wrong_outcome_sigs_do_not_count() {
        let env = Env::default();
        // Sigs produced for outcome=false but we verify outcome=true
        let (pk1, sig1_wrong) = sign(&env, 1, false);
        let (pk2, sig2_wrong) = sign(&env, 1, false);
        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1);
        signers.push_back(pk2);
        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1_wrong);
        sigs.push_back(sig2_wrong);
        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 1),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn threshold_1_of_1_succeeds() {
        let env = Env::default();
        let (pk, sig) = sign(&env, 42, false);
        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk);
        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig);
        assert_eq!(
            verify_threshold_signatures(&env, 42, false, &signers, &sigs, 1),
            Ok(())
        );
    }

    #[test]
    fn threshold_rejects_duplicate_signers() {
        let env = Env::default();
        let (pk1, sig1) = sign(&env, 1, true);

        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1.clone());
        signers.push_back(pk1); // Duplicate signer

        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1.clone());
        sigs.push_back(sig1);

        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 2),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn threshold_rejects_quorum_exceeding_signers_count() {
        let env = Env::default();
        let (pk1, sig1) = sign(&env, 1, true);
        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pk1);
        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig1);

        // Quorum 2 > signers.len() 1
        assert_eq!(
            verify_threshold_signatures(&env, 1, true, &signers, &sigs, 2),
            Err(ContractError::UnauthorizedOracle)
        );
    }

    #[test]
    fn threshold_rejects_equivocating_signer_signing_opposite_outcome() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let env = Env::default();
        let signing_key = SigningKey::generate(&mut OsRng);
        let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        // Signer produces a signature over the OPPOSITE outcome (NO)
        let opposite_msg = construct_oracle_message(&env, 10, false);
        let sig_opposite = signing_key.sign(opposite_msg.to_array().as_slice());
        let sig_bytes = BytesN::from_array(&env, &sig_opposite.to_bytes());

        let mut signers: Vec<BytesN<32>> = Vec::new(&env);
        signers.push_back(pubkey);

        let mut sigs: Vec<BytesN<64>> = Vec::new(&env);
        sigs.push_back(sig_bytes);

        // Submitting opposite signature when verifying outcome YES must be rejected
        assert_eq!(
            verify_threshold_signatures(&env, 10, true, &signers, &sigs, 1),
            Err(ContractError::InvalidSignature)
        );
    }
}

#[cfg(test)]
mod adapter_fallback_tests {
    //! Tests for the Reflector/Pyth → Ed25519 fallback dispatch (#488).
    use super::*;
    use crate::types::{AdapterType, Market, MarketStatus};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    fn make_reflector_market(env: &Env, oracle_pubkey: BytesN<32>) -> Market {
        Market {
            id: 1,
            question: String::from_str(env, "Reflector fallback test"),
            end_time: 1000,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(env),
            created_at: 0,
            collateral_token: Address::generate(env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Reflector,
            outcome_count: 2,
            closed_to_deposits: false,
        }
    }

    #[test]
    fn reflector_disabled_by_default_falls_back_to_ed25519() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market_id = 1u32;
        let outcome = true;

        let signing_key = SigningKey::generate(&mut OsRng);
        let message = construct_oracle_message(&env, market_id, outcome);
        let signature = signing_key.sign(message.to_array().as_slice());
        let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let sig = BytesN::from_array(&env, &signature.to_bytes());
        let market = make_reflector_market(&env, pubkey);

        env.as_contract(&contract_id, || {
            // No admin has enabled the Reflector adapter, so it defaults to
            // disabled and resolution falls back to direct Ed25519 verification.
            let result =
                verify_market_outcome(&env, market_id, &market, AdapterType::Reflector, outcome, &sig);
            assert_eq!(result, Ok(()));
        });
    }

    #[test]
    fn reflector_fallback_rejects_invalid_signature() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = make_reflector_market(&env, BytesN::from_array(&env, &[1u8; 32]));

        env.as_contract(&contract_id, || {
            let result = verify_market_outcome(
                &env,
                1,
                &market,
                AdapterType::Reflector,
                true,
                &BytesN::from_array(&env, &[0u8; 64]),
            );
            assert_eq!(result, Err(ContractError::InvalidSignature));
        });
    }

    #[test]
    fn reflector_enabled_still_rejects_since_adapter_not_wired() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market = make_reflector_market(&env, BytesN::from_array(&env, &[1u8; 32]));

        env.as_contract(&contract_id, || {
            crate::storage::set_adapter_enabled(&env, &AdapterType::Reflector, true);
            let result = verify_market_outcome(
                &env,
                1,
                &market,
                AdapterType::Reflector,
                true,
                &BytesN::from_array(&env, &[0u8; 64]),
            );
            assert_eq!(result, Err(ContractError::UnauthorizedOracle));
        });
    }

    // --- #555: Pyth adapter path coverage ---

    /// When a Pyth adapter is disabled (the default — no storage entry), the
    /// fallback path is identical to the Reflector case: verify_market_outcome
    /// falls through to Ed25519 verification against `oracle_pubkey`.
    #[test]
    fn pyth_disabled_falls_back_to_ed25519() {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market_id = 1u32;
        let outcome = false;

        let signing_key = SigningKey::generate(&mut OsRng);
        let message = construct_oracle_message(&env, market_id, outcome);
        let signature = signing_key.sign(message.to_array().as_slice());
        let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let sig = BytesN::from_array(&env, &signature.to_bytes());

        // Build a Pyth-typed market using the same oracle_pubkey.
        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Pyth disabled fallback test"),
            end_time: 9_999_999,
            oracle_pubkey: pubkey,
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token: Address::generate(&env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Pyth,
            outcome_count: 2,
            closed_to_deposits: false,
        };

        env.as_contract(&contract_id, || {
            // Pyth adapter is not enabled — fall back to direct Ed25519 verification.
            let result =
                verify_market_outcome(&env, market_id, &market, AdapterType::Pyth, outcome, &sig);
            assert_eq!(result, Ok(()));
        });
    }

    /// When a Pyth adapter is explicitly enabled, verify_market_outcome returns
    /// `UnauthorizedOracle` because the full on-chain Pyth adapter is not yet
    /// wired (tracked in #139). This is the "clear error for misconfig" path.
    #[test]
    fn pyth_enabled_returns_unauthorized_oracle() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        let market = Market {
            id: 2,
            question: String::from_str(&env, "Pyth enabled misconfig test"),
            end_time: 9_999_999,
            oracle_pubkey: BytesN::from_array(&env, &[1u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token: Address::generate(&env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Pyth,
            outcome_count: 2,
            closed_to_deposits: false,
        };

        env.as_contract(&contract_id, || {
            crate::storage::set_adapter_enabled(&env, &AdapterType::Pyth, true);
            let result = verify_market_outcome(
                &env,
                2,
                &market,
                AdapterType::Pyth,
                true,
                &BytesN::from_array(&env, &[0u8; 64]),
            );
            // Adapter enabled but not fully wired → UnauthorizedOracle (clear misconfig error).
            assert_eq!(result, Err(ContractError::UnauthorizedOracle));
        });
    }

    #[test]
    fn test_verify_fails_closed_when_adapters_enabled() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Enable oracle adapters
            crate::storage::enable_oracle_adapters(&env);

            // Even with a valid-looking pubkey, Ed25519 verification should fail
            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);

            let result = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            assert_eq!(result, Err(ContractError::UnauthorizedOracle));
        });
    }

    #[test]
    fn test_upgrade_order_safety_adapters_must_be_enabled_first() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Initially, no adapters are registered
            assert!(!crate::storage::has_oracle_adapters(&env));

            // Ed25519 would work at this point (if we provided valid signature)
            // But once adapters are enabled, it must be rejected

            // Simulate resolution contract upgrade setting up adapters
            crate::storage::enable_oracle_adapters(&env);
            assert!(crate::storage::has_oracle_adapters(&env));

            // Now Ed25519 must ALWAYS fail (fail-closed)
            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);

            let result = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            assert_eq!(result, Err(ContractError::UnauthorizedOracle));
        });
    }
}