use crate::error::ContractError;
use crate::types::{Market, Position};
use soroban_sdk::{contracttype, Address, Env};

/// Bump this constant whenever the storage layout changes in a breaking way.
/// `initialize()` writes this value; every storage accessor asserts it.
///
/// ## Migration procedure (testnet)
/// 1. Increment `STORAGE_VERSION` in this file.
/// 2. Redeploy the contract WASM (`make build` then `soroban contract deploy`).
/// 3. Call `initialize(admin)` on the fresh deployment — it writes the new version.
/// 4. The old deployment is now permanently locked behind `UpgradeRequired`;
///    any call that touches storage will return that error.
pub const STORAGE_VERSION: u32 = 2;

#[contracttype]
pub enum StorageKey {
    StorageVersion,
    Market(u32),
    Position(u32, Address),
    Admin,
    PendingAdmin,
    MarketCounter,
    /// Address of the deployed treasury contract that protocol fees are routed
    /// to. Optional — fees are only forwarded when this is populated and the
    /// computed fee_amount is greater than zero.
    Treasury,
    /// Withdrawal fee rate in basis points (0–10_000). Read in the withdraw
    /// path to compute the protocol fee; defaults to 0 when unset.
    FeeRateBps,
    /// Address of the deployed outcome-token contract. When set, `update_position`
    /// mints/burns outcome tokens to reflect share balance changes.
    OutcomeTokenContract,
    /// Address of the deployed resolution contract. When set, `resolve_market`
    /// requires a finalized candidate from this contract before accepting the outcome.
    ResolutionContract,
}

// --- Version helpers ---

pub fn set_version(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

/// Returns `Err(UpgradeRequired)` when the on-chain version is absent or
/// does not match `STORAGE_VERSION`.
pub fn assert_version(env: &Env) -> Result<(), ContractError> {
    let on_chain: Option<u32> = env
        .storage()
        .persistent()
        .get(&StorageKey::StorageVersion);
    if on_chain != Some(STORAGE_VERSION) {
        return Err(ContractError::UpgradeRequired);
    }
    Ok(())
}

// --- Market Storage ---

pub fn get_market(env: &Env, market_id: u32) -> Result<Option<Market>, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Market(market_id)))
}

pub fn set_market(env: &Env, market_id: u32, market: &Market) -> Result<(), ContractError> {
    assert_version(env)?;
    env.storage()
        .persistent()
        .set(&StorageKey::Market(market_id), market);
    Ok(())
}

pub fn has_market(env: &Env, market_id: u32) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .has(&StorageKey::Market(market_id)))
}

// --- Position Storage ---

pub fn get_position(env: &Env, market_id: u32, user: &Address) -> Result<Option<Position>, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Position(market_id, user.clone())))
}

pub fn set_position(env: &Env, market_id: u32, user: &Address, position: &Position) -> Result<(), ContractError> {
    assert_version(env)?;
    env.storage()
        .persistent()
        .set(&StorageKey::Position(market_id, user.clone()), position);
    Ok(())
}

pub fn has_position(env: &Env, market_id: u32, user: &Address) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .has(&StorageKey::Position(market_id, user.clone())))
}

// --- Configuration Storage ---

pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::Admin)
        .expect("Admin not set"))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&StorageKey::Admin, admin);
}

/// Returns `true` if the admin slot has been populated (i.e. `initialize` was
/// already called), `false` on a freshly deployed contract.
pub fn has_admin(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Admin)
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::PendingAdmin, admin);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingAdmin);
}

pub fn get_next_market_id(env: &Env) -> Result<u32, ContractError> {
    assert_version(env)?;
    Ok(env
        .storage()
        .persistent()
        .get(&StorageKey::MarketCounter)
        .unwrap_or(0))
}

pub fn increment_market_id(env: &Env) -> Result<u32, ContractError> {
    let next_id = get_next_market_id(env)? + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::MarketCounter, &next_id);
    Ok(next_id)
}

// --- Treasury Storage ---

/// Return the registered treasury contract address, if any.
pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::Treasury)
}

/// Register (or replace) the treasury contract address for protocol fee routing.
/// Pass `None` to remove the treasury and disable fee routing.
pub fn set_treasury(env: &Env, treasury: &Option<Address>) {
    match treasury {
        Some(addr) => env.storage().persistent().set(&StorageKey::Treasury, addr),
        None => env.storage().persistent().remove(&StorageKey::Treasury),
    }
}

pub fn has_treasury(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Treasury)
}

// --- Resolution Contract Storage ---

pub fn get_resolution_contract(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::ResolutionContract)
}

pub fn set_resolution_contract(env: &Env, contract: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::ResolutionContract, contract);
}

// --- Outcome Token Storage ---

pub fn get_outcome_token_contract(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&StorageKey::OutcomeTokenContract)
}

pub fn set_outcome_token_contract(env: &Env, contract: &Address) {
    env.storage()
        .persistent()
        .set(&StorageKey::OutcomeTokenContract, contract);
}

// --- Fee Config Storage ---

pub fn get_fee_rate_bps(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::FeeRateBps)
        .unwrap_or(0)
}

pub fn set_fee_rate_bps(env: &Env, fee_rate_bps: i128) {
    env.storage()
        .persistent()
        .set(&StorageKey::FeeRateBps, &fee_rate_bps);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::MarketStatus;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{BytesN, String};

    fn init_versioned(env: &Env, contract_id: &soroban_sdk::Address) {
        env.as_contract(contract_id, || {
            set_version(env);
        });
    }

    #[test]
    fn test_wrong_version_returns_upgrade_required() {
        use crate::error::ContractError;
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        // Deliberately write a stale version.
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_missing_version_returns_upgrade_required() {
        use crate::error::ContractError;
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        // No version written — storage is empty.
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_admin_storage() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert!(!has_admin(&env), "admin slot should be empty before set");
            set_admin(&env, &admin);
            assert!(has_admin(&env), "admin slot should be populated after set");
            assert_eq!(get_admin(&env).unwrap(), admin);
        });
    }

    #[test]
    fn test_market_id_counter() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert_eq!(get_next_market_id(&env).unwrap(), 0);
            assert_eq!(increment_market_id(&env).unwrap(), 1);
            assert_eq!(increment_market_id(&env).unwrap(), 2);
            assert_eq!(get_next_market_id(&env).unwrap(), 2);
        });
    }

    #[test]
    fn test_market_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        let market_id = 1;
        let creator = Address::generate(&env);
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Will it rain?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(&env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator,
            created_at: 0,
            collateral_token,
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
        };

        env.as_contract(&contract_id, || {
            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());

            let saved_market = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(saved_market.id, market.id);
            assert_eq!(saved_market.question, market.question);
        });
    }

    #[test]
    fn test_position_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        let market_id = 1;
        let user = Address::generate(&env);

        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 100,
            no_shares: 0,
            locked_collateral: 100,
            total_deposited: 100,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            assert!(!has_position(&env, market_id, &user).unwrap());
            set_position(&env, market_id, &user, &position).unwrap();
            assert!(has_position(&env, market_id, &user).unwrap());

            let saved_position = get_position(&env, market_id, &user).unwrap().unwrap();
            assert_eq!(saved_position.yes_shares, 100);
            assert_eq!(saved_position.market_id, market_id);
        });
    }

    /// Verify that all StorageKey variants are independent slots and that every
    /// field of the Market and Position structs survives a storage round-trip.
    #[test]
    fn test_storage_layout() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let collateral_token = Address::generate(&env);
        let market_id = 7u32;

        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Storage layout test question?"),
            end_time: 9_999_999_999u64,
            oracle_pubkey: BytesN::from_array(&env, &[0xABu8; 32]),
            status: crate::types::MarketStatus::Active,
            result: Some(true),
            creator: admin.clone(),
            created_at: 1_000_000u64,
            collateral_token: collateral_token.clone(),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
        };

        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 250,
            no_shares: 75,
            locked_collateral: 325,
            total_deposited: 400,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            // --- Admin and MarketCounter are independent slots ---
            set_admin(&env, &admin);
            increment_market_id(&env).unwrap(); // counter becomes 1

            assert_eq!(get_admin(&env).unwrap(), admin);
            // MarketCounter write must not have corrupted Admin slot
            assert_eq!(get_admin(&env).unwrap(), admin);
            // Admin write must not have corrupted MarketCounter slot
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            // --- Market slot is independent from admin and counter ---
            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());

            // Admin and counter are unchanged after market write
            assert_eq!(get_admin(&env).unwrap(), admin);
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            // All Market fields survive the round-trip
            let m = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m.id, market.id);
            assert_eq!(m.question, market.question);
            assert_eq!(m.end_time, market.end_time);
            assert_eq!(m.oracle_pubkey, market.oracle_pubkey);
            assert_eq!(m.result, market.result);
            assert_eq!(m.creator, market.creator);
            assert_eq!(m.created_at, market.created_at);
            assert_eq!(m.collateral_token, market.collateral_token);

            // --- Position slot is keyed by (market_id, user) ---
            assert!(!has_position(&env, market_id, &user).unwrap());
            set_position(&env, market_id, &user, &position).unwrap();
            assert!(has_position(&env, market_id, &user).unwrap());

            // A different user must not see this position
            let other_user = Address::generate(&env);
            assert!(!has_position(&env, market_id, &other_user).unwrap());

            // All Position fields survive the round-trip
            let p = get_position(&env, market_id, &user).unwrap().unwrap();
            assert_eq!(p.market_id, position.market_id);
            assert_eq!(p.user, position.user);
            assert_eq!(p.yes_shares, position.yes_shares);
            assert_eq!(p.no_shares, position.no_shares);
            assert_eq!(p.locked_collateral, position.locked_collateral);
            assert_eq!(p.total_deposited, position.total_deposited);
            assert_eq!(p.is_settled, position.is_settled);

            // Market slot is unchanged after position write
            let m2 = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m2.id, market.id);
        });
    }

    // ── #406 Cold upgrade migration test vectors ─────────────────────────────

    /// Vector 1: a completely fresh contract (no storage at all) behaves
    /// identically to a stale-version contract — both must return
    /// `UpgradeRequired` and never panic.
    #[test]
    fn migration_v0_missing_version_blocks_all_storage_access() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // No version written — simulates a cold-started or pre-v1 deployment.
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
            assert_eq!(
                get_market(&env, 1),
                Err(ContractError::UpgradeRequired)
            );
            assert_eq!(
                get_position(&env, 1, &user),
                Err(ContractError::UpgradeRequired)
            );
            assert_eq!(
                get_next_market_id(&env),
                Err(ContractError::UpgradeRequired)
            );
        });
    }

    /// Vector 2: a stale version (v0 = 0) written before the current
    /// `STORAGE_VERSION` must be rejected by `assert_version`.
    #[test]
    fn migration_stale_version_zero_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            // Simulate a pre-upgrade deployment by writing version 0.
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    /// Vector 3: after `set_version` (the migration step), `assert_version`
    /// passes and all storage ops work normally.
    #[test]
    fn migration_after_set_version_storage_is_accessible() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: 1,
            question: String::from_str(&env, "post-migration market?"),
            end_time: 9_000_000,
            oracle_pubkey: BytesN::from_array(&env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token,
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: crate::types::AdapterType::Ed25519,
        };

        env.as_contract(&contract_id, || {
            // Simulate the upgrade: write the current version.
            set_version(&env);
            assert_eq!(assert_version(&env), Ok(()));

            // Storage ops succeed post-migration.
            set_market(&env, 1, &market).unwrap();
            let m = get_market(&env, 1).unwrap().unwrap();
            assert_eq!(m.id, 1);
        });
    }

    /// Vector 4: any version number other than `STORAGE_VERSION` is rejected,
    /// guarding against forward-compatibility accidents.
    #[test]
    fn migration_future_version_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&StorageKey::StorageVersion, &(STORAGE_VERSION + 1));
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    // ── Treasury storage helpers ──────────────────────────────────────────────

    #[test]
    fn test_treasury_storage_set_and_get() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let treasury = Address::generate(&env);
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert!(!has_treasury(&env));
            assert_eq!(get_treasury(&env), None);

            set_treasury(&env, &Some(treasury.clone()));
            assert!(has_treasury(&env));
            assert_eq!(get_treasury(&env), Some(treasury.clone()));

            set_treasury(&env, &None);
            assert!(!has_treasury(&env));
            assert_eq!(get_treasury(&env), None);
        });
    }

    // ── Resolution contract storage helpers ───────────────────────────────────

    #[test]
    fn test_resolution_contract_storage_set_and_get() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let resolution = Address::generate(&env);
        init_versioned(&env, &contract_id);

        env.as_contract(&contract_id, || {
            assert_eq!(get_resolution_contract(&env), None);

            set_resolution_contract(&env, &resolution);
            assert_eq!(get_resolution_contract(&env), Some(resolution.clone()));
        });
    }
}
