use soroban_sdk::{contracttype, Address, BytesN, String};

/// Represents the possible states of a prediction market.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum MarketStatus {
    Active,
    Resolved,
    Canceled,
}

/// Core structure containing all relevant information for a Market.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Market {
    pub id: u32,
    pub question: String,
    pub end_time: u64,
    pub oracle_pubkey: BytesN<32>,
    pub status: MarketStatus,
    pub result: Option<bool>,
    pub creator: Address,
    pub created_at: u64,
    pub collateral_token: Address,
}

/// Tracks the position and shares of a specific user in a market.
#[derive(Clone, Debug)]
#[contracttype]
pub struct Position {
    pub market_id: u32,
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    pub locked_collateral: i128,
    pub is_settled: bool,
}
