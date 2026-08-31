//! Position / OutcomeToken dual-ledger reconciliation.
//!
//! `Position` (market storage) and `OutcomeToken` balances are two separate
//! ledgers that are supposed to always agree: every `update_position`
//! mint/burn is meant to move token balances in lockstep with share deltas,
//! and `settle_position` burns tokens back to zero on payout. They can still
//! diverge from a historical bug, a partial upgrade (e.g. the outcome-token
//! contract redeployed or re-pointed mid-market), or a manual admin
//! mint/burn issued directly on the outcome-token contract. Left unchecked,
//! over-minted tokens become extractable value and under-minted tokens brick
//! a user's exit.
//!
//! This module makes that divergence observable ([`get_position_token_parity`]),
//! blocks further trading/settlement for an affected user/market pair
//! ([`assert_position_token_parity`]), and provides an admin-gated repair
//! path ([`reconcile_position_tokens`]).
//!
//! ## Reconciliation policy
//!
//! `Position` in market storage is treated as the source of truth.
//! Reconciliation always mints/burns `OutcomeToken` balances to match
//! `Position`, never the reverse — `Position` also drives locked-collateral
//! and `total_deposited` accounting, neither of which can be safely
//! rederived from token balances alone.

use crate::error::ContractError;
use crate::events;
use crate::storage;
use crate::types::Position;
use soroban_sdk::{contracttype, Address, Env};
use vatix_outcome_token_contract::{types::TokenKind, OutcomeTokenContractClient};

/// Snapshot comparing a user's `Position` shares against their `OutcomeToken`
/// balances for the same market.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PositionTokenParity {
    pub yes_shares: i128,
    pub no_shares: i128,
    pub yes_token_balance: i128,
    pub no_token_balance: i128,
    pub is_matched: bool,
}

/// Read `(yes_balance, no_balance)` from the registered outcome-token
/// contract, or `None` if no outcome-token contract is registered.
fn load_token_balances(env: &Env, market_id: u32, user: &Address) -> Option<(i128, i128)> {
    let outcome_token_address = storage::get_outcome_token_contract(env)?;
    let token_client = OutcomeTokenContractClient::new(env, &outcome_token_address);
    Some((
        token_client.balance(&market_id, user, &TokenKind::Yes),
        token_client.balance(&market_id, user, &TokenKind::No),
    ))
}

/// Compare a user's stored `Position` against their `OutcomeToken` balances.
///
/// When no outcome-token contract is registered there is no second ledger to
/// diverge from, so parity is vacuously satisfied (token balances mirror
/// shares 1:1).
///
/// A **settled** position is likewise reported as matched regardless of the
/// raw balances (Issue #708 / #578 parity): every settle path
/// (`settle_position`, `batch_settle_positions`, `settle_positions_page`)
/// burns the position's outcome tokens back to zero on full exit while the
/// `Position` row is deliberately retained as a historical record — its
/// `yes_shares` / `no_shares` are *not* zeroed. Without this carve-out every
/// settled position would report `is_matched: false` forever, and
/// [`reconcile_position_tokens`] would "repair" it by **re-minting the
/// tokens that settlement just burned**.
pub fn get_position_token_parity(
    env: &Env,
    market_id: u32,
    user: &Address,
) -> Result<PositionTokenParity, ContractError> {
    let position = storage::get_position(env, market_id, user)?
        .unwrap_or_else(|| Position::new_empty(market_id, user.clone()));

    let (yes_token_balance, no_token_balance) = load_token_balances(env, market_id, user)
        .unwrap_or((position.yes_shares, position.no_shares));

    let is_matched = position.is_settled
        || (yes_token_balance == position.yes_shares
            && no_token_balance == position.no_shares);

    Ok(PositionTokenParity {
        yes_shares: position.yes_shares,
        no_shares: position.no_shares,
        yes_token_balance,
        no_token_balance,
        is_matched,
    })
}

/// Guard used by trading (`update_position`) and settlement entry points.
///
/// Rejects with [`ContractError::PositionTokenMismatch`] — after emitting a
/// [`events::PositionTokenMismatchDetected`] event — when the user's
/// `Position` shares and `OutcomeToken` balances have diverged for this
/// market. Callers must not silently re-sync on this path; the only way
/// forward is the admin-gated [`reconcile_position_tokens`].
pub fn assert_position_token_parity(
    env: &Env,
    market_id: u32,
    user: &Address,
) -> Result<(), ContractError> {
    let parity = get_position_token_parity(env, market_id, user)?;
    if !parity.is_matched {
        events::emit_position_token_mismatch_detected(
            env,
            market_id,
            user,
            parity.yes_shares,
            parity.no_shares,
            parity.yes_token_balance,
            parity.no_token_balance,
        );
        return Err(ContractError::PositionTokenMismatch);
    }
    Ok(())
}

/// Admin-gated repair: mint/burn `OutcomeToken` balances so they match the
/// user's stored `Position` for `market_id` (see module docs for policy).
/// Caller (`lib.rs::reconcile_position_tokens`) is responsible for verifying
/// `admin` authorization before calling this.
///
/// No-op if the two ledgers already agree (including when no outcome-token
/// contract is registered at all) — no event is emitted in that case since
/// no repair took place.
pub fn reconcile_position_tokens(
    env: &Env,
    admin: &Address,
    market_id: u32,
    user: &Address,
) -> Result<PositionTokenParity, ContractError> {
    let parity = get_position_token_parity(env, market_id, user)?;
    if parity.is_matched {
        return Ok(parity);
    }

    // A mismatch can only be reported when an outcome-token contract is
    // registered (see `get_position_token_parity` / `load_token_balances`).
    let outcome_token_address = storage::get_outcome_token_contract(env)
        .expect("mismatch implies an outcome-token contract is registered");
    let token_client = OutcomeTokenContractClient::new(env, &outcome_token_address);

    let yes_delta = parity.yes_shares - parity.yes_token_balance;
    let no_delta = parity.no_shares - parity.no_token_balance;

    if yes_delta > 0 {
        token_client.mint(&market_id, user, &TokenKind::Yes, &yes_delta);
    } else if yes_delta < 0 {
        token_client.burn(&market_id, user, &TokenKind::Yes, &(-yes_delta));
    }

    if no_delta > 0 {
        token_client.mint(&market_id, user, &TokenKind::No, &no_delta);
    } else if no_delta < 0 {
        token_client.burn(&market_id, user, &TokenKind::No, &(-no_delta));
    }

    events::emit_position_tokens_reconciled(
        env, market_id, user, admin, yes_delta, no_delta,
    );

    get_position_token_parity(env, market_id, user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use crate::{MarketContract, MarketContractClient};
    use ed25519_dalek::SigningKey;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::{token::StellarAssetClient, BytesN, String};
    use vatix_outcome_token_contract::{OutcomeTokenContract, OutcomeTokenContractClient};

    const STROOPS_PER_USDC: i128 = 10_000_000;

    #[test]
    fn test_parity_matches_when_no_outcome_token_contract_registered() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let user = <Address as TestAddress>::generate(&env);
        let market_id = 1;

        let parity = env.as_contract(&contract_id, || {
            storage::set_version(&env);
            get_position_token_parity(&env, market_id, &user).unwrap()
        });

        assert!(parity.is_matched);
        assert_eq!(parity.yes_shares, 0);
        assert_eq!(parity.no_shares, 0);
    }

    /// Issue #708 / #578 parity: once a position is settled, every settle path
    /// burns its outcome tokens back to zero while the `Position` row is kept
    /// as a historical record (shares not zeroed). The parity view must report
    /// such a position as matched — otherwise `reconcile_position_tokens`
    /// would re-mint the tokens settlement just burned.
    #[test]
    fn test_settled_position_reports_matched_even_though_tokens_burned() {
        let env = Env::default();
        env.mock_all_auths();

        let market_contract_id = env.register(MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let user = <Address as TestAddress>::generate(&env);
        let market_id = 1u32;

        let outcome_token_id = env.register(OutcomeTokenContract, ());
        OutcomeTokenContractClient::new(&env, &outcome_token_id).initialize(
            &admin,
            &market_contract_id,
            &String::from_str(&env, "Vatix Outcome Token"),
            &String::from_str(&env, "VOT"),
        );

        let parity = env.as_contract(&market_contract_id, || {
            storage::set_version(&env);
            storage::set_outcome_token_contract(&env, &outcome_token_id);

            // A fully-exited, settled position: 100 YES shares on record, but
            // the outcome-token balance is 0 (burned on settlement).
            let mut position = Position::new_empty(market_id, user.clone());
            position.yes_shares = 100 * STROOPS_PER_USDC;
            position.is_settled = true;
            storage::set_position(&env, market_id, &user, &position).unwrap();

            get_position_token_parity(&env, market_id, &user).unwrap()
        });

        assert!(
            parity.is_matched,
            "a settled position must report matched despite burned tokens"
        );
        assert_eq!(parity.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(parity.yes_token_balance, 0);

        // And the admin repair path is a no-op (does not re-mint).
        let repaired = env.as_contract(&market_contract_id, || {
            reconcile_position_tokens(&env, &admin, market_id, &user).unwrap()
        });
        assert!(repaired.is_matched);
        assert_eq!(repaired.yes_token_balance, 0);
    }

    /// Full setup: a Market contract wired to a real OutcomeToken contract, one
    /// user holding 100 YES shares bought the normal way (so the two ledgers
    /// start in parity), and the collateral SAC token needed to resolve/settle.
    ///
    /// Returns contract *addresses* rather than clients (clients borrow
    /// `&Env` with a lifetime that doesn't outlive this function) — each test
    /// reconstructs the client it needs via `XClient::new(&env, &address)`.
    fn setup_wired_market() -> (
        soroban_sdk::Env,
        soroban_sdk::Address, // market contract id
        soroban_sdk::Address, // outcome-token contract id
        soroban_sdk::Address, // admin
        soroban_sdk::Address, // user
        u32,                  // market_id
        SigningKey,            // oracle signing key (for resolve_market)
    ) {
        use rand::rngs::OsRng;

        let env = soroban_sdk::Env::default();
        env.mock_all_auths();

        let market_contract_id = env.register(MarketContract, ());
        let market_client = MarketContractClient::new(&env, &market_contract_id);

        let admin = Address::generate(&env);
        env.as_contract(&market_contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let outcome_token_id = env.register(OutcomeTokenContract, ());
        let outcome_token_client = OutcomeTokenContractClient::new(&env, &outcome_token_id);
        outcome_token_client.initialize(
            &admin,
            &market_contract_id,
            &String::from_str(&env, "Vatix Outcome Token"),
            &String::from_str(&env, "VOT"),
        );
        market_client.set_outcome_token_contract(&admin, &outcome_token_id);

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let sac = StellarAssetClient::new(&env, &collateral_token);

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let oracle_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = market_client.initialize_market(
            &admin,
            &String::from_str(&env, "Reconciliation test market?"),
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let user = Address::generate(&env);
        let deposit = 100 * STROOPS_PER_USDC;
        sac.mint(&user, &deposit);
        market_client.deposit_collateral(&user, &market_id, &deposit);
        market_client.update_position(&user, &market_id, &deposit, &0i128, &5_000i128);

        (
            env,
            market_contract_id,
            outcome_token_id,
            admin,
            user,
            market_id,
            signing_key,
        )
    }

    #[test]
    fn test_parity_matches_after_normal_trade() {
        let (env, market_contract_id, _outcome_token_id, _admin, user, market_id, _key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);

        let parity = market_client.get_position_token_parity(&market_id, &user);
        assert!(parity.is_matched);
        assert_eq!(parity.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(parity.yes_token_balance, 100 * STROOPS_PER_USDC);
    }

    /// Simulates a manual admin mint issued directly on the outcome-token
    /// contract (bypassing the Market contract entirely) — exactly the kind
    /// of historical-bug / partial-upgrade scenario this module guards
    /// against. This is the "direct storage write" divergence hook: instead
    /// of poking private storage, we call the outcome-token contract's own
    /// `mint` out-of-band, which is the realistic way an over-mint happens.
    #[test]
    fn test_mismatch_detected_after_out_of_band_mint() {
        let (env, market_contract_id, outcome_token_id, _admin, user, market_id, _key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);
        let outcome_token_client = OutcomeTokenContractClient::new(&env, &outcome_token_id);

        outcome_token_client.mint(
            &market_id,
            &user,
            &vatix_outcome_token_contract::types::TokenKind::Yes,
            &(10 * STROOPS_PER_USDC),
        );

        let parity = market_client.get_position_token_parity(&market_id, &user);
        assert!(!parity.is_matched);
        assert_eq!(parity.yes_shares, 100 * STROOPS_PER_USDC);
        assert_eq!(parity.yes_token_balance, 110 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_trading_blocked_after_mismatch() {
        let (env, market_contract_id, outcome_token_id, _admin, user, market_id, _key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);
        let outcome_token_client = OutcomeTokenContractClient::new(&env, &outcome_token_id);

        outcome_token_client.mint(
            &market_id,
            &user,
            &vatix_outcome_token_contract::types::TokenKind::Yes,
            &(10 * STROOPS_PER_USDC),
        );

        let result = market_client.try_update_position(
            &user,
            &market_id,
            &(1 * STROOPS_PER_USDC),
            &0i128,
            &5_000i128,
        );
        assert_eq!(
            result,
            Err(Ok(crate::error::ContractError::PositionTokenMismatch))
        );
    }

    #[test]
    fn test_settlement_blocked_after_mismatch_then_repaired() {
        use ed25519_dalek::Signer;

        let (env, market_contract_id, outcome_token_id, admin, user, market_id, signing_key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);
        let outcome_token_client = OutcomeTokenContractClient::new(&env, &outcome_token_id);

        // Force divergence via an out-of-band mint on the outcome-token
        // contract (simulating a historical bug / manual admin mint).
        outcome_token_client.mint(
            &market_id,
            &user,
            &vatix_outcome_token_contract::types::TokenKind::Yes,
            &(10 * STROOPS_PER_USDC),
        );

        // Resolve the market so settlement would otherwise be eligible.
        let message = crate::oracle::construct_oracle_message(&env, market_id, true);
        let sig_bytes = signing_key.sign(message.to_array().as_slice()).to_bytes();
        let signature = BytesN::from_array(&env, &sig_bytes);
        market_client.resolve_market(&admin, &String::from_str(&env, "1"), &true, &signature);

        // Settlement must be rejected while the ledgers disagree.
        let settle_result = market_client.try_settle_position(&user, &market_id);
        assert_eq!(
            settle_result,
            Err(Ok(crate::error::ContractError::PositionTokenMismatch))
        );

        // Admin repairs the divergence.
        let parity = market_client.reconcile_position_tokens(&admin, &market_id, &user);
        assert!(parity.is_matched);
        assert_eq!(parity.yes_token_balance, parity.yes_shares);

        // Settlement now succeeds.
        let payout = market_client.settle_position(&user, &market_id);
        assert_eq!(payout, 100 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_reconcile_is_noop_when_already_matched() {
        let (env, market_contract_id, _outcome_token_id, admin, user, market_id, _key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);

        let before = market_client.get_position_token_parity(&market_id, &user);
        assert!(before.is_matched);

        let after = market_client.reconcile_position_tokens(&admin, &market_id, &user);
        assert_eq!(after, before);
    }

    #[test]
    fn test_reconcile_rejects_non_admin() {
        let (env, market_contract_id, outcome_token_id, _admin, user, market_id, _key) =
            setup_wired_market();
        let market_client = MarketContractClient::new(&env, &market_contract_id);
        let outcome_token_client = OutcomeTokenContractClient::new(&env, &outcome_token_id);

        outcome_token_client.mint(
            &market_id,
            &user,
            &vatix_outcome_token_contract::types::TokenKind::Yes,
            &(10 * STROOPS_PER_USDC),
        );

        let stranger = Address::generate(&env);
        let result =
            market_client.try_reconcile_position_tokens(&stranger, &market_id, &user);
        assert_eq!(result, Err(Ok(crate::error::ContractError::NotAdmin)));
    }
}
