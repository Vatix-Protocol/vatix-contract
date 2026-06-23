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
    /// Current market price in basis points (0–10_000). Updated on every trade.
    pub price_bps: i128,
}

/// Tracks the position and shares of a specific user in a market.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Position {
    pub market_id: u32,
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    /// Collateral required to back current YES/NO shares (from calculate_locked_collateral).
    pub locked_collateral: i128,
    /// Total collateral deposited by user in this market (never decreased except by withdraw).
    pub total_deposited: i128,
    pub is_settled: bool,
}

impl Position {
    /// Create an empty position for a user in a market.
    /// Used when a position has not been previously recorded in storage.
    pub fn new_empty(market_id: u32, user: Address) -> Self {
        Position {
            market_id,
            user,
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: 0,
            is_settled: false,
        }
    }
}
