use soroban_sdk::{contracttype, Address};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum TokenKind {
    Yes,
    No,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct OutcomeTokenConfig {
    pub admin: Address,
    /// Only this address may call mint and burn.
    pub market_contract: Address,
}
