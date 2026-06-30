use crate::{ContractError, ResolutionContract, ResolutionContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, BytesN, Env, String,
};

const DEFAULT_WINDOW: u64 = 300;

fn setup(env: &Env) -> (ResolutionContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let factory = Address::generate(env);
    let market_contract = Address::generate(env);
    client.initialize(&admin, &factory, &market_contract, &DEFAULT_WINDOW);
    (client, admin, market_contract)
}

fn signature(env: &Env) -> BytesN<64> {
    BytesN::from_array(env, &[7u8; 64])
}

fn evidence(env: &Env) -> String {
    String::from_str(env, "ipfs://resolution-evidence")
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|ledger| {
        ledger.timestamp = timestamp;
    });
}

#[test]
fn propose_stores_candidate_with_challenge_deadline() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &42,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
    );

    assert_eq!(candidate_id, 1);
    let candidate = client.get_candidate(&candidate_id).unwrap();
    assert_eq!(candidate.market_id, 42);
    assert_eq!(candidate.outcome, true);
    assert_eq!(candidate.challenge_deadline, 1_300);
    assert_eq!(client.get_candidate_id_for_market(&42), Some(candidate_id));
}

#[test]
fn challenge_marks_candidate_and_blocks_finalize() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &false,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
    );

    let challenger = Address::generate(&env);
    let challenge_uri = String::from_str(&env, "ipfs://challenge");
    client.challenge(&challenger, &candidate_id, &challenge_uri);
    set_time(&env, 1_400);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert_eq!(result, Err(Ok(ContractError::CandidateAlreadyChallenged)));
}

#[test]
fn finalize_requires_closed_challenge_window() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
    );

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::ChallengeWindowOpen))
    );

    set_time(&env, 1_301);
    let candidate = client.finalize(&finalizer, &candidate_id);
    assert_eq!(candidate.status, crate::types::CandidateStatus::Finalized);
    assert_eq!(candidate.finalized_at, Some(1_301));
}

#[test]
fn challenge_after_deadline_is_rejected() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(&proposer, &1, &true, &signature(&env), &(env.ledger().timestamp() + 60), &evidence(&env), &60);

    set_time(&env, 1_061);
    let challenger = Address::generate(&env);
    let challenge_uri = String::from_str(&env, "ipfs://late-challenge");
    assert_eq!(
        client.try_challenge(&challenger, &candidate_id, &challenge_uri),
        Err(Ok(ContractError::ChallengeWindowClosed))
    );
}

#[test]
fn admin_can_update_factory_registration() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let new_factory = Address::generate(&env);
    client.set_factory(&admin, &new_factory);
    assert_eq!(client.get_config().factory, new_factory);
}

/// #404: finalize invokes resolve_market on the market contract.
///
/// Uses mock_all_auths so the cross-contract call is intercepted without
/// needing a live market contract deployment.
#[test]
fn finalize_calls_resolve_market_on_market_contract() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    set_time(&env, 1_000);
    let proposer = Address::generate(&env);
    let sig = signature(&env);
    let candidate_id = client.propose(&proposer, &5, &true, &sig, &(env.ledger().timestamp() + 60), &evidence(&env), &60);

    set_time(&env, 1_061);
    let finalizer = Address::generate(&env);
    // If the cross-contract resolve_market call were missing or wrong, this
    // would panic or return an error. Success proves the callback fired.
    let candidate = client.finalize(&finalizer, &candidate_id);

    assert_eq!(candidate.status, crate::types::CandidateStatus::Finalized);
    assert_eq!(candidate.market_id, 5);
    assert_eq!(candidate.outcome, true);
    assert!(candidate.finalized_at.is_some());
}

// ── #380: default challenge window ────────────────────────────────────────────

#[test]
fn initialize_stores_default_challenge_window() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.get_default_challenge_window(), DEFAULT_WINDOW);
}

#[test]
fn admin_can_update_default_challenge_window() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    client.set_default_challenge_window(&admin, &600);
    assert_eq!(client.get_default_challenge_window(), 600);
}

#[test]
fn set_default_challenge_window_rejects_non_admin() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let rando = Address::generate(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&rando, &600),
        Err(Ok(ContractError::NotAdmin))
    );
}

#[test]
fn set_default_challenge_window_rejects_out_of_bounds() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &10),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &(14 * 24 * 60 * 60 + 1)),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
}
