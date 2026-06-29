//! Issue #327 — Scaffold propose/challenge/finalize resolution flow.
//!
//! These tests exercise the full on-chain resolution lifecycle:
//!   propose → (challenge window) → finalize
//! and the alternate branch:
//!   propose → challenge → (contested; no auto-finalize)
//!
//! The resolution contract is intentionally decoupled from the market contract:
//! `finalize` returns the signed candidate payload so an external caller can
//! relay it to `market.resolve_market`.

#[allow(dead_code)]
mod helpers;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String,
};
use vatix_resolution_contract::{
    types::{CandidateStatus, ResolutionCandidate},
    ResolutionContract, ResolutionContractClient,
};

const CHALLENGE_WINDOW: u64 = 300; // 5 minutes

fn scaffold() -> (Env, ResolutionContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let factory = Address::generate(&env);
    let market_contract = Address::generate(&env);

    let contract_id = env.register(ResolutionContract, ());
    ResolutionContractClient::new(&env, &contract_id)
        .initialize(&admin, &factory, &market_contract);

    let client = ResolutionContractClient::new(&env, &contract_id);
    (env, client, admin)
}

fn make_signature(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[0xABu8; 64])
}

fn make_uri(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ── propose ───────────────────────────────────────────────────────────────────

#[test]
fn propose_creates_candidate_with_proposed_status() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence-hash"),
        &CHALLENGE_WINDOW,
    );

    let candidate = client.get_candidate(&candidate_id).expect("candidate exists");
    assert_eq!(candidate.id, candidate_id);
    assert_eq!(candidate.market_id, 1u32);
    assert_eq!(candidate.outcome, true);
    assert_eq!(candidate.status, CandidateStatus::Proposed);
    assert!(candidate.challenged_by.is_none());
}

#[test]
fn propose_returns_incrementing_ids() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    let id1 = client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence-1"),
        &CHALLENGE_WINDOW,
    );
    let id2 = client.propose(
        &proposer,
        &2u32,
        &false,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://evidence-2"),
        &CHALLENGE_WINDOW,
    );

    assert_eq!(id1 + 1, id2, "candidate IDs should be auto-incremented");
}

#[test]
fn duplicate_proposal_for_same_market_is_rejected() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://first"),
        &CHALLENGE_WINDOW,
    );

    let result = client.try_propose(
        &proposer,
        &1u32,
        &false,
        &make_signature(&env),
        &make_uri(&env, "ipfs://second"),
        &CHALLENGE_WINDOW,
    );

    assert!(result.is_err(), "second proposal for same market must fail");
}

// ── challenge ─────────────────────────────────────────────────────────────────

#[test]
fn challenge_transitions_status_to_challenged() {
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
    );

    let challenger = Address::generate(&env);
    client.challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://challenge-evidence"),
    );

    let candidate = client.get_candidate(&candidate_id).expect("exists");
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
    );

    // Advance ledger past the challenge deadline.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let challenger = Address::generate(&env);
    let result = client.try_challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://late-challenge"),
    );

    assert!(result.is_err(), "challenge after window must be rejected");
}

// ── finalize ──────────────────────────────────────────────────────────────────

#[test]
fn finalize_after_window_returns_candidate_payload() {
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
    );

    // Advance time past the challenge window.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let finalizer = Address::generate(&env);
    let result: ResolutionCandidate = client.finalize(&finalizer, &candidate_id);

    assert_eq!(result.id, candidate_id);
    assert_eq!(result.status, CandidateStatus::Finalized);
    assert!(result.finalized_at.is_some());
    // The returned signature can be relayed to market.resolve_market.
    assert_eq!(result.signature, make_signature(&env));
}

#[test]
fn finalize_before_window_closes_is_rejected() {
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
    );

    // Do NOT advance time — window is still open.
    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);

    assert!(result.is_err(), "finalize while window open must fail");
}

#[test]
fn challenged_candidate_cannot_be_finalized() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1u32,
        &true,
        &make_signature(&env),
        &make_uri(&env, "ipfs://evidence"),
        &CHALLENGE_WINDOW,
    );

    let challenger = Address::generate(&env);
    client.challenge(
        &challenger,
        &candidate_id,
        &make_uri(&env, "ipfs://dispute"),
    );

    // Advance past window — but challenged candidates still cannot finalize.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);

    assert!(
        result.is_err(),
        "challenged candidate must not auto-finalize"
    );
}

// ── full flow ─────────────────────────────────────────────────────────────────

#[test]
fn full_propose_then_finalize_flow() {
    let (env, client, _admin) = scaffold();

    let proposer = Address::generate(&env);
    let market_id = 42u32;
    let outcome = false;

    // Step 1: Propose.
    let candidate_id = client.propose(
        &proposer,
        &market_id,
        &outcome,
        &make_signature(&env),
        &(env.ledger().timestamp() + CHALLENGE_WINDOW + 100),
        &make_uri(&env, "ipfs://full-flow-evidence"),
        &CHALLENGE_WINDOW,
    );

    assert_eq!(
        client
            .get_candidate(&candidate_id)
            .unwrap()
            .status,
        CandidateStatus::Proposed
    );

    // Step 2: Challenge window passes without a challenge.
    env.ledger().with_mut(|li| {
        li.timestamp += CHALLENGE_WINDOW + 1;
    });

    // Step 3: Finalize — the returned payload is ready for market.resolve_market.
    let finalizer = Address::generate(&env);
    let finalized = client.finalize(&finalizer, &candidate_id);

    assert_eq!(finalized.status, CandidateStatus::Finalized);
    assert_eq!(finalized.market_id, market_id);
    assert_eq!(finalized.outcome, outcome);

    // get_candidate_id_for_market correctly maps market → candidate.
    assert_eq!(
        client.get_candidate_id_for_market(&market_id),
        Some(candidate_id)
    );
}
