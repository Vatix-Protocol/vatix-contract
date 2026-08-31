use crate::{ContractError, ResolutionContract, ResolutionContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
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
    use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String};

    #[contracttype]
    #[derive(Clone)]
    enum DataKey {
        Token,
        Status,
        /// Records the arguments of the most recent successful
        /// `resolve_market`/`resolve_market_v2` call so tests can assert the
        /// resolution contract invoked the market with the correct real
        /// signature (#701), not just that `finalize`/`arbitrate` returned Ok.
        LastResolved,
        V1Disabled,
    }

    #[contracttype]
    #[derive(Clone, Debug, PartialEq)]
    pub struct LastResolvedCall {
        pub resolver: Address,
        pub market_id: String,
        pub outcome: bool,
        pub is_v2: bool,
    }

    #[contract]
    pub struct MockMarket;

    #[contractimpl]
    impl MockMarket {
        pub fn init(env: Env, collateral_token: Address) {
            env.storage()
                .instance()
                .set(&DataKey::Token, &collateral_token);
            env.storage()
                .instance()
                .set(&DataKey::Status, &MarketStatus::Active);
        }

        pub fn get_collateral_token(env: Env, _market_id: u32) -> Address {
            env.storage().instance().get(&DataKey::Token).unwrap()
        }

        /// Accepts any signature except the reserved all-zero one, which
        /// tests use to simulate an oracle rejection. Mirrors the real
        /// market contract's V1 fail-closed gate (#701): rejects outright
        /// once V1 has been "disabled" via `set_v1_disabled`.
        pub fn verify_signature(
            env: Env,
            _market_id: u32,
            _outcome: bool,
            signature: BytesN<64>,
        ) -> Result<(), ContractError> {
            if env
                .storage()
                .instance()
                .get(&DataKey::V1Disabled)
                .unwrap_or(false)
            {
                return Err(ContractError::Unauthorized);
            }
            if signature == BytesN::from_array(&env, &[0u8; 64]) {
                return Err(ContractError::Unauthorized);
            }
            Ok(())
        }

        /// V2 counterpart of `verify_signature`. Same all-zero-signature
        /// rejection convention as V1.
        #[allow(clippy::too_many_arguments)]
        pub fn verify_signature_v2(
            env: Env,
            _passphrase_hash: BytesN<32>,
            _market_id: u32,
            _outcome: bool,
            _valid_until: u64,
            _epoch: u32,
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

        /// Real signature: `(resolver, market_id: String, outcome, signature, expires_at)`.
        pub fn resolve_market(
            env: Env,
            resolver: Address,
            market_id: String,
            outcome: bool,
            _signature: BytesN<64>,
            _expires_at: u64,
        ) {
            env.storage().instance().set(
                &DataKey::LastResolved,
                &LastResolvedCall {
                    resolver,
                    market_id,
                    outcome,
                    is_v2: false,
                },
            );
        }

        /// Real signature: `(resolver, market_id: String, outcome, valid_until, epoch, signature, passphrase_hash)`.
        #[allow(clippy::too_many_arguments)]
        pub fn resolve_market_v2(
            env: Env,
            resolver: Address,
            market_id: String,
            outcome: bool,
            _valid_until: u64,
            _epoch: u32,
            _signature: BytesN<64>,
            _passphrase_hash: BytesN<32>,
        ) {
            env.storage().instance().set(
                &DataKey::LastResolved,
                &LastResolvedCall {
                    resolver,
                    market_id,
                    outcome,
                    is_v2: true,
                },
            );
        }

        pub fn get_last_resolved(env: Env) -> Option<LastResolvedCall> {
            env.storage().instance().get(&DataKey::LastResolved)
        }

        pub fn void_market(env: Env, _caller: Address, _market_id: u32) {
            env.storage()
                .instance()
                .set(&DataKey::Status, &MarketStatus::Canceled);
        }

        pub fn set_status(env: Env, status: MarketStatus) {
            env.storage().instance().set(&DataKey::Status, &status);
        }

        /// Test hook: simulate the real market contract's V1 signatures
        /// being disabled (#701).
        pub fn set_v1_disabled(env: Env, disabled: bool) {
            env.storage()
                .instance()
                .set(&DataKey::V1Disabled, &disabled);
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
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

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
    fund(&env, &token, &proposer, 10_000_000i128);
    let candidate_id = client.propose(
        &proposer,
        &1,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + 60),
        &evidence(&env),
        &60,
        &10_000_000i128,
    );

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
    let candidate_id = client.propose(
        &proposer,
        &5,
        &true,
        &sig,
        &(env.ledger().timestamp() + 7_200),
        &evidence(&env),
        &60,
        &10_000_000i128,
    );

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

/// Regression for #723: `set_default_challenge_window` is an instant,
/// non-timelocked admin mutator by design (see its doc comment in `lib.rs`)
/// because it has no on-chain binding effect on any candidate, past or
/// future -- `propose`/`propose_v2`/`appeal` each take their own
/// independently-validated `challenge_window_seconds` argument and store it
/// directly on the candidate at creation time. This proves the "no
/// retroactive effect" half of that claim: shrinking the default after a
/// candidate is proposed must not move that candidate's already-computed
/// `challenge_deadline`.
#[test]
fn shrinking_default_does_not_move_existing_candidate_deadline() {
    let env = Env::default();
    let (client, admin, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let proposed_window = 10_000u64;
    let candidate_id = client.propose(
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + proposed_window + 3600),
        &evidence(&env),
        &proposed_window,
        &BOND,
    );
    let deadline_before = client.get_candidate(&candidate_id).unwrap().challenge_deadline;

    // Shrink the default to its minimum -- if this had any binding effect on
    // live candidates, the deadline read below would move.
    client.set_default_challenge_window(&admin, &60);

    let deadline_after = client.get_candidate(&candidate_id).unwrap().challenge_deadline;
    assert_eq!(deadline_before, deadline_after);
}

/// Regression for #723, other half: shrinking the default must not clamp or
/// otherwise constrain the window a *future* `propose` call may choose --
/// the default is advisory only, and the only enforced bounds are the fixed
/// `MIN_CHALLENGE_WINDOW_SECONDS`/`MAX_CHALLENGE_WINDOW_SECONDS` constants
/// applied to the caller's own argument.
#[test]
fn shrinking_default_does_not_constrain_a_new_proposals_chosen_window() {
    let env = Env::default();
    let (client, admin, _, token) = setup(&env);
    set_time(&env, 1_000);

    client.set_default_challenge_window(&admin, &60);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let chosen_window = 14 * 24 * 60 * 60u64; // MAX, far above the shrunk default
    let candidate_id = client.propose(
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + chosen_window + 3600),
        &evidence(&env),
        &chosen_window,
        &BOND,
    );

    let candidate = client.get_candidate(&candidate_id).unwrap();
    assert_eq!(
        candidate.challenge_deadline,
        candidate.proposed_at + chosen_window
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &MIN_WINDOW,
        &BOND,
    );
    assert!(
        result.is_ok(),
        "MIN window should be accepted, got: {:?}",
        result
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &MAX_WINDOW,
        &BOND,
    );
    assert!(
        result.is_ok(),
        "MAX window should be accepted, got: {:?}",
        result
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &(MIN_WINDOW - 1),
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &(MAX_WINDOW + 1),
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &MIN_WINDOW,
        &BOND,
    );
    assert!(
        result.is_ok(),
        "expiry == proposed_at should be accepted, got: {:?}",
        result
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &MIN_WINDOW,
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env),
        &window,
        &BOND,
    );
    // challenge_deadline == 1_000 + 300 == 1_300
    // Set time to exactly the deadline.
    set_time(&env, 1_300);

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    let uri = String::from_str(&env, "ipfs://at-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri, &BOND);
    assert!(
        result.is_ok(),
        "challenge at deadline boundary should be accepted (guard is >)"
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env),
        &window,
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env),
        &window,
        &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_299
    set_time(&env, 1_299);

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    let uri = String::from_str(&env, "ipfs://just-before-deadline");
    let result = client.try_challenge(&challenger, &candidate_id, &uri, &BOND);
    assert!(
        result.is_ok(),
        "challenge one second before deadline should be accepted"
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &(env.ledger().timestamp() + window + 3600),
        &evidence(&env),
        &window,
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
    );
    // challenge_deadline == 1_300; advance to 1_301
    set_time(&env, 1_301);

    let finalizer = Address::generate(&env);
    let result = client.try_finalize(&finalizer, &candidate_id);
    assert!(
        result.is_ok(),
        "finalize one second after deadline should succeed, got: {:?}",
        result
    );
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
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
        &proposer,
        &1u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
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
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = MIN_WINDOW;
    let expiry = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose(
        &proposer,
        &77u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
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
    let (client, _, _, token) = setup(&env);
    set_time(&env, 2_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = MIN_WINDOW;
    let expiry = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose(
        &proposer,
        &42u32,
        &false,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
    );

    set_time(&env, 2_000 + window + 1);

    let finalizer = Address::generate(&env);
    client.finalize(&finalizer, &candidate_id);

    // The candidate retrieved from storage must show Finalized status.
    let stored = client
        .get_candidate(&candidate_id)
        .expect("candidate must exist");
    assert_eq!(stored.status, crate::types::CandidateStatus::Finalized);
    assert!(stored.finalized_at.is_some(), "finalized_at must be set");
}

// ── Issue #701: fail closed without V1; resolution verifies V2 ─────────────

/// Once the market contract has disabled legacy V1 oracle signatures (the
/// default on a fresh deployment, #701), `propose`'s cross-contract
/// `verify_signature` call must fail closed instead of silently succeeding
/// and opening a challenge window for a candidate that `resolve_market`
/// could never actually finalize.
#[test]
fn propose_rejects_when_market_has_disabled_v1() {
    let env = Env::default();
    let (client, _, market_contract, token) = setup(&env);
    set_time(&env, 1_000);

    MockMarketClient::new(&env, &market_contract).set_v1_disabled(&true);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let err = client
        .try_propose(
            &proposer,
            &1u32,
            &true,
            &signature(&env),
            &(env.ledger().timestamp() + 60),
            &evidence(&env),
            &60,
            &BOND,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized);
    // No candidate should have been recorded for the market.
    assert!(client.get_candidate_id_for_market(&1u32).is_none());
}

/// `finalize` must invoke the market contract's `resolve_market` with the
/// real production signature — `(resolver, market_id: String, outcome,
/// signature, expires_at)` — not the stale 3-argument shape that predated
/// the `resolver`/`expires_at` parameters (#701). Verified by reading back
/// what the mock market actually received.
#[test]
fn finalize_invokes_resolve_market_with_real_signature() {
    let env = Env::default();
    let (client, _, market_contract, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = MIN_WINDOW;
    let expiry = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose(
        &proposer,
        &5u32,
        &true,
        &signature(&env),
        &expiry,
        &evidence(&env),
        &window,
        &BOND,
    );

    set_time(&env, 1_000 + window + 1);
    let finalizer = Address::generate(&env);
    client.finalize(&finalizer, &candidate_id);

    let last = MockMarketClient::new(&env, &market_contract)
        .get_last_resolved()
        .expect("resolve_market must have been invoked");
    assert_eq!(last.resolver, client.address);
    assert_eq!(last.market_id, String::from_str(&env, "5"));
    assert_eq!(last.outcome, true);
    assert!(!last.is_v2);
}

/// `propose_v2` + `finalize` end to end: verifies via the market's
/// `verify_signature_v2` and, on finalize, invokes `resolve_market_v2` (not
/// the legacy `resolve_market`) with the real production signature (#701).
#[test]
fn propose_v2_and_finalize_uses_resolve_market_v2() {
    let env = Env::default();
    let (client, _, market_contract, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = MIN_WINDOW;
    let valid_until = env.ledger().timestamp() + window + 7_200;
    let passphrase_hash = BytesN::from_array(&env, &[9u8; 32]);
    let candidate_id = client.propose_v2(
        &proposer,
        &9u32,
        &true,
        &signature(&env),
        &valid_until,
        &1u32,
        &passphrase_hash,
        &evidence(&env),
        &window,
        &BOND,
    );

    set_time(&env, 1_000 + window + 1);
    let finalizer = Address::generate(&env);
    let candidate = client.finalize(&finalizer, &candidate_id);
    assert_eq!(candidate.status, crate::types::CandidateStatus::Finalized);
    assert_eq!(candidate.epoch, 1u32);
    assert_eq!(candidate.passphrase_hash, Some(passphrase_hash));

    let last = MockMarketClient::new(&env, &market_contract)
        .get_last_resolved()
        .expect("resolve_market_v2 must have been invoked");
    assert_eq!(last.resolver, client.address);
    assert_eq!(last.market_id, String::from_str(&env, "9"));
    assert!(
        last.is_v2,
        "finalize must call resolve_market_v2 for a V2 candidate"
    );
}

/// `propose_v2` must reject the reserved all-zero signature the same way
/// `propose` does, proving the V2 verification call is real (not a no-op).
#[test]
fn propose_v2_rejects_invalid_signature() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let bad_sig = BytesN::from_array(&env, &[0u8; 64]);
    let err = client
        .try_propose_v2(
            &proposer,
            &2u32,
            &true,
            &bad_sig,
            &(env.ledger().timestamp() + 60),
            &1u32,
            &BytesN::from_array(&env, &[1u8; 32]),
            &evidence(&env),
            &60,
            &BOND,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized);
}

/// A candidate proposed via `propose_v2` must not be re-signable through the
/// V1-only `appeal` path — that would silently downgrade a V2-verified
/// candidate (still carrying its original `passphrase_hash`) to a
/// V1-verified signature that `finalize` would then route to
/// `resolve_market_v2` using stale V2 metadata (#701).
#[test]
fn appeal_rejects_v2_candidate() {
    let env = Env::default();
    let (client, _, _, token) = setup(&env);
    set_time(&env, 1_000);

    let proposer = Address::generate(&env);
    fund(&env, &token, &proposer, BOND);
    let window = MIN_WINDOW;
    let valid_until = env.ledger().timestamp() + window + 7_200;
    let candidate_id = client.propose_v2(
        &proposer,
        &11u32,
        &true,
        &signature(&env),
        &valid_until,
        &1u32,
        &BytesN::from_array(&env, &[3u8; 32]),
        &evidence(&env),
        &window,
        &BOND,
    );

    let challenger = Address::generate(&env);
    fund(&env, &token, &challenger, BOND);
    client.challenge(&challenger, &candidate_id, &evidence(&env), &BOND);

    let err = client
        .try_appeal(
            &proposer,
            &candidate_id,
            &true,
            &signature(&env),
            &evidence(&env),
            &window,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized);
}
