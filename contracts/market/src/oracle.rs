// TODO(#139): The oracle module currently uses a simple Ed25519 signature
// scheme with a single trusted pubkey stored per market. This needs to be
// replaced with a decentralised oracle integration (e.g. a Reflector or
// Pyth price-feed adapter) so that market resolution does not rely on a
// single off-chain signer. Tracked in:
// https://github.com/Vatix-Protocol/vatix-contract/issues/139

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
/// # Errors
/// - [`ContractError::UnauthorizedOracle`] if `oracle_pubkey` is the zero key.
/// - Panics if the Ed25519 signature is invalid (SDK limitation — see TODO below).
///
/// # Security
/// Uses Ed25519 signature verification via the Soroban crypto module.
///
/// TODO: `ed25519_verify` panics on invalid signatures. Consider `secp256k1_recover`
/// for proper error handling.
pub fn verify_oracle_signature(
    env: &Env,
    market_id: u32,
    outcome: bool,
    signature: &BytesN<64>,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
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
}
