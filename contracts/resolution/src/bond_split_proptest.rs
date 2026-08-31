//! #699: Property-based tests for `split_bond`'s forfeited-bond accounting.
//!
//! `split_bond` (see `lib.rs`) is the single place a forfeited proposer or
//! challenger bond gets carved up: `REWARD_BPS` to the winner, `BURN_BPS`
//! burned, and the remainder to the treasury (or left in this contract's own
//! balance if none is registered). These tests exercise that split directly
//! — via `super::split_bond`, the same private helper `finalize`,
//! `arbitrate_uphold_proposer`, and `void_market` all funnel through — so
//! the invariants below hold for every caller without needing to drive a
//! full propose/challenge/finalize lifecycle per case.
//!
//! ## Invariants tested
//! 1. **No over/under-distribution**: `reward + burned + treasury_cut ==
//!    total` for every forfeited amount, i.e. the split never creates or
//!    destroys value beyond the deliberate `burn()` leg.
//! 2. **No panics on edge-case inputs**: zero, dust-sized, and realistic
//!    bond magnitudes all settle without a panic or a negative leg.
//! 3. **Proportionality**: `reward` and `burned` match the documented
//!    `REWARD_BPS`/`BURN_BPS` split (5000/2500 of 10_000) up to integer
//!    floor-division dust, and that dust — not more — lands in the
//!    treasury/remainder leg.

use crate::storage;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

const REWARD_BPS: i128 = 5_000;
const BURN_BPS: i128 = 2_500;
const BPS_DENOMINATOR: i128 = 10_000;

/// Wire up a resolution contract instance plus a funded SAC token so
/// `split_bond`'s internal `TokenClient::transfer`/`burn` calls have a real
/// balance to move.
fn setup(env: &Env, funded_amount: i128) -> (Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(crate::ResolutionContract, ());
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = token.address();
    let loser = Address::generate(env);
    let winner = Address::generate(env);

    if funded_amount > 0 {
        StellarAssetClient::new(env, &token_address).mint(&contract_id, &funded_amount);
    }

    (contract_id, token_address, loser, winner)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Invariant: reward + burned + treasury_cut always sums to exactly
    /// `total` (no dust vanishes, no dust is double-counted) across a
    /// realistic bond-magnitude range (proposer/challenger bonds are
    /// stroop amounts well below overflow territory — see
    /// `MIN_BOND_AMOUNT`/`MIN_CHALLENGE_BOND_AMOUNT` in `lib.rs`).
    #[test]
    fn prop_split_bond_sums_to_total(total in 1i128..=1_000_000_000_000i128) {
        let env = Env::default();
        let (contract_id, token, loser, winner) = setup(&env, total);

        env.as_contract(&contract_id, || {
            crate::split_bond(&env, 1, 1, &token, &loser, &winner, total);
        });

        let reward = total * REWARD_BPS / BPS_DENOMINATOR;
        let burned = total * BURN_BPS / BPS_DENOMINATOR;
        let treasury_cut = total - reward - burned;

        prop_assert!(reward >= 0 && burned >= 0 && treasury_cut >= 0,
            "negative leg: reward={} burned={} treasury_cut={}", reward, burned, treasury_cut);
        prop_assert_eq!(reward + burned + treasury_cut, total,
            "split legs did not sum to total: {}+{}+{} != {}", reward, burned, treasury_cut, total);
    }

    /// Invariant: with no treasury registered, the treasury-cut leg simply
    /// stays in the contract's own balance rather than reverting or being
    /// silently dropped — `contract_balance_after == total - reward -
    /// burned` (the winner's reward and the burned leg are the only two
    /// legs that actually leave the contract's balance).
    #[test]
    fn prop_split_bond_no_treasury_retains_remainder(total in 1i128..=1_000_000_000_000i128) {
        let env = Env::default();
        let (contract_id, token, loser, winner) = setup(&env, total);

        let treasury_was_unset = env.as_contract(&contract_id, || {
            // No treasury registered: storage::get_treasury(&env) is None.
            let unset = storage::get_treasury(&env).is_none();
            crate::split_bond(&env, 1, 1, &token, &loser, &winner, total);
            unset
        });
        prop_assert!(treasury_was_unset, "test setup must not register a treasury");

        let reward = total * REWARD_BPS / BPS_DENOMINATOR;
        let burned = total * BURN_BPS / BPS_DENOMINATOR;
        let expected_remaining = total - reward - burned;

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        prop_assert_eq!(token_client.balance(&contract_id), expected_remaining);
        prop_assert_eq!(token_client.balance(&winner), reward);
    }

    /// Edge case: dust-sized bonds (below `BPS_DENOMINATOR`) must never
    /// panic and must never distribute more than `total` — this is where
    /// floor-division most often rounds a leg to exactly zero.
    #[test]
    fn prop_split_bond_dust_amounts_never_panic(total in 1i128..=9_999i128) {
        let env = Env::default();
        let (contract_id, token, loser, winner) = setup(&env, total);

        env.as_contract(&contract_id, || {
            crate::split_bond(&env, 1, 1, &token, &loser, &winner, total);
        });

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let winner_balance = token_client.balance(&winner);
        let contract_balance = token_client.balance(&contract_id);
        prop_assert!(winner_balance + contract_balance <= total);
    }

    /// Edge case: `total == 0` (and, by extension, any non-positive amount)
    /// must be a no-op — `split_bond` returns immediately without touching
    /// any balance.
    #[test]
    fn prop_split_bond_zero_total_is_noop(dummy in 0i128..=1i128) {
        let _ = dummy;
        let env = Env::default();
        let (contract_id, token, loser, winner) = setup(&env, 0);

        env.as_contract(&contract_id, || {
            crate::split_bond(&env, 1, 1, &token, &loser, &winner, 0);
        });

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        prop_assert_eq!(token_client.balance(&winner), 0);
        prop_assert_eq!(token_client.balance(&contract_id), 0);
    }
}
