#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
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

#[test]
fn initialize_writes_storage_version() {
    let s = setup();
    s.env.as_contract(&s.treasury_id, || {
        assert_eq!(storage::get_version(&s.env), Some(storage::STORAGE_VERSION));
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
fn remove_market_errors_if_not_registered() {
    let s = setup();
    let unknown = Address::generate(&s.env);
    let err = s
        .client
        .try_remove_market(&s.admin, &unknown)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::CallerNotMarket);
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
fn multiple_markets_can_each_collect_fees() {
    let s = setup();
    let market2 = Address::generate(&s.env);
    s.client.add_market(&s.admin, &market2);

    s.client.collect_fee(&s.market, &s.token, &1u32, &100i128);
    s.client.collect_fee(&market2, &s.token, &2u32, &200i128);

    assert_eq!(s.client.token_balance(&s.token), 300);
    assert_eq!(s.client.get_cumulative_fees(&s.token), 300);
}

// ── transfer_admin ────────────────────────────────────────────────────────────

#[test]
fn transfer_admin_updates_admin() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&s.admin, &new_admin);
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn transfer_admin_emits_event() {
    use soroban_sdk::testutils::Events as _;
    use soroban_sdk::{IntoVal, Map, Symbol, TryIntoVal, Val};

    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&s.admin, &new_admin);

    let events = s.env.events().all();
    // Last event is AdminTransferred (topic: admin_transferred)
    let ev = events.last().unwrap();
    let topics = &ev.1;
    let topic0: Symbol = topics.get(0).unwrap().into_val(&s.env);
    assert_eq!(topic0, Symbol::new(&s.env, "admin_transferred"));

    let data: Map<Symbol, Val> = ev.2.try_into_val(&s.env).unwrap();
    let old_val: Address = data.get(Symbol::new(&s.env, "old_admin")).unwrap().into_val(&s.env);
    let new_val: Address = data.get(Symbol::new(&s.env, "new_admin")).unwrap().into_val(&s.env);
    assert_eq!(old_val, s.admin);
    assert_eq!(new_val, new_admin);
}

#[test]
fn transfer_admin_rejects_non_admin() {
    let s = setup();
    let rando = Address::generate(&s.env);
    let new_admin = Address::generate(&s.env);
    let err = s
        .client
        .try_transfer_admin(&rando, &new_admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, TreasuryError::Unauthorized);
}

#[test]
fn new_admin_can_withdraw_after_transfer() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&s.admin, &new_admin);

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
