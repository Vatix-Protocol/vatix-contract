#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::{storage, TreasuryContract, TreasuryContractClient, TreasuryError};

// ── helpers ───────────────────────────────────────────────────────────────────

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

fn fund_treasury(s: &Setup, amount: i128) {
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn initialize_stores_admin_and_market() {
    let s = setup();
    assert_eq!(s.client.admin(), s.admin);
    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 0);
    assert!(s.client.is_authorized_market(&s.market));
}

#[test]
fn initialize_writes_storage_version() {
    let s = setup();
    s.env.as_contract(&s.treasury_id, || {
        assert_eq!(
            storage::get_version(&s.env),
            Some(storage::STORAGE_VERSION),
        );
    });
}

#[test]
fn storage_version_absent_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    env.as_contract(&id, || {
        assert_eq!(storage::get_version(&env), None);
    });
}

#[test]
fn initialize_can_only_be_called_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    let err = s
        .client
        .try_initialize(&other, &s.market)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::AlreadyInitialized);
}

#[test]
fn admin_panics_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    assert!(client.try_admin().is_err());
}

// ── collect_fee ───────────────────────────────────────────────────────────────

#[test]
fn collect_fee_updates_balance_and_cumulative() {
    let s = setup();
    s.client.collect_fee(&s.market, &s.token, &1u32, &50_000i128);
    assert_eq!(s.client.token_balance(&s.token), 50_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 50_000);
}

#[test]
fn collect_fee_accumulates_across_calls() {
    let s = setup();
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);
    s.client.collect_fee(&s.market, &s.token, &2u32, &200_000i128);
    assert_eq!(s.client.token_balance(&s.token), 300_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300_000);
}

#[test]
fn collect_fee_rejects_unauthorized_caller() {
    let s = setup();
    let rogue = Address::generate(&s.env);
    let err = s
        .client
        .try_collect_fee(&rogue, &s.token, &1u32, &50_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
}

#[test]
fn collect_fee_rejects_zero_amount() {
    let s = setup();
    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &0i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_rejects_negative_amount() {
    let s = setup();
    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &(-1i128))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidAmount);
}

#[test]
fn collect_fee_errors_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    let market = Address::generate(&env);
    let token = Address::generate(&env);
    let err = client
        .try_collect_fee(&market, &token, &1u32, &1_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── withdraw_fees ─────────────────────────────────────────────────────────────

#[test]
fn withdraw_fees_transfers_to_recipient() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &200_000i128);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&recipient), 200_000);
    assert_eq!(s.client.token_balance(&s.token), 300_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 500_000);
}

#[test]
fn withdraw_fees_rejects_non_admin() {
    let s = setup();
    let imposter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&imposter, &s.token, &recipient, &100_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn withdraw_fees_rejects_insufficient_balance() {
    let s = setup();
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &s.admin, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InsufficientBalance);
}

#[test]
fn withdraw_fees_errors_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let err = client
        .try_withdraw_fees(&admin, &token, &admin, &1_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── #384: admin withdraw accumulated fees ─────────────────────────────────────

/// Admin can withdraw ALL accumulated fees, draining the per-token balance
/// while preserving the cumulative counter.
#[test]
fn admin_withdraws_accumulated_fees_in_full() {
    let s = setup();
    let total_collected = 1_000_000i128;
    fund_treasury(&s, total_collected);
    s.client.collect_fee(&s.market, &s.token, &1u32, &total_collected);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &total_collected);

    assert_eq!(
        s.client.token_balance(&s.token),
        0,
        "balance fully drained after withdrawing all accumulated fees"
    );
    assert_eq!(
        s.client.get_cumulative_fees(&s.token),
        total_collected,
        "cumulative fees counter remains unchanged after withdrawal"
    );
    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&recipient),
        total_collected,
        "recipient received the full accumulated fee amount"
    );
}

/// Admin can withdraw a PARTIAL amount of accumulated fees.
#[test]
fn admin_withdraws_partial_accumulated_fees() {
    let s = setup();
    let total_collected = 500_000i128;
    fund_treasury(&s, total_collected);
    s.client.collect_fee(&s.market, &s.token, &1u32, &total_collected);

    let partial = 200_000i128;
    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &partial);

    assert_eq!(
        s.client.token_balance(&s.token),
        total_collected - partial,
        "remaining balance reflects partial withdrawal"
    );
    assert_eq!(
        s.client.get_cumulative_fees(&s.token),
        total_collected,
        "cumulative fees counter is monotone and unchanged"
    );
    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&recipient),
        partial,
        "recipient received the partial amount"
    );
}

/// Withdrawing fees from an uninitialized treasury is rejected.
#[test]
fn withdraw_fees_before_initialize_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let err = client
        .try_withdraw_fees(&admin, &token, &admin, &1_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NotInitialized);
}

// ── cumulative stays monotone ─────────────────────────────────────────────────

#[test]
fn cumulative_stays_high_after_withdrawal() {
    let s = setup();
    fund_treasury(&s, 300_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &300_000i128);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &300_000i128);

    assert_eq!(s.client.token_balance(&s.token), 0);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300_000);
}

// ── storage version guard (#307 / #308) ──────────────────────────────────────
// initialize_writes_storage_version / storage_version_absent_before_initialize
// are defined earlier in this file — see above.

#[test]
fn reads_return_upgrade_required_on_stale_version() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let token = Address::generate(&env);
    let client = TreasuryContractClient::new(&env, &id);

    // Write a stale version to simulate an old deployment that hasn't migrated.
    env.as_contract(&id, || {
        env.storage()
            .instance()
            .set(&storage::StorageKey::StorageVersion, &0u32);
    });

    let err = client.try_token_balance(&token).unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::UpgradeRequired);
}

#[test]
fn reads_return_upgrade_required_when_no_version_set() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TreasuryContract, ());
    let token = Address::generate(&env);
    let client = TreasuryContractClient::new(&env, &id);

    // No version written at all — simulates a freshly deployed but uninitialized contract.
    let err = client.try_token_balance(&token).unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::UpgradeRequired);
}

// ── set_market_contract ───────────────────────────────────────────────────────

#[test]
fn add_market_is_idempotent() {
    let s = setup();
    s.client.add_market(&s.admin, &s.market);
    assert_eq!(s.client.list_markets().len(), 1);
}

/// Issue #585: `list_markets` is the read path over the `AuthorizedMarkets`
/// registry — exercise it through a full add/remove/re-add sequence and
/// assert both contents *and* order at each step, not just the length.
#[test]
fn list_markets_reflects_order_and_contents_after_add_remove() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    let market3 = Address::generate(&s.env);

    // Freshly initialized: only the market registered by `initialize`.
    let markets = s.client.list_markets();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets.get(0).unwrap(), s.market);

    // Appends preserve insertion order.
    s.client.add_market(&s.admin, &market2);
    s.client.add_market(&s.admin, &market3);
    let markets = s.client.list_markets();
    assert_eq!(markets.len(), 3);
    assert_eq!(markets.get(0).unwrap(), s.market);
    assert_eq!(markets.get(1).unwrap(), market2);
    assert_eq!(markets.get(2).unwrap(), market3);

    // Removing a middle entry preserves the relative order of the rest.
    s.client.remove_market(&s.admin, &market2);
    let markets = s.client.list_markets();
    assert_eq!(markets.len(), 2);
    assert_eq!(markets.get(0).unwrap(), s.market);
    assert_eq!(markets.get(1).unwrap(), market3);
    assert!(!markets.contains(&market2));

    // Re-adding appends at the end rather than restoring the original slot.
    s.client.add_market(&s.admin, &market2);
    let markets = s.client.list_markets();
    assert_eq!(markets.len(), 3);
    assert_eq!(markets.get(0).unwrap(), s.market);
    assert_eq!(markets.get(1).unwrap(), market3);
    assert_eq!(markets.get(2).unwrap(), market2);
}

#[test]
fn add_market_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let market2 = Address::generate(&s.env);
    let err = s
        .client
        .try_add_market(&rando, &market2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn remove_market_deregisters_market() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);
    s.client.remove_market(&s.admin, &market2);

    assert!(!s.client.is_authorized_market(&market2));
    assert_eq!(s.client.list_markets().len(), 1);
}

#[test]
fn remove_market_is_idempotent_for_unknown_market() {
    let s = setup();
    let unknown = Address::generate(&s.env);

    assert_eq!(s.client.try_remove_market(&s.admin, &unknown), Ok(Ok(())));

    let markets = s.client.list_markets();
    assert_eq!(markets.len(), 1);
    assert!(markets.contains(&s.market));
    assert!(s.client.is_authorized_market(&s.market));
}

#[test]
fn removed_market_cannot_collect_fee() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);
    s.client.remove_market(&s.admin, &market2);

    let err = s
        .client
        .try_collect_fee(&market2, &s.token, &1u32, &100i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
}

#[test]
fn re_added_market_can_collect_fee_again() {
    // Round-trip: a market removed from the registry and later re-added must
    // regain collect_fee access exactly like any other authorized market —
    // removal must not leave stale state that blocks re-registration.
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);
    s.client.remove_market(&s.admin, &market2);
    assert!(!s.client.is_authorized_market(&market2));

    s.client.add_market(&s.admin, &market2);
    assert!(s.client.is_authorized_market(&market2));

    s.client.collect_fee(&market2, &s.token, &7u32, &42_000i128);
    assert_eq!(s.client.token_balance(&s.token), 42_000i128);
}

#[test]
fn collect_fee_rejects_caller_never_registered() {
    // A caller that was never part of the authorized-markets registry (as
    // opposed to one that was registered and later removed) must be
    // rejected the same way, via the same error.
    let s = setup();
    let never_registered = Address::generate(&s.env);
    let err = s
        .client
        .try_collect_fee(&never_registered, &s.token, &1u32, &10i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
    assert_eq!(s.client.token_balance(&s.token), 0);
}

#[test]
fn multiple_markets_can_each_collect_fees() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);

    s.client.collect_fee(&s.market, &s.token, &1u32, &100i128);
    s.client.collect_fee(&market2, &s.token, &2u32, &200i128);

    assert_eq!(s.client.token_balance(&s.token), 300);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300);
}

// ── propose_market_contract / execute_market_contract (timelocked, #720) ──────

#[test]
fn execute_market_contract_preserves_previously_added_markets() {
    // Regression for #720: `execute_market_contract` used to overwrite the
    // whole `AuthorizedMarkets` registry with a single-element vec, silently
    // deregistering every market added via `add_market`. It must now append,
    // just like `add_market` does, so the two entrypoints agree on one
    // registry instead of drifting apart.
    let s = setup();
    let market2 = Address::generate(&s.env);
    let market3 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);

    s.client.propose_market_contract(&s.admin, &market3);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_market_contract();

    assert!(s.client.is_authorized_market(&s.market));
    assert!(s.client.is_authorized_market(&market2));
    assert!(s.client.is_authorized_market(&market3));
    assert_eq!(s.client.list_markets().len(), 3);

    // The originally-registered market must still be able to collect fees —
    // this is the concrete symptom the drift caused: a live market silently
    // losing `collect_fee` access with no `remove_market` call or event.
    s.client.collect_fee(&s.market, &s.token, &1u32, &1_000i128);
    assert_eq!(s.client.token_balance(&s.token), 1_000i128);
}

#[test]
fn execute_market_contract_is_idempotent_for_already_authorized_market() {
    let s = setup();
    s.client.propose_market_contract(&s.admin, &s.market);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_market_contract();

    assert_eq!(s.client.list_markets().len(), 1);
    assert!(s.client.is_authorized_market(&s.market));
}

#[test]
fn execute_market_contract_rejects_before_timelock_elapses() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.propose_market_contract(&s.admin, &market2);

    let err = s.client.try_execute_market_contract().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
    assert!(!s.client.is_authorized_market(&market2));
}

#[test]
fn propose_market_contract_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let market2 = Address::generate(&s.env);
    let err = s
        .client
        .try_propose_market_contract(&rando, &market2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn cancel_market_contract_clears_pending_change() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.propose_market_contract(&s.admin, &market2);
    s.client.cancel_market_contract(&s.admin);

    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    let err = s.client.try_execute_market_contract().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
    assert!(!s.client.is_authorized_market(&market2));
}

// ── propose_admin / execute_admin (timelocked, #658) ───────────────────────────

#[test]
fn transfer_admin_updates_admin() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.propose_admin(&s.admin, &new_admin);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_admin();
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn transfer_admin_emits_event() {
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::{IntoVal, Map, Symbol, TryIntoVal, Val};

    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.propose_admin(&s.admin, &new_admin);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_admin();

    let events = s.env.events().all();
    // Last event is AdminTransferred (topic: admin_transferred)
    let ev = events.last().unwrap();
    let topics = &ev.1;
    let topic0: Symbol = topics.get(0).unwrap().into_val(&s.env);
    assert_eq!(topic0, Symbol::new(&s.env, "admin_transferred"));
    let old_val: Address = topics.get(1).unwrap().into_val(&s.env);
    let new_val: Address = topics.get(2).unwrap().into_val(&s.env);
    assert_eq!(old_val, s.admin);
    assert_eq!(new_val, new_admin);

    let data: Map<Symbol, Val> = ev.2.try_into_val(&s.env).unwrap();
    assert!(data.get(Symbol::new(&s.env, "transferred_at")).is_some());
}

#[test]
fn transfer_admin_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let new_admin = Address::generate(&s.env);
    let err = s
        .client
        .try_propose_admin(&rando, &new_admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn execute_admin_rejects_before_timelock_elapses() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.propose_admin(&s.admin, &new_admin);
    let err = s.client.try_execute_admin().unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
    assert_eq!(s.client.admin(), s.admin);
}

#[test]
fn new_admin_can_withdraw_after_transfer() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.propose_admin(&s.admin, &new_admin);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_admin();

    // old admin can no longer withdraw
    fund_treasury(&s, 100_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);
    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &recipient, &100_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);

    // new admin can withdraw
    s.client.withdraw_fees(&new_admin, &s.token, &recipient, &100_000i128);
    assert_eq!(s.client.token_balance(&s.token), 0);
}

// ── pause / unpause (#403) ────────────────────────────────────────────────────

#[test]
fn is_paused_defaults_to_false() {
    let s = setup();
    assert!(!s.client.is_paused());
}

#[test]
fn pause_blocks_collect_fee() {
    let s = setup();
    s.client.pause(&s.admin);
    assert!(s.client.is_paused());
    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &1u32, &100i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::ContractPaused);
}

#[test]
fn pause_blocks_withdraw_fees() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);
    s.client.pause(&s.admin);
    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &recipient, &100_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::ContractPaused);
}

#[test]
fn unpause_restores_operations() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.pause(&s.admin);
    s.client.unpause(&s.admin);
    assert!(!s.client.is_paused());
    // collect_fee works again
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);
    assert_eq!(s.client.token_balance(&s.token), 500_000);
}

#[test]
fn pause_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let err = s.client.try_pause(&rando).unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn unpause_rejects_non_admin() {
    let s = setup();
    s.client.pause(&s.admin);
    let rando = Address::generate(&s.env);
    let err = s.client.try_unpause(&rando).unwrap_err().unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

// ── Emergency mode (#662, #722) ─────────────────────────────────────────────
//
// Regression for #722: `StorageKey::EmergencyMode` was referenced by
// `get_emergency_mode`/`set_emergency_mode` but was never added as a variant
// of the `StorageKey` enum, so the treasury crate has not compiled at all
// since the emergency-mode feature was introduced (#662) -- these are the
// first tests to exercise this path.

#[test]
fn emergency_mode_defaults_to_normal() {
    let s = setup();
    assert_eq!(s.client.get_emergency_mode(), storage::EmergencyMode::Normal);
}

#[test]
fn set_emergency_mode_updates_stored_mode() {
    let s = setup();
    s.client.set_emergency_mode(&s.admin, &storage::EmergencyMode::GlobalFreeze);
    assert_eq!(s.client.get_emergency_mode(), storage::EmergencyMode::GlobalFreeze);
}

#[test]
fn set_emergency_mode_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let err = s
        .client
        .try_set_emergency_mode(&rando, &storage::EmergencyMode::GlobalFreeze)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
    assert_eq!(s.client.get_emergency_mode(), storage::EmergencyMode::Normal);
}

#[test]
fn global_freeze_blocks_collect_fee_withdraw_and_distribute() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let stakeholder = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder, 10_000u32));
    s.client.propose_stakeholders(&s.admin, &stakeholders);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_stakeholders();

    s.client.set_emergency_mode(&s.admin, &storage::EmergencyMode::GlobalFreeze);

    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &2u32, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::EmergencyModeActive);

    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&s.admin, &s.token, &recipient, &1i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::EmergencyModeActive);

    let err = s
        .client
        .try_distribute_fees(&s.admin, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::EmergencyModeActive);
}

#[test]
fn trading_halted_and_settle_only_still_allow_collect_fee() {
    // Per the documented mode table, TradingHalted and SettleOnly only block
    // GlobalFreeze-gated calls elsewhere (Market contract) -- on Treasury,
    // collect_fee/withdraw_fees/distribute_fees are only blocked by
    // GlobalFreeze.
    let s = setup();
    s.client.set_emergency_mode(&s.admin, &storage::EmergencyMode::TradingHalted);
    s.client.collect_fee(&s.market, &s.token, &1u32, &100i128);
    assert_eq!(s.client.token_balance(&s.token), 100);

    s.client.set_emergency_mode(&s.admin, &storage::EmergencyMode::SettleOnly);
    s.client.collect_fee(&s.market, &s.token, &2u32, &100i128);
    assert_eq!(s.client.token_balance(&s.token), 200);
}

// ── #593: collect_fee pause gate ─────────────────────────────────────────────

/// `collect_fee` must be blocked while the treasury is paused and must return
/// `ContractPaused` (#50) — not succeed, not panic, not return a different error.
#[test]
fn collect_fee_paused_returns_contract_paused() {
    let s = setup();
    s.client.pause(&s.admin);

    let err = s
        .client
        .try_collect_fee(&s.market, &s.token, &42u32, &1_000i128)
        .unwrap_err()
        .unwrap();

    assert_eq!(
        err,
        TreasuryError::ContractPaused,
        "collect_fee must return ContractPaused while treasury is paused"
    );
}

/// The pause gate is checked before the authorised-market check, so even a
/// caller that is NOT a registered market gets `ContractPaused` rather than
/// `CallerNotMarket` while the treasury is paused.
#[test]
fn collect_fee_paused_before_market_auth_check() {
    let s = setup();
    s.client.pause(&s.admin);

    let rogue = Address::generate(&s.env);
    let err = s
        .client
        .try_collect_fee(&rogue, &s.token, &1u32, &500i128)
        .unwrap_err()
        .unwrap();

    // The pause gate fires before the market-registry check.
    assert_eq!(err, TreasuryError::ContractPaused);
}

/// Balances must remain unchanged after a rejected `collect_fee` during pause.
/// No state must be mutated when the call returns `ContractPaused`.
#[test]
fn collect_fee_paused_leaves_balances_unchanged() {
    let s = setup();
    // Collect some fees before pausing so we have a non-zero baseline.
    s.client.collect_fee(&s.market, &s.token, &1u32, &200_000i128);
    let balance_before = s.client.token_balance(&s.token);
    let cumulative_before = s.client.get_cumulative_fees(&s.token);
    let total_before = s.client.total_collected();

    s.client.pause(&s.admin);

    // Attempt a second collection while paused — must be rejected.
    let _ = s
        .client
        .try_collect_fee(&s.market, &s.token, &2u32, &100_000i128);

    // All counters must be unchanged.
    assert_eq!(s.client.token_balance(&s.token), balance_before);
    assert_eq!(s.client.get_cumulative_fees(&s.token), cumulative_before);
    assert_eq!(s.client.total_collected(), total_before);
}

/// After unpausing, `collect_fee` should succeed and update all counters as
/// normal — proving the gate does not leave permanent side effects.
#[test]
fn collect_fee_resumes_after_unpause() {
    let s = setup();
    s.client.pause(&s.admin);
    s.client.unpause(&s.admin);

    s.client.collect_fee(&s.market, &s.token, &7u32, &50_000i128);

    assert_eq!(s.client.token_balance(&s.token), 50_000);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 50_000);
    assert_eq!(s.client.total_collected(), 50_000);
}

// ── stakeholder fee distribution (#485) ───────────────────────────────────────

#[test]
fn propose_stakeholders_rejects_weights_not_summing_to_10000() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((a, 4_000u32));
    stakeholders.push_back((b, 4_000u32));

    let err = s
        .client
        .try_propose_stakeholders(&s.admin, &stakeholders)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidStakeholderWeights);
}

/// Regression for #721: an empty stakeholder list must be rejected at
/// `propose_stakeholders` with a typed error, not silently accepted (which
/// would let `execute_stakeholders` install an empty list and leave
/// `distribute_fees` to reject later, or worse, not reject at all).
#[test]
fn propose_stakeholders_rejects_empty_list() {
    let s = setup();
    let empty: soroban_sdk::Vec<(Address, u32)> = soroban_sdk::Vec::new(&s.env);

    let err = s
        .client
        .try_propose_stakeholders(&s.admin, &empty)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InvalidStakeholderWeights);
}

#[test]
fn propose_stakeholders_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let a = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((a, 10_000u32));

    let err = s
        .client
        .try_propose_stakeholders(&rando, &stakeholders)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

fn propose_and_execute_stakeholders(s: &Setup, stakeholders: &soroban_sdk::Vec<(Address, u32)>) {
    s.client.propose_stakeholders(&s.admin, stakeholders);
    s.env.ledger().with_mut(|li| {
        li.timestamp += crate::ADDRESS_TIMELOCK_SECONDS + 1;
    });
    s.client.execute_stakeholders();
}

#[test]
fn distribute_fees_pays_out_by_share() {
    let s = setup();
    fund_treasury(&s, 1_000_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &1_000_000i128);

    let stakeholder_a = Address::generate(&s.env);
    let stakeholder_b = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder_a.clone(), 7_000u32));
    stakeholders.push_back((stakeholder_b.clone(), 3_000u32));
    propose_and_execute_stakeholders(&s, &stakeholders);

    s.client.distribute_fees(&s.admin, &s.token);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&stakeholder_a), 700_000);
    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&stakeholder_b), 300_000);
    assert_eq!(s.client.token_balance(&s.token), 0);
    // Cumulative fees stay monotone — distribution only moves the live balance.
    assert_eq!(s.client.get_cumulative_fees(&s.token), 1_000_000);
}

/// Regression for #721: the payout loop used to push each stakeholder onto
/// the transfer list twice, doubling every real token transfer while the
/// treasury's own ledger (`distributed`/`remaining`) accounted for only one
/// payment. A single-stakeholder, 100%-share distribution makes the bug
/// unmistakable: with the bug, the second `token_client.transfer` for the
/// same stakeholder would attempt to move funds already fully paid out
/// (panicking on insufficient real balance once the treasury's actual token
/// balance is exhausted), and any surviving distribution would leave the
/// stakeholder short by exactly what they were shorted elsewhere or would
/// double what they're due — either way, the exact-value assertion below
/// only holds without the bug.
#[test]
fn distribute_fees_pays_exact_share_not_double() {
    let s = setup();
    fund_treasury(&s, 250_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &250_000i128);

    let stakeholder = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder.clone(), 10_000u32));
    propose_and_execute_stakeholders(&s, &stakeholders);

    s.client.distribute_fees(&s.admin, &s.token);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&stakeholder), 250_000);
    assert_eq!(s.client.token_balance(&s.token), 0);
}

#[test]
fn distribute_fees_rejects_without_stakeholders_configured() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);

    let err = s
        .client
        .try_distribute_fees(&s.admin, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::NoStakeholdersConfigured);
    // A rejected distribution must move no funds.
    assert_eq!(s.client.token_balance(&s.token), 500_000);
}

#[test]
fn distribute_fees_rejects_zero_balance() {
    let s = setup();
    let stakeholder = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder, 10_000u32));
    propose_and_execute_stakeholders(&s, &stakeholders);

    let err = s
        .client
        .try_distribute_fees(&s.admin, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::InsufficientBalance);
}

#[test]
fn distribute_fees_rejects_non_admin() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);
    let stakeholder = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder, 10_000u32));
    propose_and_execute_stakeholders(&s, &stakeholders);

    let rando = Address::generate(&s.env);
    let err = s
        .client
        .try_distribute_fees(&rando, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn distribute_fees_blocked_while_paused() {
    let s = setup();
    fund_treasury(&s, 500_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &500_000i128);
    let stakeholder = Address::generate(&s.env);
    let mut stakeholders = soroban_sdk::Vec::new(&s.env);
    stakeholders.push_back((stakeholder, 10_000u32));
    propose_and_execute_stakeholders(&s, &stakeholders);

    s.client.pause(&s.admin);
    let err = s
        .client
        .try_distribute_fees(&s.admin, &s.token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::ContractPaused);
}

// ── Issue #553: withdraw_fees auth + event ─────────────────────────────────────

/// `withdraw_fees` must emit a `FeesWithdrawn` event with the correct fields:
/// - topics: event name, token address, recipient address
/// - data: amount withdrawn, remaining token balance after withdrawal
#[test]
fn withdraw_fees_emits_fees_withdrawn_event() {
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::{IntoVal, Map, Symbol, TryIntoVal, Val};

    let s = setup();
    let collected = 1_000_000i128;
    let withdrawn = 400_000i128;
    let expected_remaining = collected - withdrawn;

    fund_treasury(&s, collected);
    s.client.collect_fee(&s.market, &s.token, &1u32, &collected);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &withdrawn);

    let events = s.env.events().all();
    // The last event must be FeesWithdrawn.
    let ev = events.last().unwrap();

    // ── topics ────────────────────────────────────────────────────────────────
    // Soroban contractevent layout: [contract_address, event_name, ...#topic fields]
    let topics = &ev.1;
    let event_name: Symbol = topics.get(0).unwrap().into_val(&s.env);
    assert_eq!(
        event_name,
        Symbol::new(&s.env, "fees_withdrawn"),
        "event name topic must be 'fees_withdrawn'"
    );
    let token_topic: Address = topics.get(1).unwrap().into_val(&s.env);
    assert_eq!(token_topic, s.token, "second topic must be the token address");
    let to_topic: Address = topics.get(2).unwrap().into_val(&s.env);
    assert_eq!(to_topic, recipient, "third topic must be the recipient address");

    // ── data fields ───────────────────────────────────────────────────────────
    let data: Map<Symbol, Val> = ev.2.try_into_val(&s.env).unwrap();

    let amount_val: i128 = data
        .get(Symbol::new(&s.env, "amount"))
        .unwrap()
        .into_val(&s.env);
    assert_eq!(amount_val, withdrawn, "event 'amount' must equal the withdrawn amount");

    let remaining_val: i128 = data
        .get(Symbol::new(&s.env, "remaining_token_balance"))
        .unwrap()
        .into_val(&s.env);
    assert_eq!(
        remaining_val, expected_remaining,
        "event 'remaining_token_balance' must equal balance after withdrawal"
    );
}

/// `withdraw_fees` must call `require_auth` on the caller address before
/// proceeding — calling it without the Soroban auth context must panic, not
/// return `Unauthorized`. This verifies the auth gate sits at the entry point
/// (before any business logic) rather than being implemented as a manual
/// address comparison only.
#[test]
#[should_panic]
fn withdraw_fees_panics_without_auth() {
    // No mock_all_auths() — the env has no auth context at all.
    let env = Env::default();
    let admin = Address::generate(&env);
    let market = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let treasury_id = env.register(TreasuryContract, ());
    let client = TreasuryContractClient::new(&env, &treasury_id);

    // Bootstrap the treasury with auth mocked just for initialize.
    env.mock_all_auths();
    client.initialize(&admin, &market);
    // Clear all mock authorizations — no auth context for subsequent calls.
    env.set_auths(&[]);

    let recipient = Address::generate(&env);
    // require_auth() is unsatisfied → must panic
    client.withdraw_fees(&admin, &token, &recipient, &1i128);
}

/// `withdraw_fees` called by a non-admin returns `TreasuryError::Unauthorized`.
/// This is distinct from the auth check above: the non-admin caller *does*
/// authorize themselves (mock auth), but the contract checks `caller == admin`
/// and rejects. Ensures both layers of access control work together.
#[test]
fn withdraw_fees_non_admin_returns_unauthorized() {
    let s = setup();
    fund_treasury(&s, 100_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);

    let imposter = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    let err = s
        .client
        .try_withdraw_fees(&imposter, &s.token, &recipient, &50_000i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        TreasuryError::Unauthorized,
        "a caller who is not the admin must receive Unauthorized even with valid auth"
    );
    // Verify no funds moved.
    assert_eq!(s.client.token_balance(&s.token), 100_000);
}

#[test]
fn total_collected_invariant_after_collect_and_withdraw() {
    let s = setup();
    assert_eq!(s.client.total_collected(), 0);

    fund_treasury(&s, 100_000);
    s.client.collect_fee(&s.market, &s.token, &1u32, &100_000i128);
    assert_eq!(s.client.total_collected(), 100_000);

    let recipient = Address::generate(&s.env);
    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &40_000i128);
    assert_eq!(s.client.total_collected(), 60_000);

    s.client.withdraw_fees(&s.admin, &s.token, &recipient, &60_000i128);
    assert_eq!(s.client.total_collected(), 0);
}

