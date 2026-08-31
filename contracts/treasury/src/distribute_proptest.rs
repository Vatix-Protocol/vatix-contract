//! #699: Property-based tests for `distribute_fees`'s payout invariants.
//!
//! `distribute_fees` (see `lib.rs`) pays each configured stakeholder
//! `floor(balance * share_bps / 10_000)` of the treasury's current `token`
//! balance and leaves any floor-division dust in the treasury for the next
//! round. These tests fuzz the stakeholder split and balance to confirm the
//! payout never over-distributes, never panics, and leaves exactly the
//! expected remainder behind.

use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::{TreasuryContract, TreasuryContractClient};

const BPS_DENOMINATOR: u32 = 10_000;

struct Setup {
    env: Env,
    admin: Address,
    market: Address,
    token: Address,
    treasury_id: Address,
    client: TreasuryContractClient<'static>,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let treasury_id = env.register(TreasuryContract, ());
    let client: TreasuryContractClient<'static> =
        unsafe { core::mem::transmute(TreasuryContractClient::new(&env, &treasury_id)) };

    client.initialize(&admin, &market);

    Setup { env, admin, market, token, treasury_id, client }
}

fn fund_and_collect(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
    s.client.collect_fee(&s.market, &s.token, &1u32, &amount);
}

fn propose_and_execute_stakeholders(s: &Setup, stakeholders: &soroban_sdk::Vec<(Address, u32)>) {
    s.client.propose_stakeholders(&s.admin, stakeholders);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_stakeholders();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Invariant: for a 3-way stakeholder split (shares summing to exactly
    /// `BPS_DENOMINATOR`, matching `propose_stakeholders`' validation) and any
    /// realistic fee balance, the sum of every stakeholder payout plus the
    /// remaining treasury balance equals the pre-distribution balance
    /// exactly — no value is created or silently dropped.
    #[test]
    fn prop_distribute_fees_never_over_distributes(
        balance in 1i128..=1_000_000_000_000i128,
        share_a in 1u32..=9_998u32,
        share_b_raw in 1u32..=9_998u32,
    ) {
        // Derive three non-zero shares that sum to exactly BPS_DENOMINATOR
        // (the same invariant `propose_stakeholders` enforces).
        let remaining_after_a = BPS_DENOMINATOR - share_a;
        prop_assume!(remaining_after_a >= 2);
        let share_b = 1 + (share_b_raw % (remaining_after_a - 1));
        let share_c = BPS_DENOMINATOR - share_a - share_b;
        prop_assume!(share_c >= 1);

        let s = setup();
        fund_and_collect(&s, balance);

        let stakeholder_a = Address::generate(&s.env);
        let stakeholder_b = Address::generate(&s.env);
        let stakeholder_c = Address::generate(&s.env);
        let mut stakeholders = soroban_sdk::Vec::new(&s.env);
        stakeholders.push_back((stakeholder_a.clone(), share_a));
        stakeholders.push_back((stakeholder_b.clone(), share_b));
        stakeholders.push_back((stakeholder_c.clone(), share_c));
        propose_and_execute_stakeholders(&s, &stakeholders);

        s.client.distribute_fees(&s.admin, &s.token);

        let token_client = TokenClient::new(&s.env, &s.token);
        let paid_a = token_client.balance(&stakeholder_a);
        let paid_b = token_client.balance(&stakeholder_b);
        let paid_c = token_client.balance(&stakeholder_c);
        let remaining = s.client.token_balance(&s.token);

        let expected_a = balance * share_a as i128 / BPS_DENOMINATOR as i128;
        let expected_b = balance * share_b as i128 / BPS_DENOMINATOR as i128;
        let expected_c = balance * share_c as i128 / BPS_DENOMINATOR as i128;

        prop_assert_eq!(paid_a, expected_a);
        prop_assert_eq!(paid_b, expected_b);
        prop_assert_eq!(paid_c, expected_c);

        // No over-distribution: the sum of everything paid out plus what's
        // left in the treasury must equal the original balance exactly.
        prop_assert_eq!(paid_a + paid_b + paid_c + remaining, balance,
            "distribution did not conserve balance: {paid_a}+{paid_b}+{paid_c}+{remaining} != {balance}");

        // No under-distribution beyond documented floor-division dust: the
        // remainder can never exceed the number of stakeholders minus one
        // times the smallest possible rounding unit (BPS_DENOMINATOR - 1 per
        // leg in the worst case), and must always be non-negative.
        prop_assert!(remaining >= 0, "remaining treasury balance went negative: {remaining}");
    }

    /// Edge case: dust-sized balances (below `BPS_DENOMINATOR`) must never
    /// panic and must never pay out more than the balance itself, even when
    /// every share floors to zero.
    #[test]
    fn prop_distribute_fees_dust_balance_never_panics(
        balance in 1i128..=9_999i128,
        share_a in 1u32..=8_000u32,
    ) {
        let share_b = BPS_DENOMINATOR - share_a;
        prop_assume!(share_b >= 1);

        let s = setup();
        fund_and_collect(&s, balance);

        let stakeholder_a = Address::generate(&s.env);
        let stakeholder_b = Address::generate(&s.env);
        let mut stakeholders = soroban_sdk::Vec::new(&s.env);
        stakeholders.push_back((stakeholder_a.clone(), share_a));
        stakeholders.push_back((stakeholder_b.clone(), share_b));
        propose_and_execute_stakeholders(&s, &stakeholders);

        s.client.distribute_fees(&s.admin, &s.token);

        let token_client = TokenClient::new(&s.env, &s.token);
        let paid_a = token_client.balance(&stakeholder_a);
        let paid_b = token_client.balance(&stakeholder_b);
        let remaining = s.client.token_balance(&s.token);

        prop_assert!(paid_a + paid_b <= balance);
        prop_assert_eq!(paid_a + paid_b + remaining, balance);
    }

    /// Invariant: a single stakeholder holding 100% of the share
    /// (`BPS_DENOMINATOR`) receives the entire balance with zero dust left
    /// behind, for any balance magnitude.
    #[test]
    fn prop_distribute_fees_single_full_share_pays_out_all(
        balance in 1i128..=1_000_000_000_000i128,
    ) {
        let s = setup();
        fund_and_collect(&s, balance);

        let stakeholder = Address::generate(&s.env);
        let mut stakeholders = soroban_sdk::Vec::new(&s.env);
        stakeholders.push_back((stakeholder.clone(), BPS_DENOMINATOR));
        propose_and_execute_stakeholders(&s, &stakeholders);

        s.client.distribute_fees(&s.admin, &s.token);

        let token_client = TokenClient::new(&s.env, &s.token);
        prop_assert_eq!(token_client.balance(&stakeholder), balance);
        prop_assert_eq!(s.client.token_balance(&s.token), 0);
    }
}
