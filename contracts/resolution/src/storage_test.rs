//! Unit tests for the pending-address timelock storage helpers (Issue #1070).
//!
//! The three pending variants — `PendingTreasury`, `PendingFactory` and
//! `PendingMarketContract` — gate privileged address changes behind a
//! timelock. Before this module the market-contract variant in particular had
//! no coverage at all, so a regression in the *clear* path could leave a stale
//! pending address behind that a later `execute_market_contract` would happily
//! apply.
//!
//! Each variant is exercised through the same lifecycle so the three paths
//! cannot drift:
//!
//! 1. `get_pending_*` returns `None` on a fresh environment.
//! 2. `set_pending_*` writes a [`PendingAddressChange`] that `get_pending_*`
//!    reads back verbatim.
//! 3. A second `set_pending_*` *replaces* the first value (it does not
//!    append), so only the most recent value is readable.
//! 4. `clear_pending_*` removes the entry, and `get_pending_*` returns `None`
//!    again.

use crate::storage::{
    clear_pending_factory, clear_pending_market_contract, clear_pending_treasury,
    get_pending_factory, get_pending_market_contract, get_pending_treasury, set_pending_factory,
    set_pending_market_contract, set_pending_treasury,
};
use crate::types::PendingAddressChange;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Build a pending change with a distinct address and timestamp so that
/// "first value" and "second value" are distinguishable in assertions.
fn pending(env: &Env, effective_at: u64) -> PendingAddressChange {
    PendingAddressChange {
        new_address: Address::generate(env),
        effective_at,
    }
}

/// Run `body` inside the registered contract's storage context so the
/// persistent storage helpers behave exactly as they do on-chain.
fn in_contract<R>(env: &Env, body: impl FnOnce() -> R) -> R {
    let contract_id = env.register(crate::ResolutionContract, ());
    env.as_contract(&contract_id, body)
}

// ── PendingTreasury ───────────────────────────────────────────────────────

#[test]
fn pending_treasury_get_returns_none_on_fresh_env() {
    let env = Env::default();
    in_contract(&env, || {
        assert_eq!(get_pending_treasury(&env), None);
    });
}

#[test]
fn pending_treasury_set_get_clear_round_trip() {
    let env = Env::default();
    in_contract(&env, || {
        let change = pending(&env, 1_000);
        set_pending_treasury(&env, &change);
        assert_eq!(get_pending_treasury(&env), Some(change.clone()));

        clear_pending_treasury(&env);
        assert_eq!(get_pending_treasury(&env), None);
    });
}

#[test]
fn pending_treasury_second_set_replaces_first() {
    let env = Env::default();
    in_contract(&env, || {
        let first = pending(&env, 1_000);
        let second = pending(&env, 2_000);
        set_pending_treasury(&env, &first);
        set_pending_treasury(&env, &second);

        // Only the most recent value is readable — the first is overwritten,
        // not appended or retained alongside the second.
        assert_eq!(get_pending_treasury(&env), Some(second));
    });
}

// ── PendingFactory ────────────────────────────────────────────────────────

#[test]
fn pending_factory_get_returns_none_on_fresh_env() {
    let env = Env::default();
    in_contract(&env, || {
        assert_eq!(get_pending_factory(&env), None);
    });
}

#[test]
fn pending_factory_set_get_clear_round_trip() {
    let env = Env::default();
    in_contract(&env, || {
        let change = pending(&env, 1_000);
        set_pending_factory(&env, &change);
        assert_eq!(get_pending_factory(&env), Some(change.clone()));

        clear_pending_factory(&env);
        assert_eq!(get_pending_factory(&env), None);
    });
}

#[test]
fn pending_factory_second_set_replaces_first() {
    let env = Env::default();
    in_contract(&env, || {
        let first = pending(&env, 1_000);
        let second = pending(&env, 2_000);
        set_pending_factory(&env, &first);
        set_pending_factory(&env, &second);

        assert_eq!(get_pending_factory(&env), Some(second));
    });
}


// ── PendingMarketContract ─────────────────────────────────────────────────

#[test]
fn pending_market_contract_get_returns_none_on_fresh_env() {
    let env = Env::default();
    in_contract(&env, || {
        assert_eq!(get_pending_market_contract(&env), None);
    });
}

#[test]
fn pending_market_contract_set_get_clear_round_trip() {
    let env = Env::default();
    in_contract(&env, || {
        let change = pending(&env, 1_000);
        set_pending_market_contract(&env, &change);
        assert_eq!(get_pending_market_contract(&env), Some(change.clone()));

        clear_pending_market_contract(&env);
        assert_eq!(get_pending_market_contract(&env), None);
    });
}

#[test]
fn pending_market_contract_second_set_replaces_first() {
    let env = Env::default();
    in_contract(&env, || {
        let first = pending(&env, 1_000);
        let second = pending(&env, 2_000);
        set_pending_market_contract(&env, &first);
        set_pending_market_contract(&env, &second);

        assert_eq!(get_pending_market_contract(&env), Some(second));
    });
}

// ── Cross-variant isolation ──────────────────────────────────────────────

/// The three pending slots are independent: writing one must not make another
/// readable, and clearing one must not disturb the others.
#[test]
fn pending_variants_do_not_leak_into_each_other() {
    let env = Env::default();
    in_contract(&env, || {
        let treasury = pending(&env, 1_000);
        let factory = pending(&env, 2_000);
        let market = pending(&env, 3_000);

        set_pending_treasury(&env, &treasury);
        assert_eq!(get_pending_factory(&env), None);
        assert_eq!(get_pending_market_contract(&env), None);

        set_pending_factory(&env, &factory);
        set_pending_market_contract(&env, &market);

        assert_eq!(get_pending_treasury(&env), Some(treasury.clone()));
        assert_eq!(get_pending_factory(&env), Some(factory.clone()));
        assert_eq!(get_pending_market_contract(&env), Some(market.clone()));

        clear_pending_market_contract(&env);
        assert_eq!(get_pending_market_contract(&env), None);
        assert_eq!(get_pending_treasury(&env), Some(treasury));
        assert_eq!(get_pending_factory(&env), Some(factory));
    });
}
