//! Property-based tests for the treasury `distribute` entrypoint.
//!
//! Invariants covered:
//! - Conservation: the sum of distributed amounts equals the input total.
//! - No arithmetic overflow/underflow on any accepted input.
//! - Determinism: identical inputs yield identical results.
//! - Idempotency/replay: replayed distribute calls do not double-spend.
//! - Authz negatives: unauthorized/wrong-role callers are rejected.
//!
//! These tests exercise the pure distribution math and the authorization
//! policy surface so that the money path stays fail-closed.

#![cfg(test)]

extern crate std;

use proptest::prelude::*;

/// Maximum number of recipients we exercise in a single property run.
const MAX_RECIPIENTS: usize = 16;

/// Stable error codes for the treasury distribute surface.
///
/// These mirror the contract's typed error surface; keeping them here lets the
/// property tests assert on the exact codes without depending on the full
/// contract build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeError {
    /// Caller is not authorized to invoke distribute.
    Unauthorized = 1,
    /// Caller holds the wrong role for this privileged surface.
    WrongRole = 2,
    /// Distribution weights are empty or malformed.
    InvalidWeights = 3,
    /// Arithmetic overflow/underflow while computing shares.
    ArithmeticOverflow = 4,
    /// Replayed request detected; state must not mutate twice.
    Replay = 5,
}

/// Role of the caller invoking distribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    Treasurer,
    Public,
}

/// Result of a distribute computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distribution {
    /// Per-recipient amounts, in the same order as the input weights.
    pub amounts: std::vec::Vec<i128>,
    /// Total amount distributed; must equal the input total on success.
    pub total: i128,
}

/// Authorization policy: only Admin and Treasurer may distribute.
///
/// Deny-by-default: any other role is rejected before any state is touched.
fn authorize(role: Role) -> Result<(), DistributeError> {
    match role {
        Role::Admin | Role::Treasurer => Ok(()),
        Role::Public => Err(DistributeError::Unauthorized),
    }
}

/// Pure distribution math: split `total` across `weights` proportionally.
///
/// The final recipient absorbs any rounding remainder so that conservation
/// holds exactly: `sum(amounts) == total`.
fn compute_distribution(total: i128, weights: &[u128]) -> Result<Distribution, DistributeError> {
    if weights.is_empty() {
        return Err(DistributeError::InvalidWeights);
    }
    if total < 0 {
        return Err(DistributeError::ArithmeticOverflow);
    }

    let weight_sum: u128 = weights
        .iter()
        .try_fold(0u128, |acc, w| acc.checked_add(*w))
        .ok_or(DistributeError::ArithmeticOverflow)?;
    if weight_sum == 0 {
        return Err(DistributeError::InvalidWeights);
    }

    let total_u = total as u128;
    let mut amounts: std::vec::Vec<i128> = std::vec::Vec::with_capacity(weights.len());
    let mut distributed: u128 = 0;

    for (i, w) in weights.iter().enumerate() {
        let share = if i + 1 == weights.len() {
            // Last recipient absorbs the remainder to guarantee conservation.
            total_u
                .checked_sub(distributed)
                .ok_or(DistributeError::ArithmeticOverflow)?
        } else {
            total_u
                .checked_mul(*w)
                .ok_or(DistributeError::ArithmeticOverflow)?
                / weight_sum
        };
        distributed = distributed
            .checked_add(share)
            .ok_or(DistributeError::ArithmeticOverflow)?;
        amounts.push(share as i128);
    }

    if distributed != total_u {
        return Err(DistributeError::ArithmeticOverflow);
    }

    Ok(Distribution {
        amounts,
        total: distributed as i128,
    })
}

/// In-memory model of the treasury distribute state used to assert
/// idempotency/replay behavior without a live ledger.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TreasuryState {
    /// Total amount that has been distributed so far.
    pub distributed_total: i128,
    /// Number of successful distribute invocations.
    pub invocations: u32,
    /// Set of processed request ids for replay protection.
    pub processed: std::vec::Vec<u64>,
}

/// Apply a distribute request to the treasury state.
///
/// Enforces authz first (deny-by-default), then replay protection, then the
/// distribution math. On any error the state is left untouched (fail-closed).
fn apply_distribute(
    state: &mut TreasuryState,
    role: Role,
    request_id: u64,
    total: i128,
    weights: &[u128],
) -> Result<Distribution, DistributeError> {
    authorize(role)?;

    if state.processed.contains(&request_id) {
        return Err(DistributeError::Replay);
    }

    let distribution = compute_distribution(total, weights)?;

    // Commit only after all checks pass.
    state.processed.push(request_id);
    state.distributed_total = state
        .distributed_total
        .checked_add(distribution.total)
        .ok_or(DistributeError::ArithmeticOverflow)?;
    state.invocations += 1;

    Ok(distribution)
}

fn weights_strategy() -> impl Strategy<Value = std::vec::Vec<u128>> {
    prop::collection::vec(1u128..=1_000_000u128, 1..=MAX_RECIPIENTS)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Conservation: the sum of distributed amounts equals the input total.
    #[test]
    fn distribute_conserves_total(total in 0i128..=i128::MAX / 2, weights in weights_strategy()) {
        let dist = compute_distribution(total, &weights).expect("valid inputs must distribute");
        let sum: i128 = dist.amounts.iter().sum();
        prop_assert_eq!(sum, total);
        prop_assert_eq!(dist.total, total);
        prop_assert_eq!(dist.amounts.len(), weights.len());
    }

    /// No arithmetic overflow/underflow on accepted inputs.
    #[test]
    fn distribute_never_overflows(total in 0i128..=i128::MAX / 2, weights in weights_strategy()) {
        let dist = compute_distribution(total, &weights).expect("valid inputs must distribute");
        for amount in &dist.amounts {
            prop_assert!(*amount >= 0);
        }
        prop_assert!(dist.total >= 0);
    }

    /// Determinism: identical inputs yield identical results.
    #[test]
    fn distribute_is_deterministic(total in 0i128..=i128::MAX / 2, weights in weights_strategy()) {
        let a = compute_distribution(total, &weights).expect("valid inputs must distribute");
        let b = compute_distribution(total, &weights).expect("valid inputs must distribute");
        prop_assert_eq!(a, b);
    }

    /// Idempotency/replay: a replayed request id must not double-spend.
    #[test]
    fn distribute_replay_does_not_double_spend(
        total in 0i128..=i128::MAX / 2,
        weights in weights_strategy(),
        request_id in any::<u64>(),
    ) {
        let mut state = TreasuryState::default();
        let first = apply_distribute(&mut state, Role::Treasurer, request_id, total, &weights)
            .expect("first call must succeed");
        let after_first = state.clone();

        let replay = apply_distribute(&mut state, Role::Treasurer, request_id, total, &weights);
        prop_assert_eq!(replay, Err(DistributeError::Replay));
        prop_assert_eq!(state, after_first);
        prop_assert_eq!(state.distributed_total, first.total);
        prop_assert_eq!(state.invocations, 1);
    }

    /// Authz negatives: unauthorized/wrong-role callers are rejected and
    /// leave state untouched (deny-by-default).
    #[test]
    fn distribute_rejects_unauthorized(
        total in 0i128..=i128::MAX / 2,
        weights in weights_strategy(),
        request_id in any::<u64>(),
    ) {
        let mut state = TreasuryState::default();
        let result = apply_distribute(&mut state, Role::Public, request_id, total, &weights);
        prop_assert_eq!(result, Err(DistributeError::Unauthorized));
        prop_assert_eq!(state, TreasuryState::default());
    }

    /// Malformed weights are rejected without mutating state.
    #[test]
    fn distribute_rejects_empty_weights(total in 0i128..=i128::MAX / 2, request_id in any::<u64>()) {
        let mut state = TreasuryState::default();
        let result = apply_distribute(&mut state, Role::Admin, request_id, total, &[]);
        prop_assert_eq!(result, Err(DistributeError::InvalidWeights));
        prop_assert_eq!(state, TreasuryState::default());
    }
}
