#![no_std]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Only the registered **market contract** may deposit fees
//! via [`TreasuryContract::collect_fee`]; an **admin** address controls all
//! other privileged operations (withdrawal, market contract rotation).
//!
//! ## Fee flow
//!
//! ```text
//!  User withdrawal
//!      │  fee_amount > 0
//!      ▼
//!  MarketContract
//!      │  token.transfer(market → treasury, fee_amount)
//!      │  treasury.collect_fee(market, token, market_id, fee_amount)
//!      ▼
//!  TreasuryContract  ←  accumulates per-token balances
//!      │  (admin only)
//!      ▼
//!  treasury.withdraw_fees(admin, token, to, amount)
//! ```
//!
//! ## Authorization model
//!
//! | Operation            | Who may call             |
//! |----------------------|--------------------------|
//! | `initialize`         | anyone (once)            |
//! | `collect_fee`        | registered market contract|
//! | `withdraw_fees`      | admin                    |
//! | `set_market_contract`| admin                    |
//! | Getters              | anyone                   |
//!
//! ## Storage layout
//!
//! | Key                      | Type      | Description                         |
//! |--------------------------|-----------|-------------------------------------|
//! | `Admin`                  | `Address` | Protocol admin                      |
//! | `MarketContract`         | `Address` | Authorized fee-depositing contract  |
//! | `TotalCollected`         | `i128`    | Cumulative fees across all tokens   |
//! | `TokenBalance(Address)`  | `i128`    | Per-token custodied balance         |

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum StorageKey {
    /// Protocol admin address.
    Admin,
    /// The single market contract address allowed to call `collect_fee`.
    MarketContract,
    /// Cumulative fees collected across all tokens (in stroops).
    TotalCollected,
    /// Custodied balance for a specific collateral token.
    TokenBalance(Address),
}

// ── Errors ────────────────────────────────────────────────────────────────────

/// Error codes for the Treasury contract.
///
/// Ranges:
/// - Initialization errors: 1–9
/// - Authorization errors: 10–19
/// - Amount / balance errors: 20–29
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TreasuryError {
    // ── Initialization (1–9) ─────────────────────────────────────────────────
    /// `initialize` has already been called; cannot reinitialize.
    AlreadyInitialized = 1,

    /// An operation requires the treasury to be initialized, but it is not.
    NotInitialized = 2,

    // ── Authorization (10–19) ─────────────────────────────────────────────────
    /// Caller is not the stored admin.
    Unauthorized = 10,

    /// `collect_fee` was invoked by an address that is not the registered
    /// market contract.
    CallerNotMarket = 11,

    // ── Amount / balance (20–29) ──────────────────────────────────────────────
    /// `fee_amount` or `amount` is zero or negative.
    InvalidAmount = 20,

    /// The treasury does not hold enough of `token` to satisfy the withdrawal.
    InsufficientBalance = 21,
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryInitializedEvent {
    #[topic]
    pub admin: Address,
    #[topic]
    pub market_contract: Address,
    pub initialized_at: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollectedEvent {
    /// The market that generated the fee.
    #[topic]
    pub market_id: u32,
    /// Token in which the fee was paid.
    #[topic]
    pub token: Address,
    /// Fee collected in this call (stroops).
    pub fee_amount: i128,
    /// Cumulative balance of `token` held by the treasury after this call.
    pub new_token_balance: i128,
    /// Cumulative total across all tokens after this call.
    pub new_total_collected: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FeesWithdrawnEvent {
    #[topic]
    pub token: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
    pub remaining_token_balance: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MarketContractUpdatedEvent {
    #[topic]
    pub old_market_contract: Address,
    #[topic]
    pub new_market_contract: Address,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Bootstrap the treasury.
    ///
    /// Sets the admin and registers the market contract that is permitted to
    /// call [`collect_fee`]. Must be called once immediately after deployment;
    /// subsequent calls return [`TreasuryError::AlreadyInitialized`].
    ///
    /// # Arguments
    /// * `env` — Soroban environment.
    /// * `admin` — Protocol admin address. Authorizes privileged operations.
    /// * `market_contract` — Address of the market contract that will route
    ///   withdrawal fees here. Only this address may call `collect_fee`.
    ///
    /// # Errors
    /// * [`TreasuryError::AlreadyInitialized`] — treasury already initialized.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(TreasuryError::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::MarketContract, &market_contract);
        env.storage()
            .instance()
            .set(&StorageKey::TotalCollected, &0i128);
        TreasuryInitializedEvent {
            admin: admin.clone(),
            market_contract: market_contract.clone(),
            initialized_at: env.ledger().timestamp(),
        }
        .publish(&env);
        Ok(())
    }

    // ── Fee collection ────────────────────────────────────────────────────────

    /// Record a protocol fee collected from a market withdrawal.
    ///
    /// **Only the registered market contract** may call this function. The
    /// market contract is responsible for transferring `fee_amount` tokens to
    /// this contract's address *before* (or atomically with) this call, so
    /// that the on-chain token balance matches the accounting recorded here.
    ///
    /// # Arguments
    /// * `env` — Soroban environment.
    /// * `caller` — Address of the invoking contract; must equal the stored
    ///   `MarketContract` address. Must call `require_auth()` so Soroban's
    ///   auth engine can verify the invocation chain.
    /// * `token` — Collateral token in which the fee was paid (e.g. USDC SAC).
    /// * `market_id` — Identifier of the market that generated the fee.
    /// * `fee_amount` — Fee in stroops (must be > 0).
    ///
    /// # Errors
    /// * [`TreasuryError::NotInitialized`] — treasury not yet initialized.
    /// * [`TreasuryError::CallerNotMarket`] — `caller` is not the registered
    ///   market contract.
    /// * [`TreasuryError::InvalidAmount`] — `fee_amount` is zero or negative.
    ///
    /// # Events
    /// Emits [`FeeCollectedEvent`] with market_id, token, fee_amount, and
    /// updated running totals.
    pub fn collect_fee(
        env: Env,
        caller: Address,
        token: Address,
        market_id: u32,
        fee_amount: i128,
    ) -> Result<(), TreasuryError> {
        // Caller must prove it is authorized (allows Soroban auth graph to
        // verify the invocation chain from the user through the market to here).
        caller.require_auth();

        // Gate: only the registered market contract may deposit fees.
        let market_contract = Self::require_initialized(&env)?;
        if caller != market_contract {
            return Err(TreasuryError::CallerNotMarket);
        }

        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        // Update per-token balance.
        let token_key = StorageKey::TokenBalance(token.clone());
        let prev_token_balance: i128 = env
            .storage()
            .persistent()
            .get(&token_key)
            .unwrap_or(0i128);
        let new_token_balance = prev_token_balance
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        env.storage()
            .persistent()
            .set(&token_key, &new_token_balance);

        // Update cumulative total.
        let prev_total: i128 = env
            .storage()
            .instance()
            .get(&StorageKey::TotalCollected)
            .unwrap_or(0i128);
        let new_total = prev_total.checked_add(fee_amount).unwrap_or(i128::MAX);
        env.storage()
            .instance()
            .set(&StorageKey::TotalCollected, &new_total);

        FeeCollectedEvent {
            market_id,
            token,
            fee_amount,
            new_token_balance,
            new_total_collected: new_total,
        }
        .publish(&env);

        Ok(())
    }

    // ── Admin operations ──────────────────────────────────────────────────────

    /// Withdraw accumulated fees to a recipient address.
    ///
    /// Transfers `amount` of `token` from this contract to `to`, then
    /// decrements the on-chain accounting balance. Only the admin may call
    /// this.
    ///
    /// # Arguments
    /// * `env` — Soroban environment.
    /// * `caller` — Must be the stored admin address.
    /// * `token` — The collateral token to withdraw.
    /// * `to` — Recipient address (e.g. a multisig wallet).
    /// * `amount` — Amount in stroops to transfer (must be > 0).
    ///
    /// # Errors
    /// * [`TreasuryError::NotInitialized`] — treasury not yet initialized.
    /// * [`TreasuryError::Unauthorized`] — `caller` is not the admin.
    /// * [`TreasuryError::InvalidAmount`] — `amount` is zero or negative.
    /// * [`TreasuryError::InsufficientBalance`] — treasury holds less than
    ///   `amount` of the requested token.
    ///
    /// # Events
    /// Emits [`FeesWithdrawnEvent`] with token, to, amount, and remaining balance.
    pub fn withdraw_fees(
        env: Env,
        caller: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let token_key = StorageKey::TokenBalance(token.clone());
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&token_key)
            .unwrap_or(0i128);

        if amount > balance {
            return Err(TreasuryError::InsufficientBalance);
        }

        // Transfer tokens from this contract to the recipient.
        let treasury = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&treasury, &to, &amount);

        // Update accounting.
        let remaining = balance - amount;
        env.storage().persistent().set(&token_key, &remaining);

        FeesWithdrawnEvent {
            token,
            to,
            amount,
            remaining_token_balance: remaining,
        }
        .publish(&env);

        Ok(())
    }

    /// Rotate the registered market contract address.
    ///
    /// Useful when the market contract is upgraded. Only the admin may call
    /// this.
    ///
    /// # Errors
    /// * [`TreasuryError::NotInitialized`]
    /// * [`TreasuryError::Unauthorized`]
    pub fn set_market_contract(
        env: Env,
        caller: Address,
        new_market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let old: Address = env
            .storage()
            .instance()
            .get(&StorageKey::MarketContract)
            .ok_or(TreasuryError::NotInitialized)?;

        env.storage()
            .instance()
            .set(&StorageKey::MarketContract, &new_market_contract);

        MarketContractUpdatedEvent {
            old_market_contract: old,
            new_market_contract,
        }
        .publish(&env);

        Ok(())
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    /// Return the admin address.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("treasury not initialized")
    }

    /// Return the registered market contract address.
    pub fn market_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::MarketContract)
            .expect("treasury not initialized")
    }

    /// Return the custodied balance for a specific token (in stroops).
    pub fn token_balance(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::TokenBalance(token))
            .unwrap_or(0i128)
    }

    /// Return cumulative fees collected across all tokens since deployment.
    pub fn total_collected(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::TotalCollected)
            .unwrap_or(0i128)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn require_initialized(env: &Env) -> Result<Address, TreasuryError> {
        env.storage()
            .instance()
            .get(&StorageKey::MarketContract)
            .ok_or(TreasuryError::NotInitialized)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), TreasuryError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(TreasuryError::NotInitialized)?;
        if &admin != caller {
            return Err(TreasuryError::Unauthorized);
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        token::StellarAssetClient,
        Env, IntoVal, Map, Symbol, TryIntoVal, Val,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

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

        // Register a real SAC token so transfers work.
        let token_admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let token = token_contract.address();

        let treasury_id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &treasury_id);
        client.initialize(&admin, &market);

        Setup {
            env,
            admin,
            market,
            token,
            treasury_id,
            client,
        }
    }

    // Fund the treasury with tokens (simulates prior fee transfers from market).
    fn fund_treasury(s: &Setup, amount: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury_id, &amount);
    }

    // ── Initialization ────────────────────────────────────────────────────────

    #[test]
    fn test_initialize_stores_admin_and_market() {
        let s = setup();
        assert_eq!(s.client.admin(), s.admin);
        assert_eq!(s.client.market_contract(), s.market);
        assert_eq!(s.client.total_collected(), 0);
    }

    #[test]
    fn test_initialize_twice_fails() {
        let s = setup();
        let result = s.client.try_initialize(&s.admin, &s.market);
        assert_eq!(result, Err(Ok(TreasuryError::AlreadyInitialized)));
    }

    #[test]
    fn test_initialize_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let market = Address::generate(&env);
        let id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &id);
        client.initialize(&admin, &market);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, topics, _) = events.first().unwrap();
        let name: Symbol = topics.get(0).unwrap().into_val(&env);
        assert_eq!(name, Symbol::new(&env, "treasury_initialized_event"));
    }

    // ── collect_fee ───────────────────────────────────────────────────────────

    #[test]
    fn test_collect_fee_updates_balances() {
        let s = setup();
        s.client
            .collect_fee(&s.market, &s.token, &1u32, &500_000i128);
        assert_eq!(s.client.token_balance(&s.token), 500_000);
        assert_eq!(s.client.total_collected(), 500_000);
    }

    #[test]
    fn test_collect_fee_accumulates_across_calls() {
        let s = setup();
        s.client
            .collect_fee(&s.market, &s.token, &1u32, &300_000i128);
        s.client
            .collect_fee(&s.market, &s.token, &2u32, &200_000i128);
        assert_eq!(s.client.token_balance(&s.token), 500_000);
        assert_eq!(s.client.total_collected(), 500_000);
    }

    #[test]
    fn test_collect_fee_accumulates_across_tokens() {
        let s = setup();
        let token2 = env_token(&s.env);

        s.client
            .collect_fee(&s.market, &s.token, &1u32, &100_000i128);
        s.client
            .collect_fee(&s.market, &token2, &1u32, &200_000i128);

        assert_eq!(s.client.token_balance(&s.token), 100_000);
        assert_eq!(s.client.token_balance(&token2), 200_000);
        assert_eq!(s.client.total_collected(), 300_000);
    }

    #[test]
    fn test_collect_fee_wrong_caller_fails() {
        let s = setup();
        let rando = Address::generate(&s.env);
        let result = s
            .client
            .try_collect_fee(&rando, &s.token, &1u32, &100i128);
        assert_eq!(result, Err(Ok(TreasuryError::CallerNotMarket)));
    }

    #[test]
    fn test_collect_fee_zero_amount_fails() {
        let s = setup();
        let result = s
            .client
            .try_collect_fee(&s.market, &s.token, &1u32, &0i128);
        assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
    }

    #[test]
    fn test_collect_fee_negative_amount_fails() {
        let s = setup();
        let result = s
            .client
            .try_collect_fee(&s.market, &s.token, &1u32, &(-1i128));
        assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
    }

    #[test]
    fn test_collect_fee_emits_event() {
        let s = setup();
        s.client
            .collect_fee(&s.market, &s.token, &42u32, &1_000i128);

        // env.events().all() returns events from the most recent top-level call
        // (collect_fee). The initialize call's events are not accumulated here.
        let events = s.env.events().all();
        assert_eq!(events.len(), 1);
        let (_, topics, data) = events.get(0).unwrap();

        let name: Symbol = topics.get(0).unwrap().into_val(&s.env);
        assert_eq!(name, Symbol::new(&s.env, "fee_collected_event"));

        let market_id: u32 = topics.get(1).unwrap().into_val(&s.env);
        assert_eq!(market_id, 42u32);

        let d: Map<Symbol, Val> = data.try_into_val(&s.env).unwrap();
        let fee: i128 = d
            .get(Symbol::new(&s.env, "fee_amount"))
            .unwrap()
            .into_val(&s.env);
        assert_eq!(fee, 1_000);
    }

    // ── withdraw_fees ─────────────────────────────────────────────────────────

    #[test]
    fn test_withdraw_fees_transfers_tokens_and_updates_balance() {
        let s = setup();
        fund_treasury(&s, 1_000_000);

        // First record the fee so accounting is consistent.
        s.client
            .collect_fee(&s.market, &s.token, &1u32, &1_000_000i128);

        let recipient = Address::generate(&s.env);
        s.client
            .withdraw_fees(&s.admin, &s.token, &recipient, &400_000i128);

        assert_eq!(s.client.token_balance(&s.token), 600_000);

        // Verify actual on-chain token balance of recipient.
        let token_client = soroban_sdk::token::Client::new(&s.env, &s.token);
        assert_eq!(token_client.balance(&recipient), 400_000);
    }

    #[test]
    fn test_withdraw_fees_insufficient_balance_fails() {
        let s = setup();
        // No tokens deposited → balance = 0.
        let result = s
            .client
            .try_withdraw_fees(&s.admin, &s.token, &s.admin, &1i128);
        assert_eq!(result, Err(Ok(TreasuryError::InsufficientBalance)));
    }

    #[test]
    fn test_withdraw_fees_zero_amount_fails() {
        let s = setup();
        let result = s
            .client
            .try_withdraw_fees(&s.admin, &s.token, &s.admin, &0i128);
        assert_eq!(result, Err(Ok(TreasuryError::InvalidAmount)));
    }

    #[test]
    fn test_withdraw_fees_unauthorized_fails() {
        let s = setup();
        fund_treasury(&s, 100);
        s.client
            .collect_fee(&s.market, &s.token, &1u32, &100i128);

        let rando = Address::generate(&s.env);
        let result = s
            .client
            .try_withdraw_fees(&rando, &s.token, &rando, &1i128);
        assert_eq!(result, Err(Ok(TreasuryError::Unauthorized)));
    }

    #[test]
    fn test_withdraw_fees_emits_event() {
        let s = setup();
        fund_treasury(&s, 500);
        s.client
            .collect_fee(&s.market, &s.token, &1u32, &500i128);

        let recipient = Address::generate(&s.env);
        s.client
            .withdraw_fees(&s.admin, &s.token, &recipient, &200i128);

        // env.events().all() returns events from the most recent top-level call
        // (withdraw_fees). That call emits: (1) SAC transfer event, (2) FeesWithdrawnEvent.
        let events = s.env.events().all();
        assert_eq!(events.len(), 2);

        let (_, topics, data) = events.get(1).unwrap();
        let name: Symbol = topics.get(0).unwrap().into_val(&s.env);
        assert_eq!(name, Symbol::new(&s.env, "fees_withdrawn_event"));

        let d: Map<Symbol, Val> = data.try_into_val(&s.env).unwrap();
        let amount: i128 = d
            .get(Symbol::new(&s.env, "amount"))
            .unwrap()
            .into_val(&s.env);
        assert_eq!(amount, 200);

        let remaining: i128 = d
            .get(Symbol::new(&s.env, "remaining_token_balance"))
            .unwrap()
            .into_val(&s.env);
        assert_eq!(remaining, 300);
    }

    // ── set_market_contract ───────────────────────────────────────────────────

    #[test]
    fn test_set_market_contract_updates_address() {
        let s = setup();
        let new_market = Address::generate(&s.env);
        s.client.set_market_contract(&s.admin, &new_market);
        assert_eq!(s.client.market_contract(), new_market);
    }

    #[test]
    fn test_set_market_contract_unauthorized_fails() {
        let s = setup();
        let rando = Address::generate(&s.env);
        let new_market = Address::generate(&s.env);
        let result = s.client.try_set_market_contract(&rando, &new_market);
        assert_eq!(result, Err(Ok(TreasuryError::Unauthorized)));
    }

    #[test]
    fn test_old_market_cannot_collect_fee_after_rotation() {
        let s = setup();
        let new_market = Address::generate(&s.env);
        s.client.set_market_contract(&s.admin, &new_market);

        // Old market address should now be rejected.
        let result = s
            .client
            .try_collect_fee(&s.market, &s.token, &1u32, &100i128);
        assert_eq!(result, Err(Ok(TreasuryError::CallerNotMarket)));

        // New market address should be accepted.
        s.client
            .collect_fee(&new_market, &s.token, &1u32, &100i128);
        assert_eq!(s.client.total_collected(), 100);
    }

    // ── Uninitialized guards ──────────────────────────────────────────────────

    #[test]
    fn test_collect_fee_on_uninitialized_treasury_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &id);
        let caller = Address::generate(&env);
        let token = Address::generate(&env);
        let result = client.try_collect_fee(&caller, &token, &1u32, &100i128);
        assert_eq!(result, Err(Ok(TreasuryError::NotInitialized)));
    }

    #[test]
    fn test_withdraw_fees_on_uninitialized_treasury_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(TreasuryContract, ());
        let client = TreasuryContractClient::new(&env, &id);
        let admin = Address::generate(&env);
        let token = Address::generate(&env);
        let result = client.try_withdraw_fees(&admin, &token, &admin, &100i128);
        assert_eq!(result, Err(Ok(TreasuryError::NotInitialized)));
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn env_token(env: &Env) -> Address {
        let admin = Address::generate(env);
        env.register_stellar_asset_contract_v2(admin).address()
    }
}
