#![allow(dead_code)]

//! Oracle adapter interface — feature-gated stub for issue #139.
//!
//! Provides an `OracleAdapter` trait that abstracts over the existing
//! Ed25519 single-signer path, the Reflector on-chain oracle, and Pyth.
//! The Reflector adapter is fully implemented; Pyth remains a stub.
//! See docs/adr-001-oracle-adapter.md for the design rationale and testnet
//! comparison.
//!
//! # no_std / Soroban note
//! `dyn OracleAdapter` requires heap allocation and is unavailable in this
//! `#![no_std]` crate.  Callers should either monomorphise over a concrete
//! adapter type (`impl OracleAdapter`) or use the [`AnyAdapter`] enum for
//! runtime dispatch without an allocator.

use crate::error::ContractError;
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec};

// ---------------------------------------------------------------------------
// Reflector cross-contract types
// ---------------------------------------------------------------------------

/// Mirrors the Reflector oracle's `Asset` contracttype for cross-contract calls.
///
/// The XDR layout must match the Reflector contract exactly; the variant names
/// and field types are taken from the Reflector open-source contract definition.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Asset {
    /// A native Stellar asset identified by its issuer address.
    Stellar(Address),
    /// A non-native asset identified by a symbol (e.g., `symbol_short!("BTC")`).
    Other(Symbol),
}

/// Mirrors the Reflector oracle's `PriceData` contracttype for cross-contract calls.
///
/// Returned by `lastprice(asset)`.  `price` is in Reflector's native units
/// (typically 7 decimal places; 1 USD = 10_000_000).
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

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
        let sig: BytesN<64> =
            BytesN::try_from(proof.clone()).map_err(|_| ContractError::InvalidSignature)?;
        crate::oracle::verify_oracle_signature(env, market_id, outcome, &sig, self.oracle_pubkey)
    }
}

// ---------------------------------------------------------------------------
// Reflector adapter
// ---------------------------------------------------------------------------

/// Adapter for the [Reflector](https://reflector.network) on-chain price oracle.
///
/// Reflector is a Stellar-native, threshold-multisig federated oracle (7
/// independent nodes).  Integration is a single synchronous cross-contract
/// call within the same ledger — no off-chain keeper is required.
///
/// # How resolution works
/// 1. `verify_outcome` calls `lastprice(asset)` on the Reflector contract.
/// 2. Reflector returns `Option<PriceData>` — `None` if no price is available.
/// 3. The fetched price is compared against `resolution_price`:
///    - `price >= resolution_price` → expected outcome is `true` (YES)
///    - `price <  resolution_price` → expected outcome is `false` (NO)
/// 4. If the passed `outcome` matches the derived result, `Ok(())` is returned;
///    otherwise `Err(ContractError::InvalidSignature)`.
///
/// # `proof` parameter
/// Must be empty (`Bytes::new`).  The adapter fetches all data it needs
/// directly from the Reflector contract; no caller-supplied proof is required.
///
/// # Stale / disconnected prices
/// When Reflector's `lastprice` returns `None` the adapter returns
/// [`ContractError::OraclePriceUnavailable`] so callers receive a typed,
/// recoverable error rather than a generic failure.
///
/// # Testnet contract (2026-06-20)
/// `CAZP4SMCQX7L6O42AT4GLLRRSFDXPXS7IH7MMHZ52QWUQBFPXFQVMGQ`
pub struct ReflectorAdapter {
    /// Address of the Reflector contract on the target network.
    pub contract_id: Address,
    /// Asset to query.  Use `Asset::Other(symbol_short!("BTC"))` for
    /// non-native assets and `Asset::Stellar(issuer_address)` for SAC tokens.
    pub asset: Asset,
    /// Price threshold in Reflector's native units (7 decimal places;
    /// 1 USD = 10_000_000).  Markets resolve YES when
    /// `lastprice >= resolution_price`.
    pub resolution_price: i128,
}

impl OracleAdapter for ReflectorAdapter {
    fn verify_outcome(
        &self,
        env: &Env,
        _market_id: u32,
        outcome: bool,
        _proof: &Bytes,
    ) -> Result<(), ContractError> {
        // Build the args vector for the cross-contract call.
        // Reflector's lastprice(asset: Asset) -> Option<PriceData>
        let args: Vec<Val> = soroban_sdk::vec![env, self.asset.clone().into_val(env)];

        let price_data_opt: Option<PriceData> = env.invoke_contract(
            &self.contract_id,
            &symbol_short!("lastprice"),
            args,
        );

        let price_data =
            price_data_opt.ok_or(ContractError::OraclePriceUnavailable)?;

        // price >= threshold → YES (true); price < threshold → NO (false)
        let expected_outcome = price_data.price >= self.resolution_price;

        if outcome == expected_outcome {
            Ok(())
        } else {
            Err(ContractError::InvalidSignature)
        }
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
        // Adapter unimplemented for Pyth integration; return a typed
        // `InvalidSignature` rather than panicking so callers receive a
        // recoverable `ContractError`.
        Err(ContractError::InvalidSignature)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl, symbol_short,
        testutils::Address as _,
        Address, Bytes, BytesN, Env,
    };

    // -----------------------------------------------------------------------
    // Mock Reflector contracts
    // -----------------------------------------------------------------------
    // Each mock returns a fixed price so we can exercise all code paths
    // without a live Reflector deployment.

    /// Returns price = 50_000_0000000 (above any reasonable test threshold).
    #[contract]
    struct MockReflectorAbove;

    #[contractimpl]
    impl MockReflectorAbove {
        pub fn lastprice(_env: Env, _asset: Asset) -> Option<PriceData> {
            Some(PriceData {
                price: 50_000_0000000_i128,
                timestamp: 1_000,
            })
        }
    }

    /// Returns price = 20_000_0000000 (below any reasonable test threshold).
    #[contract]
    struct MockReflectorBelow;

    #[contractimpl]
    impl MockReflectorBelow {
        pub fn lastprice(_env: Env, _asset: Asset) -> Option<PriceData> {
            Some(PriceData {
                price: 20_000_0000000_i128,
                timestamp: 1_000,
            })
        }
    }

    /// Returns price exactly equal to the test threshold (30_000_0000000).
    #[contract]
    struct MockReflectorAtThreshold;

    #[contractimpl]
    impl MockReflectorAtThreshold {
        pub fn lastprice(_env: Env, _asset: Asset) -> Option<PriceData> {
            Some(PriceData {
                price: 30_000_0000000_i128,
                timestamp: 1_000,
            })
        }
    }

    /// Simulates a stale / unavailable price feed (returns `None`).
    #[contract]
    struct MockReflectorNone;

    #[contractimpl]
    impl MockReflectorNone {
        pub fn lastprice(_env: Env, _asset: Asset) -> Option<PriceData> {
            None
        }
    }

    // Shared test threshold: 30_000_0000000 (i.e. $30,000.00 with 7 decimals).
    const THRESHOLD: i128 = 30_000_0000000_i128;

    fn btc_asset(env: &Env) -> Asset {
        Asset::Other(symbol_short!("BTC"))
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_price_above_threshold_outcome_yes_succeeds() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAbove, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // price (50_000) >= threshold (30_000) → expected YES
        let result = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn reflector_price_below_threshold_outcome_no_succeeds() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorBelow, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // price (20_000) < threshold (30_000) → expected NO
        let result = adapter.verify_outcome(&env, 1u32, false, &Bytes::new(&env));
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn reflector_price_at_threshold_counts_as_yes() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAtThreshold, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // price == threshold → price >= threshold is true → YES
        let result = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(result, Ok(()));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — outcome mismatch
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_price_above_threshold_outcome_no_fails() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAbove, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // price (50_000) >= threshold → expected YES, but caller claims NO
        let result = adapter.verify_outcome(&env, 1u32, false, &Bytes::new(&env));
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn reflector_price_below_threshold_outcome_yes_fails() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorBelow, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // price (20_000) < threshold → expected NO, but caller claims YES
        let result = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — stale / disconnected oracle
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_none_price_returns_oracle_price_unavailable() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorNone, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // Reflector returned None — neither YES nor NO should succeed
        let result_yes = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(result_yes, Err(ContractError::OraclePriceUnavailable));

        let result_no = adapter.verify_outcome(&env, 2u32, false, &Bytes::new(&env));
        assert_eq!(result_no, Err(ContractError::OraclePriceUnavailable));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — market_id is not used in price resolution
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_different_market_ids_same_result() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAbove, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // market_id is not forwarded to Reflector; all markets using this
        // adapter and threshold share the same on-chain price.
        let r1 = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        let r2 = adapter.verify_outcome(&env, 999u32, true, &Bytes::new(&env));
        assert_eq!(r1, Ok(()));
        assert_eq!(r2, Ok(()));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — proof bytes are ignored
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_non_empty_proof_is_ignored() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAbove, ());

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        };

        // Reflector derives outcome from the on-chain price, not from the proof.
        let proof = Bytes::from_array(&env, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let result = adapter.verify_outcome(&env, 1u32, true, &proof);
        assert_eq!(result, Ok(()));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — Stellar asset variant
    // -----------------------------------------------------------------------

    #[test]
    fn reflector_stellar_asset_variant_succeeds() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorAbove, ());
        let issuer = Address::generate(&env);

        let adapter = ReflectorAdapter {
            contract_id: mock_id,
            asset: Asset::Stellar(issuer),
            resolution_price: THRESHOLD,
        };

        // Mock ignores the asset; still resolves YES based on the price.
        let result = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(result, Ok(()));
    }

    // -----------------------------------------------------------------------
    // ReflectorAdapter — AnyAdapter dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn any_adapter_reflector_variant_dispatches_correctly() {
        let env = Env::default();
        let mock_id = env.register(MockReflectorBelow, ());

        let adapter = AnyAdapter::Reflector(ReflectorAdapter {
            contract_id: mock_id,
            asset: btc_asset(&env),
            resolution_price: THRESHOLD,
        });

        // price (20_000) < threshold → NO
        let result = adapter.verify_outcome(&env, 1u32, false, &Bytes::new(&env));
        assert_eq!(result, Ok(()));
    }

    // -----------------------------------------------------------------------
    // PythAdapter stub — unchanged behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn pyth_adapter_returns_invalid_signature() {
        let env = Env::default();
        let adapter = PythAdapter {
            contract_id: Address::generate(&env),
            price_feed_id: BytesN::from_array(&env, &[0u8; 32]),
        };
        let res = adapter.verify_outcome(&env, 1u32, true, &Bytes::new(&env));
        assert_eq!(res, Err(ContractError::InvalidSignature));
    }
}
