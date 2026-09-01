//! #773 — Resolution flow e2e against the real Market WASM.
//!
//! These tests replace the earlier scaffold that wired the Resolution contract
//! to a randomly-generated (non-existent) market address.  Every cross-contract
//! call in the resolution lifecycle now hits the *real* `MarketContract`:
//!
//!   `ResolutionContract::propose`
//!       → `MarketContract::verify_signature`   (signature pre-validation)
//!       → `MarketContract::get_market_status`  (active-market guard)
//!       → `MarketContract::get_collateral_token` (bond token lookup)
//!
//!   `ResolutionContract::finalize`
//!       → `MarketContract::resolve_market`     (final state transition)
//!
//! ## Setup summary
//!
//! 1. Register & initialize `MarketContract` (admin, oracle keypair, collateral token).
//!    Bootstrap storage directly so V1 oracle signatures remain enabled — the
//!    resolution contract's `propose` uses the V1 `verify_signature` cross-
//!    contract call.  (Using the public `initialize` entrypoint would write
//!    `oracle_v1_disabled = true` per #701, which is correct for production
//!    but would break the V1 resolution path exercised here.)
//! 2. Register & initialize `ResolutionContract` pointing at the real market.
//! 3. Wire the resolution contract into the market via the timelocked
//!    `propose_resolution_contract` → fast-forward ledger →
//!    `execute_resolution_contract` path.
//! 4. Create a real market, fund proposer with collateral for the bond, then
//!    run propose → (challenge window) → finalize and assert the market is
//!    `Resolved`.

#[allow(dead_code)]
mod helpers;

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env, String,
};
use vatix_market_contract::{storage as market_storage, types::MarketStatus, MarketContract, MarketContractClient};
use vatix_resolution_contract::{
    types::CandidateStatus,
    ResolutionContract, ResolutionContractClient,
};

/// Challenge window used across all tests in this module (5 minutes).
const CHALLENGE_WINDOW: u64 = 300;

/// Minimum proposer bond required by the resolution contract (stroops).
const MIN_BOND: i128 = 10_000_000;

/// Extra margin added to `signature_expiry` so oracle messages don't expire
/// during the test's challenge window.
const SIG_EXPIRY_BUFFER: u64 = CHALLENGE_WINDOW + 3_600;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a fresh Ed25519 oracle keypair and return the on-chain public key
/// together with the signing key used to produce oracle signatures.
fn oracle_keypair(env: &Env) -> (BytesN<32>, SigningKey) {
    let key = SigningKey::generate(&mut OsRng);
    let pubkey = BytesN::from_array(env, &key.verifying_key().to_bytes());
    (pubkey, key)
}

/// Sign `(market_id, outcome)` with the oracle key and return the 64-byte
/// signature the market contract will accept via `verify_signature`.
fn oracle_sign(env: &Env, key: &SigningKey, market_id: u32, outcome: bool) -> BytesN<64> {
    let message = vatix_market_contract::oracle::construct_oracle_message(env, market_id, outcome);
    let sig = key.sign(message.to_array().as_slice());
    BytesN::from_array(env, &sig.to_bytes())
}

/// Full test harness: real MarketContract + real ResolutionContract, wired
/// together and ready for propose/finalize calls.
struct Harness {
    env: Env,
    market_client: MarketContractClient<'static>,
    market_id: Address,
    resolution_client: ResolutionContractClient<'static>,
    admin: Address,
    collateral_token: Address,
    oracle_key: SigningKey,
    /// The numeric market ID returned by `initialize_market`.
    numeric_market_id: u32,
}

const BOND_AMOUNT: i128 = 10_000_000;

// ── propose ───────────────────────────────────────────────────────────────────

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full happy-path: propose → challenge window passes → finalize → market resolved.
///
/// This is the primary #773 regression test: `finalize` must actually call
/// `MarketContract::resolve_market` on the real contract and transition its
/// status to `Resolved`.
#[test]
fn e2e_propose_finalize_resolves_real_market() {
    let h = Harness::new();
    let env = &h.env;

    let outcome = true;
    let signature = h.sign(outcome);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    // Fund the proposer for the bond.
    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 2);

    // Step 1: propose — cross-contracts to market's verify_signature and
    //         get_collateral_token / get_market_status.
    let candidate_id = h.resolution_client.propose(
        &proposer,
        &h.numeric_market_id,
        &outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://evidence-hash"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    let candidate = h.resolution_client
        .get_candidate(&candidate_id)
        .expect("candidate should exist after propose");
    assert_eq!(candidate.status, CandidateStatus::Proposed);
    assert_eq!(candidate.market_id, h.numeric_market_id);
    assert_eq!(candidate.outcome, outcome);

    // Market must still be Active — finalize hasn't run yet.
    assert_eq!(h.market_status(), MarketStatus::Active);

    let proposer = Address::generate(&env);
    let id1 = client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence-1"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );
    let id2 = client.propose(
        &proposer,
        &2u32,
        &false,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence-2"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    // Step 3: finalize — cross-contracts to market's resolve_market.
    let finalizer = Address::generate(env);
    let finalized = h.resolution_client.finalize(&finalizer, &candidate_id);

    // Resolution contract state.
    assert_eq!(finalized.status, CandidateStatus::Finalized);
    assert!(finalized.finalized_at.is_some());
    assert_eq!(finalized.outcome, outcome);
    assert_eq!(finalized.signature, signature);

    let proposer = Address::generate(&env);
    let expiry = env.ledger().timestamp() + CHALLENGE_WINDOW + 100;
    client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &expiry,
        &make_uri(&env, "ipfs://first"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    let result = client.try_propose(
        &proposer,
        &1u32,
        &false,
        &make_signature(&env),
        &expiry,
        &make_uri(&env, "ipfs://second"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );
}

/// Verifies that a challenged candidate cannot finalize, and therefore the
/// real market remains Active — not inadvertently resolved.
#[test]
fn e2e_challenged_candidate_leaves_market_active() {
    let h = Harness::new();
    let env = &h.env;

    let outcome = false;
    let signature = h.sign(outcome);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 2);

    let candidate_id = h.resolution_client.propose(
        &proposer,
        &h.numeric_market_id,
        &outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://evidence"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    // Challenge within the window.
    let challenger = Address::generate(env);
    h.mint(&challenger, MIN_BOND * 2);
    h.resolution_client.challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://challenge-evidence"),
        &BOND_AMOUNT,
    );

    let candidate = h.resolution_client
        .get_candidate(&candidate_id)
        .expect("candidate should exist");
    assert_eq!(candidate.status, CandidateStatus::Challenged);
    assert_eq!(candidate.challenged_by, Some(challenger));
}

#[test]
fn challenge_after_window_closes_is_rejected() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    // Advance past the challenge window.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let challenger = Address::generate(&env);
    let result = client.try_challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://late-challenge"),
        &BOND_AMOUNT,
    );

    // Market stays Active — no spurious resolve_market call.
    assert_eq!(
        h.market_status(),
        MarketStatus::Active,
        "challenged candidate must leave market Active"
    );
}

/// Verifies that finalize fails before the challenge window closes, and that
/// the real market is not prematurely resolved.
#[test]
fn e2e_finalize_before_window_rejects_and_market_stays_active() {
    let h = Harness::new();
    let env = &h.env;

    let outcome = true;
    let signature = h.sign(outcome);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 2);

    let candidate_id = h.resolution_client.propose(
        &proposer,
        &h.numeric_market_id,
        &outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://evidence"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    // Do NOT advance time — window is still open.
    let finalizer = Address::generate(env);
    let result = h.resolution_client.try_finalize(&finalizer, &candidate_id);
    assert!(result.is_err(), "finalize while window open must fail");

    assert_eq!(
        h.market_status(),
        MarketStatus::Active,
        "premature finalize attempt must leave market Active"
    );
    let _ = candidate_id;
}

/// Duplicate proposal for the same market is rejected.  The real market must
/// remain Active throughout.
#[test]
fn e2e_duplicate_proposal_rejected() {
    let h = Harness::new();
    let env = &h.env;

    let outcome = true;
    let signature = h.sign(outcome);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 4);

    // First proposal succeeds.
    h.resolution_client.propose(
        &proposer,
        &h.numeric_market_id,
        &outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://first"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    // Second proposal for the same market must fail.
    let result = h.resolution_client.try_propose(
        &proposer,
        &h.numeric_market_id,
        &!outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://second"),
        &CHALLENGE_WINDOW,
        &MIN_BOND,
    );
    assert!(result.is_err(), "duplicate proposal must be rejected");
    assert_eq!(h.market_status(), MarketStatus::Active);
}

/// Invalid oracle signature causes `propose` to be rejected (real market's
/// `verify_signature` rejects it) — the invalid-sig guard works e2e.
#[test]
fn e2e_invalid_signature_rejected_by_market() {
    let h = Harness::new();
    let env = &h.env;

    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 2);

    // All-zero signature is explicitly rejected by the market contract.
    let bad_sig = BytesN::from_array(env, &[0u8; 64]);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    let result = h.resolution_client.try_propose(
        &proposer,
        &h.numeric_market_id,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    let challenger = Address::generate(&env);
    client.challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://dispute"),
        &BOND_AMOUNT,
    );

    // Advance past window — but challenged candidates still cannot finalize.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);

    assert!(
        result.is_err(),
        "propose with all-zero signature must be rejected by real market"
    );
}

/// Propose with an outcome of `false` (NO) and finalize — market must resolve
/// to NO, not YES.
#[test]
fn e2e_no_outcome_resolves_correctly() {
    let h = Harness::new();
    let env = &h.env;

    let outcome = false;
    let signature = h.sign(outcome);
    let sig_expiry = h.now() + SIG_EXPIRY_BUFFER;

    let proposer = Address::generate(env);
    h.mint(&proposer, MIN_BOND * 2);

    let candidate_id = h.resolution_client.propose(
        &proposer,
        &h.numeric_market_id,
        &outcome,
        &signature,
        &sig_expiry,
        &String::from_str(env, "ipfs://no-evidence"),
        &CHALLENGE_WINDOW,
        &BOND_AMOUNT,
    );

    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let finalizer = Address::generate(env);
    let finalized = h.resolution_client.finalize(&finalizer, &candidate_id);

    assert_eq!(finalized.outcome, false);
    assert_eq!(h.market_status(), MarketStatus::Resolved);

    // Confirm the stored market result is NO.
    let market = env.as_contract(&h.market_id, || {
        market_storage::get_market(env, h.numeric_market_id)
            .unwrap()
            .unwrap()
    });
    assert_eq!(market.result, Some(false), "market result must be NO");
}
