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
