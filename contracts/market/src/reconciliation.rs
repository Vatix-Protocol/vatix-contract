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
//!
//! ## Idempotency & replay safety
//!
//! Every repair is keyed by a caller-supplied `correlation_id` and recorded
//! in [`storage`] via [`storage::get_reconciliation_record`] /
//! [`storage::set_reconciliation_record`]. A replayed or concurrent request
//! carrying an already-seen `correlation_id` is rejected with
//! [`ContractError::ReconciliationAlreadyApplied`] *before* any token
//! mint/burn is attempted, so a retried admin call can never double-apply a
//! repair. Because the repair is also a pure function of the current
//! `Position` (the source of truth), a fresh `correlation_id` against an
//! already-consistent pair is a no-op that emits no event.

use crate::error::ContractError;
use crate::events;
use crate::storage;
use crate::types::Position;
use soroban_sdk::{contracttype, Address, BytesN, Env};
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
/// `correlation_id` is a caller-supplied idempotency key. It is recorded
/// before any token movement; a replayed or concurrent request carrying an
/// already-recorded id is rejected with
/// [`ContractError::ReconciliationAlreadyApplied`] without touching token
/// balances. This makes the repair safe to retry after an RPC/DB outage.
///
/// No-op if the two ledgers already agree (including when no outcome-token
/// contract is registered at all) — no event is emitted in that case since
/// no repair took place, and the `correlation_id` is *not* consumed so a
/// later genuine repair may reuse it.
pub fn reconcile_position_tokens(
    env: &Env,
    admin: &Address,
    market_id: u32,
    user: &Address,
    correlation_id: &BytesN<32>,
) -> Result<PositionTokenParity, ContractError> {
    let parity = get_position_token_parity(env, market_id, user)?;
    if parity.is_matched {
        return Ok(parity);
    }

    // Idempotency: reject replays/concurrent duplicates before any mint/burn.
    if storage::get_reconciliation_record(env, correlation_id).is_some() {
        return Err(ContractError::ReconciliationAlreadyApplied);
    }
    storage::set_reconciliation_record(env, correlation_id, market_id, user);

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

    /// Issue #708 / #578 parity: once a position is settled, every settle
    /// path burns the outcome tokens back to zero while the `Position` row is
    /// retained as a historical record. Parity must report such a position as
    /// matched so reconciliation never re-mints the burned tokens.
    #[test]
    fn test_parity_matches_for_settled_position() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let user = <Address as TestAddress>::generate(&env);
        let market_id = 1;

        let parity = env.as_contract(&contract_id, || {
            storage::set_version(&env);
            let mut position = Position::new_empty(market_id, &user);
            position.yes_shares = 100;
            position.no_shares = 50;
            position.is_settled = true;
            storage::set_position(&env, market_id, &user, &position);
            get_position_token_parity(&env, market_id, &user).unwrap()
        });

        assert!(parity.is_matched);
        assert_eq!(parity.yes_shares, 100);
        assert_eq!(parity.no_shares, 50);
    }

    /// A replayed reconciliation request (same `correlation_id`) must be
    /// rejected with `ReconciliationAlreadyApplied` and must not move tokens.
    #[test]
    fn test_reconcile_rejects_replayed_correlation_id() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let user = <Address as TestAddress>::generate(&env);
        let market_id = 1;
        let correlation_id = BytesN::from_array(&env, &[7u8; 32]);

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_reconciliation_record(&env, &correlation_id, market_id, &user);
            let err = reconcile_position_tokens(
                &env,
                &admin,
                market_id,
                &user,
                &correlation_id,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::ReconciliationAlreadyApplied);
        });
    }
}
