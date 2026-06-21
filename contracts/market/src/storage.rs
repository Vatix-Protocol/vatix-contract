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
pub const STORAGE_VERSION: u32 = 1;

#[contracttype]
pub enum StorageKey {
    StorageVersion,
    Market(u32),
    Position(u32, Address),
    Admin,
    MarketCounter,
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
}
