//! Event emission functions for the Vatix prediction market contract

use soroban_sdk::{contractevent, Address, BytesN, Env, String};

#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractInitializedEvent {
    #[topic]
    pub admin: Address,
    /// Ledger timestamp when the contract was bootstrapped.
    pub initialized_at: u64,
}

/// Emit an event when the contract is initialized with an admin.
///
/// Publishes a [`ContractInitializedEvent`] to the Soroban event stream when
/// `initialize` is called for the first time. Indexed by `admin` as a topic
/// so off-chain indexers can confirm who bootstrapped the contract.
///
/// # Arguments
/// * `env` - Contract environment
/// * `admin` - The address stored as the contract admin
pub fn emit_contract_initialized(env: &Env, admin: &Address) {
    ContractInitializedEvent {
        admin: admin.clone(),
        initialized_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCreatedEvent {
    #[topic]
    pub market_id: u32,
    pub creator: Address,
    pub question: String,
    pub end_time: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralDepositedEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralWithdrawnEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
    pub new_total: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawEdgeCaseEvent {
    #[topic]
    pub user: Address,
    #[topic]
    pub market_id: u32,
    pub amount: i128,
}

/// Emit event when collateral is deposited
///
/// Publishes a [`CollateralDepositedEvent`] to the Soroban event stream.
/// This event is indexed by `user` and `market_id` as topics, allowing
/// off-chain indexers to efficiently query deposits by user or market.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount deposited in stroops
/// * new_total - User's total collateral in this market after deposit
///
/// # Example
/// ```ignore
/// emit_collateral_deposited(&env, &user, 1, 5_000_000, 5_000_000);
/// ```
pub fn emit_collateral_deposited(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralDepositedEvent {
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Emit event when collateral is withdrawn
///
/// Publishes a [`CollateralWithdrawnEvent`] to the Soroban event stream.
/// This event is indexed by `user` and `market_id` as topics for efficient
/// querying by off-chain services.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount withdrawn in stroops
/// * new_total - User's total collateral in this market after withdrawal
///
/// # Example
/// ```ignore
/// emit_collateral_withdrawn(&env, &user, 1, 2_000_000, 3_000_000);
/// ```
pub fn emit_collateral_withdrawn(
    env: &Env,
    user: &Address,
    market_id: u32,
    amount: i128,
    new_total: i128,
) {
    CollateralWithdrawnEvent {
        user: user.clone(),
        market_id,
        amount,
        new_total,
    }
    .publish(env);
}

/// Emit a MarketCreated event
///
/// Publishes a [`MarketCreatedEvent`] to the Soroban event stream when a new
/// prediction market is initialized. The event is indexed by `market_id` as
/// a topic for efficient lookup by off-chain indexers and frontends.
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the created market
/// * creator - Address that created the market
/// * question - The market question
/// * end_time - Unix timestamp when market closes for trading
///
/// # Example
/// ```ignore
/// emit_market_created(&env, 1, &creator, &String::from_str(&env, "Will BTC hit $100k?"), 1735689600);
/// ```
pub fn emit_market_created(
    env: &Env,
    market_id: u32,
    creator: &Address,
    question: &String,
    end_time: u64,
) {
    // Publish the event with topics and data
    MarketCreatedEvent {
        market_id,
        creator: creator.clone(),
        question: question.clone(),
        end_time,
    }
    .publish(env);
}

/// Emit event for withdraw edge case when user has zero collateral deposited
///
/// Publishes a [`WithdrawEdgeCaseEvent`] when a user attempts to withdraw
/// from a market where they have no deposited collateral. This helps off-chain
/// monitoring tools identify potential UI bugs or user confusion.
///
/// # Arguments
/// * env - Soroban environment
/// * user - User's address
/// * market_id - Market identifier
/// * amount - Amount attempted to withdraw in stroops
///
/// # Example
/// ```ignore
/// emit_withdraw_edge_case(&env, &user, 1, 1_000_000);
/// ```
pub fn emit_withdraw_edge_case(env: &Env, user: &Address, market_id: u32, amount: i128) {
    WithdrawEdgeCaseEvent {
        user: user.clone(),
        market_id,
        amount,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketResolvedEvent {
    #[topic]
    pub market_id: u32,
    pub oracle_pubkey: BytesN<32>,
    pub resolver: Address,
    pub outcome: bool,
    pub resolved_at: u64,
}

/// Emit a MarketResolved event
///
/// Publishes a [`MarketResolvedEvent`] to the Soroban event stream when a
/// market is resolved by an oracle. The event is indexed by `market_id` as
/// a topic, allowing efficient queries for resolution events.
///
/// # Arguments
/// * env - Contract environment
/// * market_id - Unique identifier of the resolved market
/// * oracle_pubkey - Oracle public key used to verify the resolution signature
/// * resolver - Address of the resolver who submitted the resolution
/// * outcome - Market outcome (true = YES won, false = NO won)
/// * resolved_at - Unix timestamp when market was resolved
///
/// # Example
/// ```ignore
/// emit_market_resolved(&env, 1, &oracle_pubkey, &resolver_address, true, env.ledger().timestamp());
/// ```
pub fn emit_market_resolved(
    env: &Env,
    market_id: u32,
    oracle_pubkey: &BytesN<32>,
    resolver: &Address,
    outcome: bool,
    resolved_at: u64,
) {
    MarketResolvedEvent {
        market_id,
        oracle_pubkey: oracle_pubkey.clone(),
        resolver: resolver.clone(),
        outcome,
        resolved_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketCanceledEvent {
    #[topic]
    pub market_id: u32,
    pub canceler: Address,
    pub canceled_at: u64,
}

/// Emit a MarketCanceled event
///
/// Publishes a [`MarketCanceledEvent`] to the Soroban event stream when an
/// admin halts a market before resolution. The event is indexed by `market_id`
/// as a topic so off-chain indexers can detect canceled markets and surface
/// collateral-reclaim flows to affected users.
///
/// # Arguments
/// * `env` - Contract environment
/// * `market_id` - Unique identifier of the canceled market
/// * `canceler` - Admin address that canceled the market
/// * `canceled_at` - Unix timestamp (ledger time) when the cancellation occurred
///
/// # Example
/// ```ignore
/// emit_market_canceled(&env, 1, &admin, env.ledger().timestamp());
/// ```
pub fn emit_market_canceled(env: &Env, market_id: u32, canceler: &Address, canceled_at: u64) {
    MarketCanceledEvent {
        market_id,
        canceler: canceler.clone(),
        canceled_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PositionLimitExceededEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    /// The share side that would go negative: true = YES, false = NO
    pub side_yes: bool,
}

/// Emit an event when a position change is rejected because it would push a
/// share balance below zero.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier where the limit was hit
/// * `user` - Address of the user whose position change was rejected
/// * `side_yes` - `true` if the YES share balance would go negative; `false`
///   if the NO side would go negative
///
/// # Example
/// ```ignore
/// emit_position_limit_exceeded(&env, market_id, &user, true);
/// ```
pub fn emit_position_limit_exceeded(env: &Env, market_id: u32, user: &Address, side_yes: bool) {
    PositionLimitExceededEvent {
        market_id,
        user: user.clone(),
        side_yes,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionUpdatedEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub yes_shares: i128,
    pub no_shares: i128,
    pub locked_collateral: i128,
}

/// Emit an event whenever a user's position is modified (shares bought or sold).
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user whose position was updated
/// * `yes_shares` - New total YES share balance after the update
/// * `no_shares` - New total NO share balance after the update
/// * `locked_collateral` - Collateral (in stroops) now locked to cover the
///   net position
///
/// # Example
/// ```ignore
/// emit_position_updated(&env, market_id, &user, 100, 0, 100);
/// ```
#[allow(dead_code)]
pub fn emit_position_updated(
    env: &Env,
    market_id: u32,
    user: &Address,
    yes_shares: i128,
    no_shares: i128,
    locked_collateral: i128,
) {
    PositionUpdatedEvent {
        market_id,
        user: user.clone(),
        yes_shares,
        no_shares,
        locked_collateral,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TradeExecutedEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub quantity: i128,
    pub price_bps: i128,
    pub side_yes: bool,
    pub executed_at: u64,
}

/// Emit an event when a trade is executed (shares bought or sold).
///
/// Publishes a [`TradeExecutedEvent`] to the Soroban event stream indexed by
/// `market_id` and `user` to allow efficient off-chain indexing of trades
/// by market or trader.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user executing the trade
/// * `quantity` - Number of shares traded (always positive)
/// * `price_bps` - Market price in basis points (0–10_000)
/// * `side_yes` - `true` for YES side, `false` for NO side
/// * `executed_at` - Unix timestamp when the trade was executed
///
/// # Example
/// ```ignore
/// emit_trade_executed(&env, 1, &user, 100, 5_000, true, env.ledger().timestamp());
/// ```
pub fn emit_trade_executed(
    env: &Env,
    market_id: u32,
    user: &Address,
    quantity: i128,
    price_bps: i128,
    side_yes: bool,
    executed_at: u64,
) {
    TradeExecutedEvent {
        market_id,
        user: user.clone(),
        quantity,
        price_bps,
        side_yes,
        executed_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ValidationFailedEvent {
    #[topic]
    pub context: soroban_sdk::Symbol,
    pub error_code: u32,
}

/// Emit an event when a validation step fails, recording which context triggered
/// the failure and the associated error code.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `context` - Symbol identifying the validation site (e.g.
///   `Symbol::new(&env, "validate_collateral")`)
/// * `error_code` - Numeric value of the [`ContractError`] variant that was
///   returned (e.g. `31` for `InvalidQuantity`)
///
/// # Example
/// ```ignore
/// emit_validation_failed(
///     &env,
///     Symbol::new(&env, "validate_collateral"),
///     ContractError::InvalidQuantity as u32,
/// );
/// ```
#[allow(dead_code)]
pub fn emit_validation_failed(env: &Env, context: soroban_sdk::Symbol, error_code: u32) {
    ValidationFailedEvent {
        context,
        error_code,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PositionSettledEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub payout: i128,
    pub settled_at: u64,
}

/// Emit an event when a user's position is settled and payout is transferred.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user receiving the payout
/// * `payout` - Amount transferred to the user in stroops
/// * `settled_at` - Unix timestamp (ledger time) when settlement occurred
///
/// # Example
/// ```ignore
/// emit_position_settled(&env, market_id, &user, 500_000, env.ledger().timestamp());
/// ```
#[allow(dead_code)]
pub fn emit_position_settled(
    env: &Env,
    market_id: u32,
    user: &Address,
    payout: i128,
    settled_at: u64,
) {
    PositionSettledEvent {
        market_id,
        user: user.clone(),
        payout,
        settled_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct OracleSignatureVerifiedEvent {
    #[topic]
    pub market_id: u32,
    pub outcome: bool,
    pub verified_at: u64,
}

/// Emit event when oracle signature is verified
///
/// Publishes an [`OracleSignatureVerifiedEvent`] when an oracle's Ed25519
/// signature is successfully verified during market resolution. This event
/// provides an audit trail for resolution authenticity and is indexed by
/// `market_id` for efficient querying.
///
/// # Arguments
/// * env - Soroban environment
/// * market_id - Market identifier
/// * outcome - Verified outcome (true = YES, false = NO)
/// * verified_at - Unix timestamp when verification occurred
///
/// # Example
/// ```ignore
/// emit_oracle_signature_verified(&env, 1, true, env.ledger().timestamp());
/// ```
pub fn emit_oracle_signature_verified(env: &Env, market_id: u32, outcome: bool, verified_at: u64) {
    OracleSignatureVerifiedEvent {
        market_id,
        outcome,
        verified_at,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCalculatedEvent {
    #[topic]
    pub market_id: u32,
    #[topic]
    pub user: Address,
    pub fee_amount: i128,
    pub available_after_fee: i128,
}

/// Emit event when a fee is calculated during a withdrawal action.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `market_id` - Market identifier
/// * `user` - Address of the user performing the withdrawal
/// * `fee_amount` - Fee deducted in stroops
/// * `available_after_fee` - Collateral available to withdraw after fee
pub fn emit_fee_calculated(
    env: &Env,
    market_id: u32,
    user: &Address,
    fee_amount: i128,
    available_after_fee: i128,
) {
    FeeCalculatedEvent {
        market_id,
        user: user.clone(),
        fee_amount,
        available_after_fee,
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposedEvent {
    #[topic]
    pub current_admin: Address,
    #[topic]
    pub pending_admin: Address,
    pub proposed_at: u64,
}

pub fn emit_admin_transfer_proposed(env: &Env, current_admin: &Address, pending_admin: &Address) {
    AdminTransferProposedEvent {
        current_admin: current_admin.clone(),
        pending_admin: pending_admin.clone(),
        proposed_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferAcceptedEvent {
    #[topic]
    pub old_admin: Address,
    #[topic]
    pub new_admin: Address,
    pub accepted_at: u64,
}

pub fn emit_admin_transfer_accepted(env: &Env, old_admin: &Address, new_admin: &Address) {
    AdminTransferAcceptedEvent {
        old_admin: old_admin.clone(),
        new_admin: new_admin.clone(),
        accepted_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasurySetEvent {
    #[topic]
    pub treasury: Address,
    pub set_at: u64,
}

pub fn emit_treasury_set(env: &Env, treasury: &Address) {
    TreasurySetEvent {
        treasury: treasury.clone(),
        set_at: env.ledger().timestamp(),
    }
    .publish(env);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarketContract;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Env, IntoVal, Map, String, Symbol, TryIntoVal, Val,
    };

    #[test]
    fn test_emit_contract_initialized() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            emit_contract_initialized(&env, &admin);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "contract_initialized_event"));

        let topic1: Address = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, admin);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let initialized_at_val: u64 = data
            .get(Symbol::new(&env, "initialized_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(initialized_at_val, env.ledger().timestamp());
    }

    #[test]
    fn test_emit_market_created() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let creator = Address::generate(&env);
        let question = String::from_str(&env, "Will BTC hit $100k?");
        let end_time = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_created(&env, market_id, &creator, &question, end_time);
        });

        // Verify event was published
        let events = env.events().all();
        assert_eq!(events.len(), 1);

        // Verify event content
        let event = events.first().unwrap();
        // event is (Contract Address, Topics, Data)

        // Topics
        let topics = &event.1;
        assert_eq!(topics.len(), 2);

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_created_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        // Data
        // Data
        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let creator_val: Address = data
            .get(Symbol::new(&env, "creator"))
            .unwrap()
            .into_val(&env);
        let question_val: String = data
            .get(Symbol::new(&env, "question"))
            .unwrap()
            .into_val(&env);
        let end_time_val: u64 = data
            .get(Symbol::new(&env, "end_time"))
            .unwrap()
            .into_val(&env);
        assert_eq!(creator_val, creator);
        assert_eq!(question_val, question);
        assert_eq!(end_time_val, end_time);
    }

    #[test]
    fn test_emit_market_resolved() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let resolver = Address::generate(&env);
        let outcome = true;
        let resolved_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_resolved(&env, market_id, &oracle_pubkey, &resolver, outcome, resolved_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_resolved_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let resolver_val: BytesN<32> = data
            .get(Symbol::new(&env, "resolver"))
            .unwrap()
            .into_val(&env);
        let outcome_val: bool = data
            .get(Symbol::new(&env, "outcome"))
            .unwrap()
            .into_val(&env);
        let resolved_at_val: u64 = data
            .get(Symbol::new(&env, "resolved_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(resolver_val, resolver);
        assert_eq!(outcome_val, outcome);
        assert_eq!(resolved_at_val, resolved_at);
    }

    #[test]
    fn test_emit_market_canceled() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let canceler = Address::generate(&env);
        let canceled_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_market_canceled(&env, market_id, &canceler, canceled_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "market_canceled_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let canceler_val: Address = data
            .get(Symbol::new(&env, "canceler"))
            .unwrap()
            .into_val(&env);
        let canceled_at_val: u64 = data
            .get(Symbol::new(&env, "canceled_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(canceler_val, canceler);
        assert_eq!(canceled_at_val, canceled_at);
    }

    #[test]
    fn test_emit_position_updated() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let yes_shares = 100i128;
        let no_shares = 50i128;
        let locked_collateral = 150i128;

        env.as_contract(&contract_id, || {
            emit_position_updated(
                &env,
                market_id,
                &user,
                yes_shares,
                no_shares,
                locked_collateral,
            );
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_updated_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let yes_shares_val: i128 = data
            .get(Symbol::new(&env, "yes_shares"))
            .unwrap()
            .into_val(&env);
        let no_shares_val: i128 = data
            .get(Symbol::new(&env, "no_shares"))
            .unwrap()
            .into_val(&env);
        let locked_collateral_val: i128 = data
            .get(Symbol::new(&env, "locked_collateral"))
            .unwrap()
            .into_val(&env);

        assert_eq!(yes_shares_val, yes_shares);
        assert_eq!(no_shares_val, no_shares);
        assert_eq!(locked_collateral_val, locked_collateral);
    }

    #[test]
    fn test_emit_position_settled() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let payout = 100i128;
        let settled_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_position_settled(&env, market_id, &user, payout, settled_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "position_settled_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let topic2: Address = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, user);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let payout_val: i128 = data
            .get(Symbol::new(&env, "payout"))
            .unwrap()
            .into_val(&env);
        assert_eq!(payout_val, payout);
    }

    #[test]
    fn test_emit_collateral_deposited() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 1000i128;
        let new_total = 1000i128;

        env.as_contract(&contract_id, || {
            emit_collateral_deposited(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_deposited_event"));

        let topic1: Address = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, user);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }

    #[test]
    fn test_emit_validation_failed() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let context = Symbol::new(&env, "validate_collateral");
        let error_code = 31u32;

        env.as_contract(&contract_id, || {
            emit_validation_failed(&env, context.clone(), error_code);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "validation_failed_event"));

        let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, context);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let code: u32 = data
            .get(Symbol::new(&env, "error_code"))
            .unwrap()
            .into_val(&env);
        assert_eq!(code, error_code);
    }

    #[test]
    fn test_emit_collateral_withdrawn() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let amount = 500i128;
        let new_total = 500i128;

        env.as_contract(&contract_id, || {
            emit_collateral_withdrawn(&env, &user, market_id, amount, new_total);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "collateral_withdrawn_event"));

        let topic1: Address = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, user);

        let topic2: u32 = topics.get(2).unwrap().into_val(&env);
        assert_eq!(topic2, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let amount_val: i128 = data
            .get(Symbol::new(&env, "amount"))
            .unwrap()
            .into_val(&env);
        let new_total_val: i128 = data
            .get(Symbol::new(&env, "new_total"))
            .unwrap()
            .into_val(&env);
        assert_eq!(amount_val, amount);
        assert_eq!(new_total_val, new_total);
    }

    #[test]
    fn test_emit_oracle_signature_verified() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let outcome = true;
        let verified_at = 1234567890u64;

        env.as_contract(&contract_id, || {
            emit_oracle_signature_verified(&env, market_id, outcome, verified_at);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "oracle_signature_verified_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let outcome_val: bool = data
            .get(Symbol::new(&env, "outcome"))
            .unwrap()
            .into_val(&env);
        let verified_at_val: u64 = data
            .get(Symbol::new(&env, "verified_at"))
            .unwrap()
            .into_val(&env);
        assert_eq!(outcome_val, outcome);
        assert_eq!(verified_at_val, verified_at);
    }

    #[test]
    fn test_emit_fee_calculated() {
        let env = Env::default();
        let contract_id = env.register(MarketContract, ());

        let market_id = 1u32;
        let user = Address::generate(&env);
        let fee_amount = 0i128;
        let available_after_fee = 5_000i128;

        env.as_contract(&contract_id, || {
            emit_fee_calculated(&env, market_id, &user, fee_amount, available_after_fee);
        });

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let event = events.first().unwrap();
        let topics = &event.1;

        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(topic0, Symbol::new(&env, "fee_calculated_event"));

        let topic1: u32 = topics.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, market_id);

        let data: Map<Symbol, Val> = event.2.try_into_val(&env).unwrap();
        let fee_val: i128 = data
            .get(Symbol::new(&env, "fee_amount"))
            .unwrap()
            .into_val(&env);
        let available_val: i128 = data
            .get(Symbol::new(&env, "available_after_fee"))
            .unwrap()
            .into_val(&env);
        assert_eq!(fee_val, fee_amount);
        assert_eq!(available_val, available_after_fee);
    }
}
