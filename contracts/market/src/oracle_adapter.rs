#![allow(dead_code)]

//! Oracle adapter interface — feature-gated stub for issue #139.
//!
//! Provides an `OracleAdapter` trait that abstracts over the existing
//! Ed25519 single-signer path, the Reflector on-chain oracle, and Pyth.
//! Concrete implementations for Reflector and Pyth are unimplemented stubs;
//! see docs/adr-001-oracle-adapter.md for the design rationale and testnet
//! comparison.
//!
//! # no_std / Soroban note
//! `dyn OracleAdapter` requires heap allocation and is unavailable in this
//! `#![no_std]` crate.  Callers should either monomorphise over a concrete
//! adapter type (`impl OracleAdapter`) or use the [`AnyAdapter`] enum for
//! runtime dispatch without an allocator.

use crate::error::ContractError;
use soroban_sdk::{Address, Bytes, BytesN, Env};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Adapter-agnostic interface for resolving a prediction-market outcome.
///
/// Each implementor bridges between the contract's binary `(market_id,
/// outcome)` model and a specific oracle provider's on-chain proof mechanism.
pub trait OracleAdapter {
    /// Verify that `outcome` is the correct resolution for `market_id`.
    ///
    /// `proof` carries adapter-specific evidence:
    /// - [`Ed25519Adapter`]: exactly 64 bytes — the Ed25519 signature produced
    ///   by the market's stored oracle key over
    ///   `keccak256(market_id_be || outcome_byte)`.
    /// - [`ReflectorAdapter`]: empty (`Bytes::new`); the adapter fetches the
    ///   price on-chain from the Reflector contract.
    /// - [`PythAdapter`]: raw Wormhole VAA bytes containing the price
    ///   attestation; the adapter submits them to the Pyth receiver contract
    ///   before reading the verified price.
    ///
    /// # Errors
    /// Returns [`ContractError::InvalidSignature`] or
    /// [`ContractError::UnauthorizedOracle`] on verification failure.
    fn verify_outcome(
        &self,
        env: &Env,
        market_id: u32,
        outcome: bool,
        proof: &Bytes,
    ) -> Result<(), ContractError>;
}

// ---------------------------------------------------------------------------
// Ed25519 adapter — wraps the existing single-signer path
// ---------------------------------------------------------------------------

/// Wraps the existing Ed25519 single-signer path as an [`OracleAdapter`].
///
/// `proof` must be exactly 64 bytes (the Ed25519 signature).  Delegates to
/// [`crate::oracle::verify_oracle_signature`] so behaviour is identical to
/// the pre-adapter code path.
pub struct Ed25519Adapter<'a> {
    pub oracle_pubkey: &'a BytesN<32>,
}

impl<'a> OracleAdapter for Ed25519Adapter<'a> {
    fn verify_outcome(
        &self,
        env: &Env,
        market_id: u32,
        outcome: bool,
        proof: &Bytes,
    ) -> Result<(), ContractError> {
        let sig: BytesN<64> = BytesN::try_from(proof.clone())
            .map_err(|_| ContractError::InvalidSignature)?;
        crate::oracle::verify_oracle_signature(env, market_id, outcome, &sig, self.oracle_pubkey)
    }
}

// ---------------------------------------------------------------------------
// Reflector adapter stub
// ---------------------------------------------------------------------------

/// Stub for the [Reflector](https://reflector.network) on-chain price oracle.
///
/// Reflector is a Stellar-native, threshold-multisig federated oracle.
/// Integration is a single cross-contract call — no off-chain keeper required.
/// The adapter fetches `lastprice(asset)` and compares the returned price
/// against a market-stored `resolution_price` threshold to derive the outcome.
///
/// Testnet contract (2026-06-20):
/// `CAZP4SMCQX7L6O42AT4GLLRRSFDXPXS7IH7MMHZ52QWUQBFPXFQVMGQ`
///
/// # Status
/// Unimplemented stub — see docs/adr-001-oracle-adapter.md.
pub struct ReflectorAdapter {
    /// Address of the Reflector contract on the target network.
    pub contract_id: Address,
}

impl OracleAdapter for ReflectorAdapter {
    fn verify_outcome(
        &self,
        _env: &Env,
        _market_id: u32,
        _outcome: bool,
        _proof: &Bytes,
    ) -> Result<(), ContractError> {
        // TODO(#139): Cross-contract call to fetch the price and evaluate
        // the resolution condition.
        //
        // Sketch:
        //   let args = (asset_symbol,).into_val(_env);
        //   let (price, _ts): (i128, u64) =
        //       _env.invoke_contract(&self.contract_id, &symbol_short!("lastprice"), args);
        //   let resolved_yes = price >= market.resolution_price;
        //   if resolved_yes != _outcome { return Err(ContractError::InvalidSignature); }
        //   Ok(())
        unimplemented!("Reflector adapter — tracked in #139")
    }
}

// ---------------------------------------------------------------------------
// Pyth adapter stub
// ---------------------------------------------------------------------------

/// Stub for the [Pyth Network](https://pyth.network) cross-chain price oracle.
///
/// Pyth on Soroban uses a pull model: the resolution caller (or a keeper)
/// must first submit a Wormhole VAA via `update_price_feeds`, after which the
/// verified price can be read with `get_price`.  `proof` carries the raw VAA
/// bytes from the Hermes off-chain API.
///
/// Testnet receiver contract (Stellar testnet, 2026-06-20):
/// `HDWN46CTTXDZ5L5SWKQFUU25L5R2L6XNMCPDWP34PZMBVQJMZAPDVSN`
///
/// # Status
/// Unimplemented stub — see docs/adr-001-oracle-adapter.md.
pub struct PythAdapter {
    /// Address of the Pyth Soroban receiver contract on the target network.
    pub contract_id: Address,
    /// 32-byte Pyth price-feed ID for the asset this market tracks.
    pub price_feed_id: BytesN<32>,
}

impl OracleAdapter for PythAdapter {
    fn verify_outcome(
        &self,
        _env: &Env,
        _market_id: u32,
        _outcome: bool,
        _proof: &Bytes,
    ) -> Result<(), ContractError> {
        // TODO(#139): Two-step Pyth integration.
        //
        // Step 1 — submit VAA (caller passes Hermes-fetched bytes in `proof`):
        //   _env.invoke_contract(&self.contract_id,
        //       &symbol_short!("upd_feeds"), (_proof.clone(),).into_val(_env));
        //
        // Step 2 — read verified price:
        //   let price: i64 = _env.invoke_contract(&self.contract_id,
        //       &symbol_short!("get_price"), (self.price_feed_id.clone(),).into_val(_env));
        //
        //   let resolved_yes = price >= market.resolution_price as i64;
        //   if resolved_yes != _outcome { return Err(ContractError::InvalidSignature); }
        //   Ok(())
        unimplemented!("Pyth adapter — tracked in #139")
    }
}

// ---------------------------------------------------------------------------
// Runtime-dispatch enum (no heap required)
// ---------------------------------------------------------------------------

/// Runtime-dispatch wrapper over the three adapter variants.
///
/// Use this when the adapter kind is determined at runtime but `dyn
/// OracleAdapter` is unavailable (no heap in `#![no_std]`).
pub enum AnyAdapter<'a> {
    Ed25519(Ed25519Adapter<'a>),
    Reflector(ReflectorAdapter),
    Pyth(PythAdapter),
}

impl<'a> OracleAdapter for AnyAdapter<'a> {
    fn verify_outcome(
        &self,
        env: &Env,
        market_id: u32,
        outcome: bool,
        proof: &Bytes,
    ) -> Result<(), ContractError> {
        match self {
            AnyAdapter::Ed25519(a) => a.verify_outcome(env, market_id, outcome, proof),
            AnyAdapter::Reflector(a) => a.verify_outcome(env, market_id, outcome, proof),
            AnyAdapter::Pyth(a) => a.verify_outcome(env, market_id, outcome, proof),
        }
    }
}
