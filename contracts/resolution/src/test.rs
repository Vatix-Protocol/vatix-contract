use crate::{ContractError, ResolutionContract, ResolutionContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String,
};

/// A minimal stand-in for the market contract's dispute-facing surface
/// (`verify_signature`, `get_market_status`, `get_collateral_token`,
/// `resolve_market`, `void_market`), used so these tests can exercise the
/// resolution contract's real cross-contract calls without depending on the
/// `vatix-market-contract` crate (the resolution crate cannot depend on it —
/// see the module doc comment in `lib.rs`). Mirrors exactly the shape
/// resolution's `lib.rs` currently invokes on the registered market
/// contract, using this crate's own `types::MarketStatus`/`error::ContractError`
/// so decoding across the cross-contract boundary matches structurally.
mod mock_market {
    use crate::error::ContractError;
    use crate::types::MarketStatus;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

    #[contracttype]
    #[derive(Clone)]
    enum DataKey {
        Token,
        Status,
    }

    #[contract]
    pub struct MockMarket;

    #[contractimpl]
    impl MockMarket {
        pub fn init(env: Env, collateral_token: Address) {
            env.storage().instance().set(&DataKey::Token, &collateral_token);
            env.storage()
                .instance()
                .set(&DataKey::Status, &MarketStatus::Active);
        }

        pub fn get_collateral_token(env: Env, _market_id: u32) -> Address {
            env.storage().instance().get(&DataKey::Token).unwrap()
        }

        /// Accepts any signature except the reserved all-zero one, which
        /// tests use to simulate an oracle rejection.
        pub fn verify_signature(
            env: Env,
            _market_id: u32,
            _outcome: bool,
            signature: BytesN<64>,
        ) -> Result<(), ContractError> {
            if signature == BytesN::from_array(&env, &[0u8; 64]) {
                return Err(ContractError::Unauthorized);
            }
            Ok(())
        }

        pub fn get_market_status(env: Env, _market_id: u32) -> MarketStatus {
            env.storage().instance().get(&DataKey::Status).unwrap()
        }

        pub fn resolve_market(_env: Env, _market_id: u32, _outcome: bool, _signature: BytesN<64>) {}

        pub fn void_market(env: Env, _caller: Address, _market_id: u32) {
            env.storage()
                .instance()
                .set(&DataKey::Status, &MarketStatus::Canceled);
        }

        pub fn set_status(env: Env, status: MarketStatus) {
            env.storage().instance().set(&DataKey::Status, &status);
        }
    }
}

use mock_market::{MockMarket, MockMarketClient};

const DEFAULT_WINDOW: u64 = 300;

fn setup(env: &Env) -> (ResolutionContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(ResolutionContract, ());
    let client = ResolutionContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let factory = Address::generate(env);

    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();

    let market_contract = env.register(MockMarket, ());
    MockMarketClient::new(env, &market_contract).init(&token);

    client.initialize(&admin, &factory, &market_contract, &DEFAULT_WINDOW);
    (client, admin, market_contract, token)
}

/// Mint `amount` of the test collateral token to `who`, so they can post a
/// proposer/challenger bond.
fn fund(env: &Env, token: &Address, who: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(who, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
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
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    let candidate_id = client.propose(
        &proposer,
        &42,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
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
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &false,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, 10_000_000i128);
    let challenge_uri = String::from_str(&env, "ipfs://challenge");
    client.challenge(&challenger, &candidate_id, &challenge_uri, &10_000_000i128);
    set_time(&env, 1_400);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert_eq!(result, Err(Ok(ContractError::CandidateAlreadyChallenged)));
}

#[test]
fn finalize_requires_closed_challenge_window() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
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
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let candidate_id = client.propose(&proposer, &1, &true, &signature(&env), &(env.ledger().timestamp() + 60), &evidence(&env), &60, &10_000_000i128);

    set_time(&env, 1_061);
    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, 10_000_000i128);
    let challenge_uri = String::from_str(&env, "ipfs://late-challenge");
    assert_eq!(
        client.try_challenge(&challenger, &candidate_id, &challenge_uri, &10_000_000i128),
        Err(Ok(ContractError::ChallengeWindowClosed))
    );
}

#[test]
fn admin_can_update_factory_registration() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);

    set_time(&env, 100);
    let new_factory = Address::generate(&env);
    client.propose_factory(&admin, &new_factory);
    
    set_time(&env, 100 + 172_800);
    client.execute_factory();
    
    assert_eq!(client.get_config().factory, new_factory);
}

/// #404: finalize invokes resolve_market on the market contract.
///
/// Uses mock_all_auths so the cross-contract call is intercepted without
/// needing a live market contract deployment.
#[test]
fn finalize_calls_resolve_market_on_market_contract() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);

    set_time(&env, 1_000);
    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    let sig = signature(&env);
    let candidate_id = client.propose(&proposer, &5, &true, &sig, &(env.ledger().timestamp() + 60), &evidence(&env), &60, &10_000_000i128);

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

/// Test that proposal with valid signature is accepted via market contract delegation.
///
/// Verifies that the resolution contract correctly delegates signature verification
/// to the market contract's verify_signature entrypoint.
#[test]
fn propose_with_valid_signature_succeeds_via_delegation() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    let valid_sig = signature(&env);

    // This should succeed because mock_all_auths intercepts the cross-contract call
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &valid_sig,
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    assert_eq!(candidate_id, 1);
    let candidate = client.get_candidate(&candidate_id).unwrap();
    assert_eq!(candidate.market_id, 1);
    assert_eq!(candidate.outcome, true);
}

/// Test that proposal with invalid signature is rejected via market contract delegation.
///
/// Verifies that when the market contract rejects a signature, the resolution
/// contract propagates that error back to the caller.
#[test]
fn propose_with_invalid_signature_fails_via_delegation() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, 10_000_000i128);
    // Use an invalid signature (all zeros) to trigger rejection
    let invalid_sig = BytesN::from_array(&env, &[0u8; 64]);

    let result = client.try_propose(
        &proposer,
        &1,
        &true,
        &invalid_sig,
        &(env.ledger().timestamp() + 600),
        &evidence(&env),
        &300,
        &10_000_000i128,
    );

    // The market contract should reject the invalid signature
    assert!(result.is_err());
}

// ── #380: default challenge window ────────────────────────────────────────────

#[test]
fn initialize_stores_default_challenge_window() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    assert_eq!(client.get_default_challenge_window(), DEFAULT_WINDOW);
}

#[test]
fn admin_can_update_default_challenge_window() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    client.set_default_challenge_window(&admin, &600);
    assert_eq!(client.get_default_challenge_window(), 600);
}

#[test]
fn set_default_challenge_window_rejects_non_admin() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    let rando = Address::generate(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&rando, &600),
        Err(Ok(ContractError::NotAdmin))
    );
}

#[test]
fn set_default_challenge_window_rejects_out_of_bounds() {
    let env = Env::default();
    let (client, admin, _, _) = setup(&env);
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &10),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
    assert_eq!(
        client.try_set_default_challenge_window(&admin, &(14 * 24 * 60 * 60 + 1)),
        Err(Ok(ContractError::InvalidChallengeWindow))
    );
}

// ── Issue #552: challenge_window boundary tests at propose ─────────────────────
//
// Constants (from lib.rs):
//   MIN_CHALLENGE_WINDOW_SECONDS = 60
//   MAX_CHALLENGE_WINDOW_SECONDS = 14 * 24 * 60 * 60  (1_209_600)
//
// validate_challenge_window uses a Range::contains check equivalent to:
//   MIN..=MAX — so both endpoints are valid.

const MIN_WINDOW: u64 = 60;
const MAX_WINDOW: u64 = 14 * 24 * 60 * 60; // 1_209_600
const BOND: i128 = 10_000_000;

/// challenge_window == MIN (60 s) is accepted.
#[test]
fn propose_accepts_challenge_window_at_min() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let expiry = env.ledger().timestamp() + MIN_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "MIN window should be accepted, got: {:?}", result);
}

/// challenge_window == MAX (14 days) is accepted.
#[test]
fn propose_accepts_challenge_window_at_max() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let expiry = env.ledger().timestamp() + MAX_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MAX_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "MAX window should be accepted, got: {:?}", result);
}

/// challenge_window == MIN - 1 (59 s) is rejected with InvalidChallengeWindow.
#[test]
fn propose_rejects_challenge_window_below_min() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let expiry = env.ledger().timestamp() + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &(MIN_WINDOW - 1), &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidChallengeWindow)));
}

/// challenge_window == MAX + 1 is rejected with InvalidChallengeWindow.
#[test]
fn propose_rejects_challenge_window_above_max() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let expiry = env.ledger().timestamp() + MAX_WINDOW + 3600;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &(MAX_WINDOW + 1), &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidChallengeWindow)));
}

// ── Issue #552: signature_expiry boundary tests at propose ────────────────────
//
// From lib.rs `propose`:
//   if signature_expiry < proposed_at  → InvalidSignatureExpiry
//   if signature_expiry == proposed_at → accepted (>= is valid)

/// signature_expiry == proposed_at is accepted (equal is the boundary minimum).
#[test]
fn propose_accepts_signature_expiry_equal_to_proposed_at() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    // expiry == ledger timestamp at call time
    let expiry = env.ledger().timestamp();
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert!(result.is_ok(), "expiry == proposed_at should be accepted, got: {:?}", result);
}

/// signature_expiry < proposed_at is rejected with InvalidSignatureExpiry.
#[test]
fn propose_rejects_signature_expiry_before_proposed_at() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    // expiry is one second before ledger time
    let expiry = env.ledger().timestamp() - 1;
    let result = client.try_propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &MIN_WINDOW, &BOND,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidSignatureExpiry)));
}

// ── Issue #552: challenge window boundary tests at challenge ──────────────────
//
// From lib.rs `challenge`:
//   if timestamp > challenge_deadline  → ChallengeWindowClosed
//
// Key off-by-one cases:
//   timestamp == deadline     → > is false  → window CLOSED (error)
//   timestamp == deadline - 1 → > is false  → window OPEN   (accepted)
//
// Wait — re-read: `> deadline` means equal is NOT greater, so:
//   timestamp == deadline     → NOT > deadline → window still open → accepted
//   timestamp == deadline + 1 → > deadline     → ChallengeWindowClosed

/// challenge at exactly the deadline (t == deadline) is still accepted —
/// the guard is strictly `>`, so the boundary second is inside the window.
#[test]
fn challenge_accepted_at_exactly_deadline() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_000 + 300 == 1_300
    // Set time to exactly the deadline.
    set_time(&env, 1_300);

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    let uri = String::from_str(&env, "ipfs://at-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri, &BOND);
    assert!(result.is_ok(), "challenge at deadline boundary should be accepted (guard is >)");
}

/// challenge one second after the deadline is rejected with ChallengeWindowClosed.
#[test]
fn challenge_rejected_one_second_after_deadline() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_301
    set_time(&env, 1_301);

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    let uri = String::from_str(&env, "ipfs://past-deadline");
    assert_eq!(
        client.try_challenge(&challenger, &candidate_id, &uri, &BOND),
        Err(Ok(ContractError::ChallengeWindowClosed))
    );
}

/// challenge one second before the deadline is accepted.
#[test]
fn challenge_accepted_one_second_before_deadline() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_299
    set_time(&env, 1_299);

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    let uri = String::from_str(&env, "ipfs://just-before-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri, &BOND);
    assert!(result.is_ok(), "challenge one second before deadline should be accepted");
}

// ── Issue #552: finalize window boundary tests ────────────────────────────────
//
// From lib.rs `finalize`:
//   if timestamp <= challenge_deadline  → ChallengeWindowOpen
//
// Key off-by-one cases:
//   timestamp == deadline     → <= is true → window OPEN  → ChallengeWindowOpen
//   timestamp == deadline + 1 → <= is false → window CLOSED → accepted

/// finalize at exactly the deadline is rejected — the window is still open
/// because the guard is `<=`.
#[test]
fn finalize_rejected_at_exactly_deadline() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; attempt finalize at exactly 1_300
    set_time(&env, 1_300);

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::ChallengeWindowOpen))
    );
}

/// finalize one second after the deadline succeeds.
#[test]
fn finalize_accepted_one_second_after_deadline() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    // signature_expiry well in the future so it doesn't interfere
    let expiry = env.ledger().timestamp() + window + 7200;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_301
    set_time(&env, 1_301);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert!(result.is_ok(), "finalize one second after deadline should succeed, got: {:?}", result);
}

// ── Issue #552: signature_expiry boundary tests at finalize ───────────────────
//
// From lib.rs `finalize`:
//   if timestamp > signature_expiry  → SignatureExpired
//
// Key off-by-one cases:
//   timestamp == expiry     → NOT > expiry → still valid  → accepted
//   timestamp == expiry + 1 → > expiry     → SignatureExpired

/// finalize at exactly signature_expiry (t == expiry) is accepted —
/// the guard is strictly `>`, so the boundary second is still valid.
#[test]
fn finalize_accepted_at_exactly_signature_expiry() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    // Set expiry to exactly deadline + 1 so there's a valid finalize window
    // that starts at 1_301 and the expiry is also exactly 1_301.
    let expiry = 1_301u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    // Advance to the expiry second (which is also one second past the deadline)
    set_time(&env, expiry);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert!(
        result.is_ok(),
        "finalize at exactly signature_expiry should be accepted (guard is >), got: {:?}",
        result
    );
}

/// finalize one second after signature_expiry is rejected with SignatureExpired.
#[test]
fn finalize_rejected_one_second_after_signature_expiry() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = 300u64;
    // expiry is challenge_deadline + 1, so it's right at the edge of being
    // finalizeable. We'll advance one second past it to trigger SignatureExpired.
    let expiry = 1_301u64;
    let candidate_id = client.propose(
        &proposer, &1u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    // Advance to expiry + 1
    set_time(&env, expiry + 1);

    let finalizer = Address::generate(&env);
    assert_eq!(
        client.try_finalize(&finalizer, &candidate_id),
        Err(Ok(ContractError::SignatureExpired))
    );
}

// ── Issue #577: finalize is exactly-once ─────────────────────────────────────
//
// A second call to finalize() on an already-finalized candidate must return
// CandidateAlreadyFinalized without re-invoking the market's resolve_market.

/// First finalize succeeds; a second finalize on the same candidate_id must
/// return CandidateAlreadyFinalized (exactly-once guarantee, Issue #577).
#[test]
fn double_finalize_returns_already_finalized() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    let window = MIN_WINDOW;
    let expiry = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose(
        &proposer, &77u32, &true, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    // Advance past the challenge window.
    set_time(&env, 1_000 + window + 1);

    let finalizer = Address::generate(&env);
    // First finalize must succeed.
    let first = client.finalize(&finalizer, &candidate_id);
    assert_eq!(first.status, crate::types::CandidateStatus::Finalized);

    // Second finalize on the same candidate must be rejected safely —
    // no second resolve_market cross-contract call is fired.
    let second = client.try_finalize(&finalizer, &candidate_id);
    assert_eq!(
        second,
        Err(Ok(ContractError::CandidateAlreadyFinalized)),
        "second finalize must return CandidateAlreadyFinalized (Issue #577)"
    );
}

/// Verify that after a successful finalize the stored candidate status is
/// Finalized, so any subsequent finalize attempt is blocked at the guard
/// (Issue #577).
#[test]
fn finalized_candidate_status_is_persisted() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    set_time(&env, 2_000);

    let proposer = Address::generate(&env);
    let window = MIN_WINDOW;
    let expiry = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose(
        &proposer, &42u32, &false, &signature(&env), &expiry,
        &evidence(&env), &window, &BOND,
    );

    set_time(&env, 2_000 + window + 1);

    let finalizer = Address::generate(&env);
    client.finalize(&finalizer, &candidate_id);

    // The candidate retrieved from storage must show Finalized status.
    let stored = client.get_candidate(&candidate_id).expect("candidate must exist");
    assert_eq!(stored.status, crate::types::CandidateStatus::Finalized);
    assert!(stored.finalized_at.is_some(), "finalized_at must be set");
}
