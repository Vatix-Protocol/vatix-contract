use crate::types::TokenKind;
use soroban_sdk::{contractevent, Address, Env};

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenMinted {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub kind: TokenKind,
    pub amount: i128,
    pub new_balance: i128,
}

pub fn emit_token_minted(
    env: &Env,
    market_id: u32,
    user: &Address,
    kind: TokenKind,
    amount: i128,
    new_balance: i128,
) {
    TokenMinted {
        market_id,
        user: user.clone(),
        kind,
        amount,
        new_balance,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenBurned {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub kind: TokenKind,
    pub amount: i128,
    pub new_balance: i128,
}

pub fn emit_token_burned(
    env: &Env,
    market_id: u32,
    user: &Address,
    kind: TokenKind,
    amount: i128,
    new_balance: i128,
) {
    TokenBurned {
        market_id,
        user: user.clone(),
        kind,
        amount,
        new_balance,
    }
    .publish(env);
}

/// Emitted when an admin proposes rotating the `market_contract` (mint/burn
/// authority) address (Issue #691). Does not take effect until
/// `effective_at` and `execute_market_contract` is called.
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractProposed {
    #[topic]
    pub market_contract: Address,
    pub effective_at: u64,
}

pub fn emit_market_contract_proposed(env: &Env, market_contract: &Address, effective_at: u64) {
    MarketContractProposed {
        market_contract: market_contract.clone(),
        effective_at,
    }
    .publish(env);
}

/// Emitted once a proposed `market_contract` rotation's timelock has elapsed
/// and it is applied (Issue #691).
#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractSet {
    #[topic]
    pub market_contract: Address,
    pub set_at: u64,
}

pub fn emit_market_contract_set(env: &Env, market_contract: &Address) {
    MarketContractSet {
        market_contract: market_contract.clone(),
        set_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TokenTransferred {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub from: Address,
    pub to: Address,
    pub kind: TokenKind,
    pub amount: i128,
}

#[allow(dead_code)]
pub fn emit_token_transferred(
    env: &Env,
    market_id: u32,
    from: &Address,
    to: &Address,
    kind: TokenKind,
    amount: i128,
) {
    TokenTransferred {
        market_id,
        from: from.clone(),
        to: to.clone(),
        kind,
        amount,
    }
    .publish(env);
}

/// Emitted when an admin pauses the contract.
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractPaused {
    #[topic]
    pub admin: Address,
    pub paused_at: u64,
}

pub fn emit_contract_paused(env: &Env, admin: &Address) {
    ContractPaused {
        admin: admin.clone(),
        paused_at: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin unpauses the contract.
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractUnpaused {
    #[topic]
    pub admin: Address,
    pub unpaused_at: u64,
}

pub fn emit_contract_unpaused(env: &Env, admin: &Address) {
    ContractUnpaused {
        admin: admin.clone(),
        unpaused_at: env.ledger().timestamp(),
    }
    .publish(env);
}
