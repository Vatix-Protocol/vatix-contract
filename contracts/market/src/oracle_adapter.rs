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
use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec};

// ---------------------------------------------------------------------------
// Asset type used by Reflector (#379)
// ---------------------------------------------------------------------------

/// Identifies the asset to query from the Reflector oracle.
///
/// Matches the Reflector contract's `Asset` enum exactly so that
/// cross-contract serialisation succeeds.
#[contracttype]
#[derive(Clone)]
pub enum Asset {
    /// A Stellar-native SAC token identified by its issuer address.
    Stellar(Address),
    /// Any other asset identified by a 4-byte symbol (e.g. `symbol_short!("BTC")`).
    Other(soroban_sdk::Symbol),
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
        // Call `lastprice(asset)` on the Reflector contract.
        // Reflector returns `Option<PriceData>` where PriceData = { price: i128, timestamp: u64 }.
        // When the oracle has no recent price for the asset it returns None.
        let args: Vec<Val> =
            soroban_sdk::vec![env, self.asset.clone().into_val(env)];
        let price_data: Option<ReflectorPriceData> = env.invoke_contract(
            &self.contract_id,
            &Symbol::new(env, "lastprice"),
            args,
        );

        let data = price_data.ok_or(ContractError::OraclePriceUnavailable)?;

        // YES when price >= threshold, NO otherwise.
        let resolved_yes = data.price >= self.resolution_price;
        if resolved_yes != outcome {
            return Err(ContractError::InvalidSignature);
        }
        Ok(())
    }
}

/// Mirror of the `PriceData` struct returned by the Reflector contract.
///
/// Field names and order must match the Reflector contract's XDR encoding so
/// that cross-contract deserialisation succeeds.
#[contracttype]
#[derive(Clone)]
pub struct ReflectorPriceData {
    /// Price in Reflector's native units (7 decimal places; 1 USD = 10_000_000).
    pub price: i128,
    /// Unix timestamp (seconds) of the price observation.
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Pyth adapter
// ---------------------------------------------------------------------------

/// Mirror of the `Price` contracttype returned by the Pyth Soroban receiver.
///
/// Field names and order must match the Pyth contract's XDR encoding exactly
/// so that cross-contract deserialisation succeeds. See the Pyth Network
/// Soroban receiver contract source for the canonical definition.
#[contracttype]
struct PythPrice {
    /// Raw price value. Divide by `10^abs(exp)` to get a decimal price.
    price: i64,
    /// Confidence interval around `price`, in the same fixed-point units.
    conf: u64,
    /// Exponent: number of decimal places (usually negative, e.g. `-8`).
    exp: i32,
    /// Unix timestamp (seconds) when Pyth published this price.
    publish_time: u64,
}

/// Adapter for the [Pyth Network](https://pyth.network) cross-chain price oracle.
///
/// Pyth on Soroban uses a pull model:
/// 1. The resolution caller passes raw Wormhole VAA bytes in `proof`.
/// 2. The adapter submits them to the Pyth receiver contract via
///    `update_price_feeds`, which verifies the Wormhole signatures and stores
///    the attested price on-chain.
/// 3. The adapter then reads the verified price back via `get_price` using the
///    configured `price_feed_id` and compares it against `resolution_price`.
///
/// Testnet receiver contract (Stellar testnet, 2026-06-20):
/// `HDWN46CTTXDZ5L5SWKQFUU25L5R2L6XNMCPDWP34PZMBVQJMZAPDVSN`
pub struct PythAdapter {
    /// Address of the Pyth Soroban receiver contract on the target network.
    pub contract_id: Address,
    /// 32-byte Pyth price-feed ID for the asset this market tracks.
    pub price_feed_id: BytesN<32>,
    /// Price threshold in the same fixed-point integer units as `PythPrice.price`.
    /// The outcome resolves YES when `get_price().price >= resolution_price`.
    pub resolution_price: i64,
}

impl OracleAdapter for PythAdapter {
    /// Submits the VAA in `proof` to the Pyth receiver, reads back the verified
    /// price, and checks whether `outcome` matches the price-derived resolution.
    ///
    /// Returns [`ContractError::InvalidSignature`] when:
    /// - `proof` is empty (no VAA to submit), or
    /// - the derived outcome (`price >= resolution_price`) does not match `outcome`.
    fn verify_outcome(
        &self,
        env: &Env,
        _market_id: u32,
        outcome: bool,
        proof: &Bytes,
    ) -> Result<(), ContractError> {
        if proof.is_empty() {
            return Err(ContractError::InvalidSignature);
        }

        // Step 1 — submit VAA to Pyth receiver so it stores the verified price.
        let update_args: Vec<Val> = soroban_sdk::vec![env, proof.clone().into_val(env)];
        let _: () = env.invoke_contract(
            &self.contract_id,
            &Symbol::new(env, "update_price_feeds"),
            update_args,
        );

        // Step 2 — read the verified price for this feed.
        let price_args: Vec<Val> =
            soroban_sdk::vec![env, self.price_feed_id.clone().into_val(env)];
        let price_data: PythPrice = env.invoke_contract(
            &self.contract_id,
            &Symbol::new(env, "get_price"),
            price_args,
        );

        let resolved_yes = price_data.price >= self.resolution_price;
        if resolved_yes != outcome {
            return Err(ContractError::InvalidSignature);
        }
        Ok(())
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
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Env};

    // ---- Mock Pyth receiver contract ----

    /// A minimal mock of the Pyth Soroban receiver for unit tests.
    /// Registered in the Soroban test environment so that `invoke_contract`
    /// resolves without network I/O or Wormhole validation.
    #[contract]
    struct MockPyth;

    /// Controls what price `MockPyth.get_price` returns.
    #[contracttype]
    #[derive(Clone)]
    pub struct MockPythState {
        pub price: i64,
    }

    #[contractimpl]
    impl MockPyth {
        pub fn set_price(env: Env, price: i64) {
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("price"), &price);
        }

        pub fn update_price_feeds(env: Env, _data: Bytes) {
            // In tests we pre-set the price via `set_price`; the VAA is ignored.
            let _ = env;
        }

        pub fn get_price(env: Env, _feed_id: BytesN<32>) -> PythPrice {
            let price: i64 = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("price"))
                .unwrap_or(0);
            PythPrice { price, conf: 0, exp: -8, publish_time: 1_000_000 }
        }
    }

    fn setup_mock_pyth(env: &Env) -> Address {
        env.register(MockPyth, ())
    }

    fn vaa_bytes(env: &Env) -> Bytes {
        Bytes::from_slice(env, &[0xde, 0xad, 0xbe, 0xef])
    }

    // ---- PythAdapter tests ----

    #[test]
    fn pyth_returns_ok_when_price_meets_threshold_yes() {
        let env = Env::default();
        let contract_id = setup_mock_pyth(&env);
        let mock_client = MockPythClient::new(&env, &contract_id);
        mock_client.set_price(&1_000);

        let adapter = PythAdapter {
            contract_id,
            price_feed_id: BytesN::from_array(&env, &[0u8; 32]),
            resolution_price: 1_000,
        };
        // price (1_000) >= resolution_price (1_000) → YES
        assert_eq!(
            adapter.verify_outcome(&env, 1, true, &vaa_bytes(&env)),
            Ok(())
        );
    }

    #[test]
    fn pyth_returns_ok_when_price_below_threshold_no() {
        let env = Env::default();
        let contract_id = setup_mock_pyth(&env);
        let mock_client = MockPythClient::new(&env, &contract_id);
        mock_client.set_price(&999);

        let adapter = PythAdapter {
            contract_id,
            price_feed_id: BytesN::from_array(&env, &[0u8; 32]),
            resolution_price: 1_000,
        };
        // price (999) < resolution_price (1_000) → NO
        assert_eq!(
            adapter.verify_outcome(&env, 1, false, &vaa_bytes(&env)),
            Ok(())
        );
    }

    #[test]
    fn pyth_returns_invalid_signature_on_outcome_mismatch() {
        let env = Env::default();
        let contract_id = setup_mock_pyth(&env);
        let mock_client = MockPythClient::new(&env, &contract_id);
        // price exceeds threshold but caller claims NO
        mock_client.set_price(&2_000);

        let adapter = PythAdapter {
            contract_id,
            price_feed_id: BytesN::from_array(&env, &[1u8; 32]),
            resolution_price: 1_000,
        };
        assert_eq!(
            adapter.verify_outcome(&env, 1, false, &vaa_bytes(&env)),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn pyth_returns_invalid_signature_when_proof_is_empty() {
        let env = Env::default();
        let contract_id = setup_mock_pyth(&env);

        let adapter = PythAdapter {
            contract_id,
            price_feed_id: BytesN::from_array(&env, &[0u8; 32]),
            resolution_price: 500,
        };
        // Empty proof → no VAA to submit; must not panic
        assert_eq!(
            adapter.verify_outcome(&env, 1, true, &Bytes::new(&env)),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn pyth_any_adapter_resolves_correctly() {
        let env = Env::default();
        let contract_id = setup_mock_pyth(&env);
        let mock_client = MockPythClient::new(&env, &contract_id);
        mock_client.set_price(&300);

        let adapter = AnyAdapter::Pyth(PythAdapter {
            contract_id,
            price_feed_id: BytesN::from_array(&env, &[2u8; 32]),
            resolution_price: 500,
        });
        // price (300) < threshold (500) → NO
        assert_eq!(
            adapter.verify_outcome(&env, 5, false, &vaa_bytes(&env)),
            Ok(())
        );
    }
}

#[cfg(test)]
mod reflector_tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Env};

    // ---- Mock Reflector contract (#379) ----

    /// A minimal mock of the Reflector oracle for unit tests.
    /// `lastprice` returns a pre-seeded price or `None` to simulate unavailability.
    #[contract]
    struct MockReflector;

    #[contractimpl]
    impl MockReflector {
        /// Seed the price the mock returns.
        pub fn set_price(env: Env, price: i128) {
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("price"), &price);
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("has"), &true);
        }

        /// Clear the price — causes `lastprice` to return `None`.
        pub fn clear_price(env: Env) {
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("has"), &false);
        }

        pub fn lastprice(env: Env, _asset: Asset) -> Option<ReflectorPriceData> {
            let has: bool = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("has"))
                .unwrap_or(false);
            if !has {
                return None;
            }
            let price: i128 = env
                .storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("price"))
                .unwrap_or(0);
            Some(ReflectorPriceData { price, timestamp: 1_000_000 })
        }
    }

    fn setup_mock_reflector(env: &Env) -> Address {
        env.register(MockReflector, ())
    }

    fn dummy_asset(env: &Env) -> Asset {
        Asset::Other(soroban_sdk::symbol_short!("BTC"))
    }

    /// Testnet contract address for Reflector (as of 2026-06-20):
    /// `CAZP4SMCQX7L6O42AT4GLLRRSFDXPXS7IH7MMHZ52QWUQBFPXFQVMGQ`
    /// See docs/adr-001-oracle-adapter.md §Option B for integration details.

    #[test]
    fn reflector_resolves_yes_when_price_meets_threshold() {
        let env = Env::default();
        let contract_id = setup_mock_reflector(&env);
        MockReflectorClient::new(&env, &contract_id).set_price(&1_000_000);

        let adapter = ReflectorAdapter {
            contract_id,
            asset: dummy_asset(&env),
            resolution_price: 1_000_000,
        };
        // price (1_000_000) >= threshold (1_000_000) → YES
        assert_eq!(
            adapter.verify_outcome(&env, 1, true, &Bytes::new(&env)),
            Ok(())
        );
    }

    #[test]
    fn reflector_resolves_no_when_price_below_threshold() {
        let env = Env::default();
        let contract_id = setup_mock_reflector(&env);
        MockReflectorClient::new(&env, &contract_id).set_price(&999_999);

        let adapter = ReflectorAdapter {
            contract_id,
            asset: dummy_asset(&env),
            resolution_price: 1_000_000,
        };
        // price (999_999) < threshold (1_000_000) → NO
        assert_eq!(
            adapter.verify_outcome(&env, 1, false, &Bytes::new(&env)),
            Ok(())
        );
    }

    #[test]
    fn reflector_returns_invalid_signature_on_outcome_mismatch() {
        let env = Env::default();
        let contract_id = setup_mock_reflector(&env);
        MockReflectorClient::new(&env, &contract_id).set_price(&2_000_000);

        let adapter = ReflectorAdapter {
            contract_id,
            asset: dummy_asset(&env),
            resolution_price: 1_000_000,
        };
        // price >= threshold → YES, but caller claims NO
        assert_eq!(
            adapter.verify_outcome(&env, 1, false, &Bytes::new(&env)),
            Err(ContractError::InvalidSignature)
        );
    }

    #[test]
    fn reflector_returns_price_unavailable_when_oracle_returns_none() {
        let env = Env::default();
        let contract_id = setup_mock_reflector(&env);
        MockReflectorClient::new(&env, &contract_id).clear_price();

        let adapter = ReflectorAdapter {
            contract_id,
            asset: dummy_asset(&env),
            resolution_price: 1_000_000,
        };
        assert_eq!(
            adapter.verify_outcome(&env, 1, true, &Bytes::new(&env)),
            Err(ContractError::OraclePriceUnavailable)
        );
    }

    #[test]
    fn reflector_any_adapter_dispatch_works() {
        let env = Env::default();
        let contract_id = setup_mock_reflector(&env);
        MockReflectorClient::new(&env, &contract_id).set_price(&500);

        let adapter = AnyAdapter::Reflector(ReflectorAdapter {
            contract_id,
            asset: dummy_asset(&env),
            resolution_price: 1_000,
        });
        // price (500) < threshold (1_000) → NO
        assert_eq!(
            adapter.verify_outcome(&env, 7, false, &Bytes::new(&env)),
            Ok(())
        );
    }
}
