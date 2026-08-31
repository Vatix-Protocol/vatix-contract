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

use crate::error::ContractError;
use crate::types::Market;
use soroban_sdk::{Bytes, BytesN, Env};

/// Construct the message that the oracle signs.
///
/// Message format: `keccak256(market_id_be || outcome_byte)`
/// - `market_id`: u32 big-endian (4 bytes)
/// - `outcome_byte`: `0x01` = YES, `0x00` = NO
pub fn construct_oracle_message(env: &Env, market_id: u32, outcome: bool) -> BytesN<32> {
    let mut message = Bytes::new(env);
    message.append(&Bytes::from_slice(env, &market_id.to_be_bytes()));
    message.append(&Bytes::from_slice(env, &[u8::from(outcome)]));
    env.crypto().keccak256(&message).into()
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
    env.crypto()
        .ed25519_verify(oracle_pubkey, &message.into(), signature);

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

#[cfg(test)]
mod tests {
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
    #[should_panic]
    fn test_verify_invalid_signature() {
        let env = Env::default();
        verify_oracle_signature(
            &env,
            123u32,
            true,
            &BytesN::random(&env),
            &BytesN::random(&env),
        )
        .unwrap();
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

    #[test]
    fn test_adapter_enabled_multiple_calls_idempotent() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Enable adapters multiple times
            crate::storage::enable_oracle_adapters(&env);
            crate::storage::enable_oracle_adapters(&env);
            crate::storage::enable_oracle_adapters(&env);

            // Each time should still be enabled
            assert!(crate::storage::has_oracle_adapters(&env));

            // Ed25519 must still fail
            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);
            let result = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            assert_eq!(result, Err(ContractError::UnauthorizedOracle));
        });
    }

    #[test]
    fn test_fail_closed_rejects_all_pubkeys_when_adapters_enabled() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            crate::storage::enable_oracle_adapters(&env);

            // Test various pubkey values — all should fail
            let test_keys = vec![
                BytesN::from_array(&env, &[1u8; 32]),
                BytesN::from_array(&env, &[255u8; 32]),
                BytesN::from_array(&env, &[42u8; 32]),
                BytesN::from_array(&env, &[128u8; 32]),
            ];

            let signature = BytesN::from_array(&env, &[0u8; 64]);

            for pubkey in test_keys {
                let result = verify_oracle_signature(&env, 1u32, true, &signature, &pubkey);
                assert_eq!(
                    result,
                    Err(ContractError::UnauthorizedOracle),
                    "pubkey should fail with adapters enabled"
                );
            }
        });
    }

    #[test]
    fn test_fail_closed_different_outcomes_all_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            crate::storage::enable_oracle_adapters(&env);

            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);

            // Test both YES (true) and NO (false) outcomes
            let yes_result = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            let no_result = verify_oracle_signature(&env, 1u32, false, &signature, &oracle_pubkey);

            assert_eq!(yes_result, Err(ContractError::UnauthorizedOracle));
            assert_eq!(no_result, Err(ContractError::UnauthorizedOracle));
        });
    }

    #[test]
    fn test_fail_closed_different_markets_all_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            crate::storage::enable_oracle_adapters(&env);

            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);

            // Test different market IDs
            let market_1 = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            let market_100 = verify_oracle_signature(&env, 100u32, true, &signature, &oracle_pubkey);
            let market_u32_max =
                verify_oracle_signature(&env, u32::MAX, true, &signature, &oracle_pubkey);

            assert_eq!(market_1, Err(ContractError::UnauthorizedOracle));
            assert_eq!(market_100, Err(ContractError::UnauthorizedOracle));
            assert_eq!(market_u32_max, Err(ContractError::UnauthorizedOracle));
        });
    }

    #[test]
    fn test_zero_key_rejected_without_adapters() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Adapters NOT enabled
            assert!(!crate::storage::has_oracle_adapters(&env));

            let zero_key = BytesN::from_array(&env, &[0u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);

            let result = verify_oracle_signature(&env, 1u32, true, &signature, &zero_key);
            assert_eq!(
                result,
                Err(ContractError::UnauthorizedOracle),
                "zero key should be rejected even without adapters"
            );
        });
    }

    #[test]
    fn test_fail_closed_is_irreversible() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Start with adapters disabled
            assert!(!crate::storage::has_oracle_adapters(&env));

            // Enable adapters
            crate::storage::enable_oracle_adapters(&env);
            assert!(crate::storage::has_oracle_adapters(&env));

            // Try to verify a signature (should fail)
            let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
            let signature = BytesN::from_array(&env, &[0u8; 64]);
            let result1 = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            assert_eq!(result1, Err(ContractError::UnauthorizedOracle));

            // Try again (should still fail — state is persistent)
            let result2 = verify_oracle_signature(&env, 1u32, true, &signature, &oracle_pubkey);
            assert_eq!(result2, Err(ContractError::UnauthorizedOracle));

            // Verify no storage mutation happened
            assert!(crate::storage::has_oracle_adapters(&env));
        });
    }

    #[test]
    fn test_adapter_flag_persists_across_function_calls() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Enable adapters in one "call"
            crate::storage::enable_oracle_adapters(&env);

            // Simulate a new entry point call checking the flag
            let has_adapters_1 = crate::storage::has_oracle_adapters(&env);
            assert!(has_adapters_1);

            // Simulate another entry point call
            let has_adapters_2 = crate::storage::has_oracle_adapters(&env);
            assert!(has_adapters_2);

            // Flag should be consistent (persistent storage)
            assert_eq!(has_adapters_1, has_adapters_2);
        });
    }
