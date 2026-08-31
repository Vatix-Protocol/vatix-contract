#![cfg(test)]
//! Regression corpus for share/fee/payout math (#504, #758) and oracle-message (#757).
//!
//! Loads `test-vectors/fee-math.json`, `test-vectors/share-math.json`, and
//! `test-vectors/oracle-message.json` at test time and replays every case,
//! enforcing round-trip properties and canonical payload parity.

use crate::error::ContractError;
use crate::validation::calculate_fee;
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    vectors: std::vec::Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    id: std::string::String,
    #[allow(dead_code)]
    description: std::string::String,
    amount: std::string::String,
    fee_rate_bps: i128,
    expected: Expected,
}

#[derive(Deserialize)]
struct Expected {
    ok: Option<std::string::String>,
    error: Option<std::string::String>,
}

fn error_name(err: ContractError) -> &'static str {
    match err {
        ContractError::InvalidQuantity => "InvalidQuantity",
        ContractError::InvalidPrice => "InvalidPrice",
        ContractError::ArithmeticOverflow => "ArithmeticOverflow",
        _ => "Other",
    }
}

#[test]
fn fee_math_regression_corpus() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/fee-math.json"
    ))
    .expect("read test-vectors/fee-math.json");
    let corpus: Corpus = serde_json::from_str(&raw).expect("parse fee-math.json");

    assert!(
        corpus.vectors.len() >= 5,
        "corpus must document at least 5 cases"
    );

    for vector in &corpus.vectors {
        let amount: i128 = vector
            .amount
            .parse()
            .unwrap_or_else(|_| panic!("vector {}: invalid amount", vector.id));
        let result = calculate_fee(amount, vector.fee_rate_bps);

        match (&vector.expected.ok, &vector.expected.error) {
            (Some(expected_ok), None) => {
                let expected: i128 = expected_ok
                    .parse()
                    .unwrap_or_else(|_| panic!("vector {}: invalid expected.ok", vector.id));
                assert_eq!(
                    result,
                    Ok(expected),
                    "vector {}: unexpected fee result",
                    vector.id
                );

                // Round-trip fee property checks (#758)
                let fee = result.unwrap();
                assert!(fee >= 0, "vector {}: fee must be non-negative", vector.id);
                assert!(fee <= amount, "vector {}: fee cannot exceed amount", vector.id);
                assert_eq!(
                    (amount - fee) + fee,
                    amount,
                    "vector {}: fee round-trip addition invariant failed",
                    vector.id
                );
            }
            (None, Some(expected_err)) => {
                let err = result.expect_err(&std::format!(
                    "vector {}: expected error {}, got Ok",
                    vector.id,
                    expected_err
                ));
                assert_eq!(
                    error_name(err),
                    expected_err.as_str(),
                    "vector {}: wrong error variant",
                    vector.id
                );
            }
            _ => panic!(
                "vector {}: expected must have exactly one of ok/error",
                vector.id
            ),
        }
    }
}

// ── Share-collateral math regression corpus (#582, #758) ───────────────────

use crate::positions::calculate_locked_collateral;

#[derive(Deserialize)]
struct ShareCorpus {
    vectors: std::vec::Vec<ShareVector>,
}

#[derive(Deserialize)]
struct ShareVector {
    id: std::string::String,
    #[allow(dead_code)]
    description: std::string::String,
    yes_shares: std::string::String,
    no_shares: std::string::String,
    market_price: i128,
    expected: ShareExpected,
}

#[derive(Deserialize)]
struct ShareExpected {
    ok: std::string::String,
}

#[test]
fn share_math_regression_corpus() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/share-math.json"
    ))
    .expect("read test-vectors/share-math.json");
    let corpus: ShareCorpus = serde_json::from_str(&raw).expect("parse share-math.json");

    assert!(
        corpus.vectors.len() >= 3,
        "corpus must document at least 3 cases"
    );

    for vector in &corpus.vectors {
        let yes_shares: i128 = vector
            .yes_shares
            .parse()
            .unwrap_or_else(|_| panic!("vector {}: invalid yes_shares", vector.id));
        let no_shares: i128 = vector
            .no_shares
            .parse()
            .unwrap_or_else(|_| panic!("vector {}: invalid no_shares", vector.id));
        let expected: i128 = vector
            .expected
            .ok
            .parse()
            .unwrap_or_else(|_| panic!("vector {}: invalid expected.ok", vector.id));

        let result = calculate_locked_collateral(yes_shares, no_shares, vector.market_price);
        assert_eq!(
            result, expected,
            "vector {}: unexpected locked collateral",
            vector.id
        );

        // Round-trip symmetry check (#758): swapping shares and using complementary price
        // (10_000 - price) must produce identical locked collateral.
        let inverse_price = 10_000 - vector.market_price;
        let roundtrip_locked = calculate_locked_collateral(no_shares, yes_shares, inverse_price);
        assert_eq!(
            result, roundtrip_locked,
            "vector {}: share math round-trip symmetry check failed",
            vector.id
        );
    }
}

// ── Oracle Message V2 regression corpus (#757) ────────────────────────────

fn hex_to_bytes(hex: &str) -> std::vec::Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex byte"))
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> std::string::String {
    bytes.iter().map(|b| std::format!("{:02x}", b)).collect()
}

#[derive(Deserialize)]
struct OracleMessageVector {
    #[allow(dead_code)]
    description: std::string::String,
    #[allow(dead_code)]
    domain_separator: std::string::String,
    network_passphrase_hash_hex: std::string::String,
    market_id: u32,
    outcome: std::string::String,
    valid_until: u64,
    epoch: u32,
    raw_hex: std::string::String,
    keccak_hex: std::string::String,
    pubkey_hex: std::string::String,
    signature_hex: std::string::String,
}

#[test]
fn oracle_message_regression_corpus() {
    use crate::oracle::{
        build_oracle_preimage_v2, construct_oracle_message_v2, verify_oracle_signature_v2,
    };
    use soroban_sdk::{BytesN, Env};

    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-vectors/oracle-message.json"
    ))
    .expect("read test-vectors/oracle-message.json");
    let vector: OracleMessageVector =
        serde_json::from_str(&raw).expect("parse oracle-message.json");

    let env = Env::default();

    let passphrase_hash_bytes = hex_to_bytes(&vector.network_passphrase_hash_hex);
    let mut passphrase_hash_arr = [0u8; 32];
    passphrase_hash_arr.copy_from_slice(&passphrase_hash_bytes);
    let passphrase_hash = BytesN::from_array(&env, &passphrase_hash_arr);

    let outcome_bool = vector.outcome.to_uppercase() == "YES";

    // 1. Verify raw preimage hex match
    let preimage = build_oracle_preimage_v2(
        &env,
        &passphrase_hash,
        vector.market_id,
        outcome_bool,
        vector.valid_until,
        vector.epoch,
    );
    let mut preimage_bytes = std::vec![0u8; preimage.len() as usize];
    preimage.copy_into_slice(&mut preimage_bytes);
    assert_eq!(
        bytes_to_hex(&preimage_bytes),
        vector.raw_hex,
        "oracle-message preimage raw_hex mismatch"
    );

    // 2. Verify keccak256 message hash match
    let msg_hash = construct_oracle_message_v2(
        &env,
        &passphrase_hash,
        vector.market_id,
        outcome_bool,
        vector.valid_until,
        vector.epoch,
    );
    assert_eq!(
        bytes_to_hex(&msg_hash.to_array()),
        vector.keccak_hex,
        "oracle-message keccak_hex mismatch"
    );

    // 3. Verify signature verification succeeds against oracle pubkey
    let pubkey_bytes = hex_to_bytes(&vector.pubkey_hex);
    let mut pubkey_arr = [0u8; 32];
    pubkey_arr.copy_from_slice(&pubkey_bytes);
    let oracle_pubkey = BytesN::from_array(&env, &pubkey_arr);

    let sig_bytes = hex_to_bytes(&vector.signature_hex);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = BytesN::from_array(&env, &sig_arr);

    let verify_res = verify_oracle_signature_v2(
        &env,
        &passphrase_hash,
        vector.market_id,
        outcome_bool,
        vector.valid_until,
        vector.epoch,
        &signature,
        &oracle_pubkey,
    );
    assert_eq!(
        verify_res,
        Ok(()),
        "oracle-message vector signature verification failed"
    );
}

