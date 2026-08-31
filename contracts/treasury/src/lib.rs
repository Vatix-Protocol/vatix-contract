// Issue #765: Required no_std attribute for Soroban WASM contract execution
#![no_std]
#![warn(clippy::all)]

//! # Treasury Contract
//!
//! Collects and custodies protocol fees on behalf of the Vatix prediction
//! market protocol. Any address in the authorized market registry may deposit
//! fees via [`TreasuryContract::collect_fee`]; the **admin** controls all
//! other privileged operations (withdrawal, registry management).
//!
//! ## Authorization model
//!
//! | Operation                          | Who may call              |
//! |------------------------------------|---------------------------|
//! | `initialize`                       | anyone (once)             |
//! | `collect_fee`                      | registered market contract|
//! | `withdraw_fees`                    | admin                     |
//! | `add_market` / `remove_market`     | admin                     |
//! | `propose_market_contract` / `execute_market_contract` | admin (propose), timelock (execute) |
//! | `propose_stakeholders` / `execute_stakeholders` | admin (propose), timelock (execute) |
//! | `distribute_fees`                  | admin                     |
//! | Getters                            | anyone                    |
//!
//! ## Storage layout
//!
//! | Key                       | Type                  | Description                               |
//! |---------------------------|-----------------------|-------------------------------------------|
//! | `StorageVersion`          | `u32`                 | Schema version guard                      |
//! | `Admin`                   | `Address`             | Protocol admin                            |
//! | `PendingAdmin`            | `PendingAddressChange`| Nominated admin awaiting timelock (#658)  |
//! | `AuthorizedMarkets`       | `Vec<Address>`        | Market contracts allowed to call `collect_fee` |
//! | `PendingMarketContract`   | `PendingAddressChange`| Proposed market contract awaiting timelock |
//! | `TokenBalance(Address)`   | `i128`                | Current custodied balance per token (decreasable) |
//! | `CumulativeFees(Address)` | `i128`                | Historical total collected per token (monotone)   |
//! | `TotalCollected`          | `i128`                | Global monotone counter across all tokens |
//! | `Paused`                  | `bool`                | Blocks `collect_fee`/`withdraw_fees` until unpaused |
//! | `Stakeholders`            | `Vec<(Address, u32)>` | Revenue-share list, `share_bps` sums to 10_000 (#485) |
//! | `PendingStakeholders`     | `PendingStakeholders` | Proposed stakeholder list awaiting timelock (#689) |
//! | `FeeTokens`               | `Vec<Address>`        | Registry of every token ever collected (#484) |
//! | `EmergencyMode`           | `EmergencyMode`       | Coordinated mode mirrored with Market/Resolution (#662) |
//!
//! See [`docs/treasury-storage.md`](../../../docs/treasury-storage.md) for
//! full descriptions, storage tiers, and the reviewer checklist that keeps
//! this table, that document, and the `StorageKey` enum itself in lockstep
//! (#722).

pub mod error;
pub mod events;
pub mod storage;
#[cfg(test)]
mod test;
#[cfg(test)]
mod distribute_proptest;

pub use error::TreasuryError;

use soroban_sdk::{contract, contractimpl, token, Address, Env, Vec};

/// Basis-point denominator: stakeholder shares must sum to exactly this value.
const BPS_DENOMINATOR: i128 = 10_000;

/// Uniform timelock delay for privileged address mutations
pub const ADDRESS_TIMELOCK_SECONDS: u64 = 172_800;

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    // ── Lifecycle ──────────────────────────────────────────────────────────────

    /// Bootstrap the treasury with an initial market contract in the registry.
    pub fn initialize(
        env: Env,
        admin: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        // Reject contract addresses as admin before anything else.
        // A contract admin can be called without a real key owner's consent
        // and would allow privilege escalation.
        if admin.executable().is_some() {
            return Err(TreasuryError::InvalidAdmin);
        }
        admin.require_auth();
        if storage::has_admin(&env) {
            return Err(TreasuryError::AlreadyInitialized);
        }
        storage::set_admin(&env, &admin);
        let markets = soroban_sdk::vec![&env, market_contract.clone()];
        storage::set_authorized_markets(&env, &markets);
        storage::set_version(&env);
        events::emit_treasury_initialized(&env, &admin, &market_contract);
        Ok(())
    }

    // ── Fee collection ─────────────────────────────────────────────────────────

    /// Record a protocol fee transferred from any registered market contract.
    ///
    /// `token` identifies which token mint the fee was paid in (#484): the
    /// treasury custodies an independent balance per token, so markets using
    /// different collateral tokens can all route fees through the same
    /// treasury deployment without their balances colliding.
    pub fn collect_fee(
        env: Env,
        caller: Address,
        token: Address,
        market_id: u32,
        fee_amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        // Emergency mode: fee collection is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
        if !storage::is_authorized_market(&env, &caller) {
            return Err(TreasuryError::CallerNotMarket);
        }
        if fee_amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        // Track every token we've ever seen so callers can enumerate the full
        // set of fee-bearing tokens without prior knowledge (#484).
        storage::register_fee_token(&env, &token);

        let prev_balance = storage::get_token_balance(&env, &token)?;
        let new_balance = prev_balance.checked_add(fee_amount).unwrap_or(i128::MAX);
        storage::set_token_balance(&env, &token, new_balance);

        let prev_cumulative = storage::get_cumulative_fees(&env, &token)?;
        let new_cumulative = prev_cumulative
            .checked_add(fee_amount)
            .unwrap_or(i128::MAX);
        storage::set_cumulative_fees(&env, &token, new_cumulative);

        let prev_total = storage::get_total_collected(&env)?;
        storage::set_total_collected(
            &env,
            prev_total.checked_add(fee_amount).unwrap_or(i128::MAX),
        );

        events::emit_fee_collected(
            &env,
            market_id,
            &token,
            fee_amount,
            new_balance,
            new_cumulative,
        );
        Ok(())
    }

    // ── Admin operations ───────────────────────────────────────────────────────

    /// Withdraw accumulated fees to a recipient address.
    pub fn withdraw_fees(
        env: Env,
        caller: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        // Emergency mode: fee withdrawal is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if amount <= 0 {
            return Err(TreasuryError::InvalidAmount);
        }

        let balance = storage::get_token_balance(&env, &token)?;
        if amount > balance {
            return Err(TreasuryError::InsufficientBalance);
        }

        // Effects before Interactions (Checks-Effects-Interactions, Issue
        // #695): persist the reduced balance and cumulative total BEFORE the
        // external token transfer below. Previously the transfer ran first
        // and both storage writes happened after it, so a reentrant call
        // back into a balance-reading entry point mid-transfer would have
        // observed the stale, not-yet-decremented balance.
        let remaining = balance - amount;
        storage::set_token_balance(&env, &token, remaining);

        let prev_total = storage::get_total_collected(&env)?;
        storage::set_total_collected(&env, prev_total.checked_sub(amount).unwrap_or(0));

        let treasury = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&treasury, &to, &amount);

        events::emit_fees_withdrawn(&env, &token, &to, amount, remaining);
        Ok(())
    }

    pub fn propose_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let effective_at = env.ledger().timestamp() + ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_admin(
            &env,
            &storage::PendingAddressChange {
                new_address: new_admin.clone(),
                effective_at,
            },
        );

        events::emit_admin_transfer_proposed(&env, &admin, &new_admin, effective_at);
        Ok(())
    }

    pub fn execute_admin(env: Env) -> Result<Address, TreasuryError> {
        let pending = storage::get_pending_admin(&env)
            .ok_or(TreasuryError::Unauthorized)?; // Using Unauthorized as fallback for now

        if env.ledger().timestamp() < pending.effective_at {
            return Err(TreasuryError::Unauthorized); // TimelockNotElapsed
        }

        let current_admin = storage::get_admin(&env)?;
        storage::set_admin(&env, &pending.new_address);
        storage::clear_pending_admin(&env);

        events::emit_admin_transferred(&env, &current_admin, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_admin(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::clear_pending_admin(&env);
        Ok(())
    }

    /// Register an additional market contract allowed to call `collect_fee`.
    ///
    /// Idempotent — adding an already-registered market is a no-op. Only the
    /// admin may call this.
    pub fn add_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
            events::emit_market_added(&env, &market_contract);
        }
        Ok(())
    }

    /// Deregister a market contract, revoking its ability to call `collect_fee`.
    ///
    /// Removing an unknown market is a no-op that preserves the existing
    /// registry contents.
    ///
    /// Only the admin may call this.
    pub fn remove_market(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            return Ok(());
        }
        let mut updated = Vec::new(&env);
        for m in markets.iter() {
            if m != market_contract {
                updated.push_back(m);
            }
        }
        storage::set_authorized_markets(&env, &updated);
        events::emit_market_removed(&env, &market_contract);
        Ok(())
    }

    pub fn propose_market_contract(
        env: Env,
        caller: Address,
        new_market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let effective_at = env.ledger().timestamp() + ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_market_contract(
            &env,
            &storage::PendingAddressChange {
                new_address: new_market_contract.clone(),
                effective_at,
            },
        );

        events::emit_market_contract_proposed(&env, &new_market_contract, effective_at);
        Ok(())
    }

    pub fn execute_market_contract(env: Env) -> Result<Address, TreasuryError> {
        let pending = storage::get_pending_market_contract(&env)
            .ok_or(TreasuryError::Unauthorized)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(TreasuryError::Unauthorized);
        }

        // Append rather than replace (#720): this entrypoint and
        // `add_market`/`remove_market` both mutate the single
        // `AuthorizedMarkets` registry. Overwriting it with a fresh
        // single-element vec here used to silently deregister every other
        // market added via `add_market` — including markets still live and
        // routing fees through this treasury — with no `market_removed`
        // event to signal the drop. Matching `add_market`'s idempotent
        // append keeps the two entrypoints consistent with one registry.
        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&pending.new_address) {
            markets.push_back(pending.new_address.clone());
            storage::set_authorized_markets(&env, &markets);
        }
        storage::clear_pending_market_contract(&env);

        events::emit_market_contract_set(&env, &pending.new_address);
        Ok(pending.new_address)
    }

    pub fn cancel_market_contract(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        storage::clear_pending_market_contract(&env);
        Ok(())
    }

    /// Update or register the authorized market contract address directly (admin only).
    pub fn set_market_contract(
        env: Env,
        caller: Address,
        market_contract: Address,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();

        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let mut markets = storage::get_authorized_markets(&env);
        if !markets.contains(&market_contract) {
            markets.push_back(market_contract.clone());
            storage::set_authorized_markets(&env, &markets);
        }
        events::emit_market_contract_set(&env, &market_contract);
        Ok(())
    }

    /// Pause the treasury, blocking `collect_fee` and `withdraw_fees`.
    pub fn pause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, true);
        events::emit_treasury_paused(&env, &caller);
        Ok(())
    }

    /// Unpause the treasury, restoring normal operation.
    pub fn unpause(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_paused(&env, false);
        events::emit_treasury_unpaused(&env, &caller);
        Ok(())
    }

    /// Set the mirrored emergency mode (Issue #662).
    ///
    /// Only the treasury admin may call this. Operators should keep this value
    /// in sync with the Market and Resolution contracts for coordinated behaviour.
    pub fn set_emergency_mode(
        env: Env,
        caller: Address,
        new_mode: storage::EmergencyMode,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::set_emergency_mode(&env, &new_mode);
        events::emit_emergency_mode_changed(&env, &new_mode, &caller);
        Ok(())
    }

    /// Return the current mirrored emergency mode.
    pub fn get_emergency_mode(env: Env) -> storage::EmergencyMode {
        storage::get_emergency_mode(&env)
    }

    // ── Stakeholder fee distribution (#485) ────────────────────────────────────

    /// Propose a new stakeholder revenue-share list, subject to the same
    /// `ADDRESS_TIMELOCK_SECONDS` delay used by [`Self::propose_admin`] /
    /// [`Self::propose_market_contract`] (Issue #689). Admin only.
    ///
    /// `stakeholders` is a list of `(address, share_bps)` pairs. `share_bps`
    /// values must sum to exactly 10_000 (100%); once executed this fully
    /// replaces any previously configured list. The full payload is emitted
    /// on [`StakeholdersProposed`](events::StakeholdersProposed) so an
    /// off-chain indexer can reconstruct the pending split without an extra
    /// on-chain read.
    pub fn propose_stakeholders(
        env: Env,
        caller: Address,
        stakeholders: Vec<(Address, u32)>,
    ) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        if stakeholders.is_empty() {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let mut total: i128 = 0;
        for (_, share_bps) in stakeholders.iter() {
            total = total
                .checked_add(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        }
        if total != BPS_DENOMINATOR {
            return Err(TreasuryError::InvalidStakeholderWeights);
        }

        let effective_at = env.ledger().timestamp() + ADDRESS_TIMELOCK_SECONDS;
        storage::set_pending_stakeholders(
            &env,
            &storage::PendingStakeholders {
                stakeholders: stakeholders.clone(),
                effective_at,
            },
        );

        let mut addrs: Vec<Address> = Vec::new(&env);
        let mut shares: Vec<u32> = Vec::new(&env);
        for (addr, share_bps) in stakeholders.iter() {
            addrs.push_back(addr.clone());
            shares.push_back(share_bps);
        }
        events::emit_stakeholders_proposed(&env, &addrs, &shares, effective_at);
        Ok(())
    }

    /// Apply a previously-proposed stakeholder list once its timelock has
    /// elapsed (Issue #689). Callable by anyone — the timelock itself is the
    /// access control.
    pub fn execute_stakeholders(env: Env) -> Result<(), TreasuryError> {
        let pending = storage::get_pending_stakeholders(&env)
            .ok_or(TreasuryError::NoPendingStakeholderChange)?;

        if env.ledger().timestamp() < pending.effective_at {
            return Err(TreasuryError::TimelockNotElapsed);
        }

        storage::set_stakeholders(&env, &pending.stakeholders);
        storage::clear_pending_stakeholders(&env);

        let mut addrs: Vec<Address> = Vec::new(&env);
        let mut shares: Vec<u32> = Vec::new(&env);
        for (addr, share_bps) in pending.stakeholders.iter() {
            addrs.push_back(addr.clone());
            shares.push_back(share_bps);
        }
        events::emit_stakeholders_updated(&env, &addrs, &shares);
        Ok(())
    }

    /// Cancel a pending stakeholder list change before it takes effect.
    pub fn cancel_stakeholders(env: Env, caller: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }
        storage::clear_pending_stakeholders(&env);
        Ok(())
    }

    /// Return the currently pending stakeholder change, if any (Issue #689).
    pub fn get_pending_stakeholders(env: Env) -> Option<storage::PendingStakeholders> {
        storage::get_pending_stakeholders(&env)
    }

    /// Return the configured `(stakeholder, share_bps)` list.
    pub fn get_stakeholders(env: Env) -> Result<Vec<(Address, u32)>, TreasuryError> {
        storage::get_stakeholders(&env)
    }

    /// Distribute the treasury's current `token` balance to the configured
    /// stakeholders, proportionally to their `share_bps` weight (admin only).
    ///
    /// Each stakeholder receives `floor(balance * share_bps / 10_000)`. Any
    /// integer-division remainder (dust) stays in the treasury balance and
    /// rolls into the next distribution.
    ///
    /// # Dust remainder (issue #688)
    /// Because each share is floor-divided, `sum(amount)` across all
    /// stakeholders can be up to `stakeholders.len() - 1` stroops less than
    /// `balance`. That leftover is **not** dropped: `remaining = balance -
    /// distributed` is written back to `TokenBalance(token)` before any
    /// transfer is made, so it is simply carried forward and gets
    /// distributed — proportionally, like any other collected fee — the
    /// next time `distribute_fees` runs for this token.
    ///
    /// # CEI ordering (issue #688)
    /// All per-stakeholder amounts are computed and the treasury's own
    /// balance is persisted (`storage::set_token_balance`) *before* any
    /// external `token_client.transfer` call is made, per
    /// Checks-Effects-Interactions (see `docs/reentrancy-cei-audit.md`).
    /// This closes a reentrancy window where a malicious/upgraded token
    /// contract's `transfer` callback could otherwise re-enter
    /// `distribute_fees` while the old (undecremented) balance was still
    /// visible in storage.
    ///
    /// # Errors
    /// - [`TreasuryError::NotInitialized`] – treasury not initialized.
    /// - [`TreasuryError::ContractPaused`] – treasury is paused.
    /// - [`TreasuryError::Unauthorized`] – caller is not the admin.
    /// - [`TreasuryError::NoStakeholdersConfigured`] – `propose_stakeholders`
    ///   / `execute_stakeholders` has never installed a list.
    /// - [`TreasuryError::InsufficientBalance`] – the current `token` balance is zero.
    pub fn distribute_fees(env: Env, caller: Address, token: Address) -> Result<(), TreasuryError> {
        caller.require_auth();
        if !storage::has_admin(&env) {
            return Err(TreasuryError::NotInitialized);
        }
        if storage::is_paused(&env) {
            return Err(TreasuryError::ContractPaused);
        }
        // Emergency mode: fee distribution is blocked only in GlobalFreeze
        require_emergency_mode_allows(
            &env,
            &[
                storage::EmergencyMode::Normal,
                storage::EmergencyMode::TradingHalted,
                storage::EmergencyMode::SettleOnly,
            ],
        )?;
        let admin = storage::get_admin(&env)?;
        if caller != admin {
            return Err(TreasuryError::Unauthorized);
        }

        let stakeholders = storage::get_stakeholders(&env)?;
        if stakeholders.is_empty() {
            return Err(TreasuryError::NoStakeholdersConfigured);
        }

        let balance = storage::get_token_balance(&env, &token)?;
        if balance <= 0 {
            return Err(TreasuryError::InsufficientBalance);
        }

        // Effects before Interactions (Checks-Effects-Interactions, Issue
        // #695): compute every stakeholder's payout and persist the reduced
        // balance BEFORE making any external transfer. Previously each
        // transfer fired inside the same loop that accumulated
        // `distributed`, with storage::set_token_balance only updated after
        // every transfer had already gone out — a reentrant call back into
        // a balance-reading entry point mid-loop would have observed the
        // stale, not-yet-decremented balance.
        // Each stakeholder must appear exactly once in `payouts` — a second
        // push here would double the real token transfer below while
        // `distributed`/`remaining` still accounted for only one, silently
        // overpaying every stakeholder by 2x relative to the treasury's own
        // ledger (#721).
        let mut payouts: Vec<(Address, i128)> = Vec::new(&env);
        let mut distributed: i128 = 0;
        for (stakeholder, share_bps) in stakeholders.iter() {
            let amount = balance
                .checked_mul(share_bps as i128)
                .ok_or(TreasuryError::ArithmeticOverflow)?
                / BPS_DENOMINATOR;
            if amount > 0 {
                payouts.push_back((stakeholder, amount));
                distributed = distributed
                    .checked_add(amount)
                    .ok_or(TreasuryError::ArithmeticOverflow)?;
            }
        }

        // Floor-division dust remainder: `balance - distributed` is whatever
        // is left after every stakeholder's basis-point share is rounded
        // down. It is credited straight back into the treasury's own
        // `TokenBalance(token)` below (not dropped, and not sent to any one
        // stakeholder), so it simply rolls forward and is redistributed —
        // proportionally, same as any other collected fee — the next time
        // `distribute_fees` is called for this token.
        let remaining = balance - distributed;
        storage::set_token_balance(&env, &token, remaining);

        let treasury = env.current_contract_address();
        let token_client = token::Client::new(&env, &token);
        for (stakeholder, amount) in payouts.iter() {
            token_client.transfer(&treasury, &stakeholder, &amount);
        }

        events::emit_fees_distributed(&env, &token, distributed, remaining, stakeholders.len());
        Ok(())
    }

    // ── Getters ────────────────────────────────────────────────────────────────

    /// Return whether the treasury is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    /// Return the admin address.
    pub fn admin(env: Env) -> Result<Address, TreasuryError> {
        storage::get_admin(&env)
    }

    /// Return the primary registered market contract address (the first entry
    /// in the authorized-markets registry). Returns `NotInitialized` if no
    /// market has ever been registered.
    pub fn market_contract(env: Env) -> Result<Address, TreasuryError> {
        storage::get_authorized_markets(&env)
            .get(0)
            .ok_or(TreasuryError::NotInitialized)
    }

    /// Return whether `market` is currently authorized to call `collect_fee`.
    pub fn is_authorized_market(env: Env, market: Address) -> bool {
        storage::is_authorized_market(&env, &market)
    }

    /// Return every market contract currently authorized to call `collect_fee`.
    pub fn list_markets(env: Env) -> Result<Vec<Address>, TreasuryError> {
        Ok(storage::get_authorized_markets(&env))
    }

    /// Return every distinct token mint that has ever had a fee collected for it (#484).
    pub fn list_fee_tokens(env: Env) -> Vec<Address> {
        storage::get_fee_tokens(&env)
    }

    /// Return the current custodied balance for `token` (decreases on withdrawal).
    pub fn token_balance(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_token_balance(&env, &token)
    }

    /// Return the per-token cumulative fees collected for `token` since deployment.
    pub fn get_cumulative_fees(env: Env, token: Address) -> Result<i128, TreasuryError> {
        storage::get_cumulative_fees(&env, &token)
    }

    /// Return the global cumulative fees collected across all tokens since deployment.
    pub fn total_collected(env: Env) -> Result<i128, TreasuryError> {
        storage::get_total_collected(&env)
    }
}

/// Guard: reject operations that are not permitted under the current emergency
/// mode (Issue #662).
///
/// `allowed_modes` specifies the set of modes under which the guarded operation
/// is permitted. If the current mode is not in this set, the call is rejected
/// with [`TreasuryError::EmergencyModeActive`].
fn require_emergency_mode_allows(
    env: &Env,
    allowed_modes: &[storage::EmergencyMode],
) -> Result<(), TreasuryError> {
    let current = storage::get_emergency_mode(env);
    if !allowed_modes.contains(&current) {
        return Err(TreasuryError::EmergencyModeActive);
    }
    Ok(())
}
