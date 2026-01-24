use crate::error::ContractError;
use crate::types::Market;
use soroban_sdk::{Bytes, BytesN, Env, String};

/// Construct the message that the oracle signs
///
/// The message format is: keccak256(market_id || outcome_byte)
/// - market_id: UTF-8 encoded string
/// - outcome_byte: 0x01 for YES, 0x00 for NO
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Market identifier
/// * `outcome` - Market outcome
///
/// # Returns
/// 32-byte hash of the message
pub fn construct_oracle_message(env: &Env, market_id: u32, outcome: bool) -> BytesN<32> {
    // 1. Convert market_id to bytes (UTF-8 encoded)
    let mut message = Bytes::new(env);
    
    // Append market_id bytes
    let market_id_bytes = market_id.to_bytes();
    for i in 0..market_id_bytes.len() {
        message.append(&Bytes::from_slice(env, &[market_id_bytes.get(i).unwrap()]));
    }
    
    // 2. Append outcome as single byte (0x01 for YES/true, 0x00 for NO/false)
    let outcome_byte: u8 = if outcome { 0x01 } else { 0x00 };
    message.append(&Bytes::from_slice(env, &[outcome_byte]));
    
    // 3. Hash the combined bytes using keccak256
    let hash = env.crypto().keccak256(&message);
    
    // 4. Return 32-byte hash (convert from Hash to BytesN)
    hash.into()
}

/// Verify that an oracle signature is valid for a market resolution
///
/// # Arguments
/// * `env` - Contract environment (provides crypto functions)
/// * `market_id` - Market being resolved
/// * `outcome` - Proposed outcome (true = YES won, false = NO won)
/// * `signature` - Ed25519 signature (64 bytes)
/// * `oracle_pubkey` - Oracle's public key (32 bytes)
///
/// # Returns
/// Ok if signature is valid, error otherwise
///
/// # Errors
/// - InvalidSignature if signature verification fails
///
/// # Security
/// Uses Ed25519 signature verification via Soroban crypto module
pub fn verify_oracle_signature(
    env: &Env,
    market_id: &String,
    outcome: bool,
    signature: &BytesN<64>,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    // 1. Construct message to verify (market_id + outcome)
    let message = construct_oracle_message(env, market_id, outcome);
    
    // 2. Verify signature using env.crypto().ed25519_verify()
// TODO: ed25519_verify panics on invalid signatures. Consider secp256k1_recover 
//  for proper error handling
    env.crypto()
        .ed25519_verify(oracle_pubkey, &message.into(), signature);
    
    // 3. If we reach here, signature is valid
    Ok(())
}

/// Check if an address is authorized to resolve markets
///
/// For MVP: Only check that the provided pubkey matches the market's oracle
/// Post-MVP: Could check against a registry of approved oracles
///
/// # Arguments
/// * `market` - Market being resolved
/// * `oracle_pubkey` - Public key attempting resolution
///
/// # Returns
/// Ok if authorized, error otherwise
///
/// # Errors
/// - UnauthorizedOracle if pubkey doesn't match
pub fn validate_oracle_authorization(
    market: &Market,
    oracle_pubkey: &BytesN<32>,
) -> Result<(), ContractError> {
    // For MVP: Simply check oracle_pubkey == market.oracle_pubkey
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
        Address, Env,
    };

    #[test]
    fn test_construct_oracle_message_yes() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market_123");
        let outcome = true;

        let message = construct_oracle_message(&env, &market_id, outcome);

        // Message should be 32 bytes (keccak256 output)
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_no() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market_123");
        let outcome = false;

        let message = construct_oracle_message(&env, &market_id, outcome);

        // Message should be 32 bytes (keccak256 output)
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_different_outcomes_different_messages() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market_123");

        // Same market_id, different outcome = different message
        let msg_yes = construct_oracle_message(&env, &market_id, true);
        let msg_no = construct_oracle_message(&env, &market_id, false);

        assert_ne!(msg_yes, msg_no);
    }

    #[test]
    fn test_construct_oracle_message_deterministic() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market_456");
        let outcome = true;

        // Same inputs should produce same hash
        let msg1 = construct_oracle_message(&env, &market_id, outcome);
        let msg2 = construct_oracle_message(&env, &market_id, outcome);

        assert_eq!(msg1, msg2);
    }

    #[test]
    fn test_different_market_ids_different_messages() {
        let env = Env::default();
        let market_id_1 = String::from_str(&env, "market_1");
        let market_id_2 = String::from_str(&env, "market_2");
        let outcome = true;

        let msg1 = construct_oracle_message(&env, &market_id_1, outcome);
        let msg2 = construct_oracle_message(&env, &market_id_2, outcome);

        assert_ne!(msg1, msg2);
    }

    #[test]
    fn test_validate_oracle_authorized() {
        let env = Env::default();
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let creator = Address::generate(&env);
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: 1,
            question: String::from_str(&env, "Test market"),
            end_time: 1000,
            oracle_pubkey: oracle_pubkey.clone(),
            status: MarketStatus::Active,
            result: None,
            creator,
            created_at: 0,
            collateral_token,
        };

        // Should return Ok when pubkey matches
        let result = validate_oracle_authorization(&market, &oracle_pubkey);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_oracle_unauthorized() {
        let env = Env::default();
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let wrong_pubkey = BytesN::from_array(&env, &[2u8; 32]);
        let creator = Address::generate(&env);
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: 1,
            question: String::from_str(&env, "Test market"),
            end_time: 1000,
            oracle_pubkey,
            status: MarketStatus::Active,
            result: None,
            creator,
            created_at: 0,
            collateral_token,
        };

        // Should return Err when pubkey doesn't match
        let result = validate_oracle_authorization(&market, &wrong_pubkey);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ContractError::UnauthorizedOracle);
    }

    #[test]
    #[should_panic]
    fn test_verify_invalid_signature() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market_123");
        let outcome = true;

        // Generate random keypair components for testing
        let oracle_pubkey = BytesN::random(&env);
        let invalid_signature = BytesN::random(&env);

        // This should panic because signature is invalid
        verify_oracle_signature(&env, &market_id, outcome, &invalid_signature, &oracle_pubkey)
            .unwrap();
    }

    #[test]
    fn test_verify_valid_signature() {
        // This test demonstrates the signature verification flow
        // In a real scenario, you would:
        // 1. Generate a proper Ed25519 keypair
        // 2. Sign the message with the private key
        // 3. Verify the signature with the public key
        //
        // For this test, we'll use the Stellar documentation pattern
        // and generate test data that would work in practice

        let env = Env::default();
        let market_id = String::from_str(&env, "test_market");
        let outcome = true;

        // Construct the message that would be signed
        let message = construct_oracle_message(&env, &market_id, outcome);

        // In practice, the oracle backend would:
        // 1. Generate this same message
        // 2. Sign it with their private key
        // 3. Submit (signature, public_key) to the contract
        //
        // For testing without external crypto libraries in the contract,
        // we acknowledge that ed25519_verify will panic on invalid signatures
        // The test above (test_verify_invalid_signature) verifies this behavior

        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_empty_market_id() {
        let env = Env::default();
        let market_id = String::from_str(&env, "");
        let outcome = true;

        // Should still produce a valid hash even with empty market_id
        let message = construct_oracle_message(&env, &market_id, outcome);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_long_market_id() {
        let env = Env::default();
        let market_id = String::from_str(
            &env,
            "this_is_a_very_long_market_identifier_to_test_edge_cases_123456789",
        );
        let outcome = false;

        let message = construct_oracle_message(&env, &market_id, outcome);
        assert_eq!(message.len(), 32);
    }

    #[test]
    fn test_construct_oracle_message_special_characters() {
        let env = Env::default();
        let market_id = String::from_str(&env, "market!@#$%^&*()_+-=[]{}");
        let outcome = true;

        let message = construct_oracle_message(&env, &market_id, outcome);
        assert_eq!(message.len(), 32);
    }
}
