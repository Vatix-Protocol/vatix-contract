//! Event emission functions for the Vatix Treasury contract.
//!
//! All events are published to the Soroban event stream and indexed by the
//! topics marked with `#[topic]`. Off-chain indexers can filter on these
//! topics to reconstruct treasury activity without scanning all contract
//! storage.

use soroban_sdk::{contractevent, Address, Env};

// ── Initialization ─────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryInitializedEvent {
    #[topic]
    pub admin: Address,
    #[topic]
    pub authorized_market: Address,
    pub initialized_at: u64,
}

/// Emit when the treasury is bootstrapped for the first time.
pub fn emit_treasury_initialized(env: &Env, admin: &Address, authorized_market: &Address) {
    TreasuryInitializedEvent {
        admin: admin.clone(),
        authorized_market: authorized_market.clone(),
        initialized_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Fee collection ─────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollectedEvent {
    /// The SAC token address in which the fee was denominated.
    #[topic]
    pub token: Address,
    /// Source market contract that triggered the collection.
    #[topic]
    pub market: Address,
    /// Fee amount in stroops (1 USDC = 10^7 stroops).
    pub amount: i128,
    /// Running total of fees accumulated for this token.
    pub cumulative: i128,
    pub collected_at: u64,
}

/// Emit when the market contract transfers a protocol fee into the treasury.
pub fn emit_fee_collected(
    env: &Env,
    token: &Address,
    market: &Address,
    amount: i128,
    cumulative: i128,
) {
    FeeCollectedEvent {
        token: token.clone(),
        market: market.clone(),
        amount,
        cumulative,
        collected_at: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Admin withdrawal ───────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeesWithdrawnEvent {
    #[topic]
    pub admin: Address,
    #[topic]
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    pub withdrawn_at: u64,
}

/// Emit when the admin withdraws accumulated fees to an external address.
pub fn emit_fees_withdrawn(env: &Env, admin: &Address, token: &Address, to: &Address, amount: i128) {
    FeesWithdrawnEvent {
        admin: admin.clone(),
        token: token.clone(),
        to: to.clone(),
        amount,
        withdrawn_at: env.ledger().timestamp(),
    }
    .publish(env);
}
