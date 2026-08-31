#![cfg(test)]
//! Regression corpus for share/fee/payout math (#504).
//!
//! Loads `test-vectors/fee-math.json` at test time and replays every case
//! through `validation::calculate_fee`, so a reintroduced rounding/overflow
//! bug fails this test instead of surfacing later in production.

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

// ── Share-collateral math regression corpus (#582) ─────────────────────────

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
    }
}
