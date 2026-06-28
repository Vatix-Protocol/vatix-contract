use crate::error::ContractError;
use crate::types::{Market, Position};
use soroban_sdk::{contracttype, Address, BytesN, Env, Vec};

/// Bump this constant whenever the storage layout changes in a breaking way.
pub const STORAGE_VERSION: u32 = 2;

#[contracttype]
pub enum StorageKey {
    StorageVersion,
    Market(u32),
    Position(u32, Address),
    Admin,
    PendingAdmin,
    MarketCounter,
    /// Withdrawal fee rate in basis points (0–10_000). Defaults to 0 when unset.
    FeeRateBps,
    /// Address of the deployed treasury contract for protocol fee routing.
    Treasury,
    /// Address of the deployed outcome-token contract.
    OutcomeTokenContract,
    /// Address of the deployed resolution contract that gates resolve_market.
    ResolutionContract,
    /// Ordered list of oracle public keys forming the multi-signer quorum (#378).
    ThresholdSigners,
    /// Minimum number of valid signatures required to resolve a market (#378).
    ThresholdQuorum,
}

// --- Version helpers ---

pub fn set_version(env: &Env) {
    env.storage()
        .persistent()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION);
}

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
    Ok(env.storage().persistent().get(&StorageKey::Market(market_id)))
}

pub fn set_market(env: &Env, market_id: u32, market: &Market) -> Result<(), ContractError> {
    assert_version(env)?;
    env.storage().persistent().set(&StorageKey::Market(market_id), market);
    Ok(())
}

pub fn has_market(env: &Env, market_id: u32) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().has(&StorageKey::Market(market_id)))
}

// --- Position Storage ---

pub fn get_position(env: &Env, market_id: u32, user: &Address) -> Result<Option<Position>, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().get(&StorageKey::Position(market_id, user.clone())))
}

pub fn set_position(env: &Env, market_id: u32, user: &Address, position: &Position) -> Result<(), ContractError> {
    assert_version(env)?;
    env.storage().persistent().set(&StorageKey::Position(market_id, user.clone()), position);
    Ok(())
}

pub fn has_position(env: &Env, market_id: u32, user: &Address) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().has(&StorageKey::Position(market_id, user.clone())))
}

// --- Admin Storage ---

pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().get(&StorageKey::Admin).expect("Admin not set"))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&StorageKey::Admin, admin);
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Admin)
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&StorageKey::PendingAdmin, admin);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().persistent().remove(&StorageKey::PendingAdmin);
}

// --- Market Counter ---

pub fn get_next_market_id(env: &Env) -> Result<u32, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().get(&StorageKey::MarketCounter).unwrap_or(0))
}

pub fn increment_market_id(env: &Env) -> Result<u32, ContractError> {
    let next_id = get_next_market_id(env)? + 1;
    env.storage().persistent().set(&StorageKey::MarketCounter, &next_id);
    Ok(next_id)
}

// --- Treasury Storage ---

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().persistent().set(&StorageKey::Treasury, treasury);
}

pub fn has_treasury(env: &Env) -> bool {
    env.storage().persistent().has(&StorageKey::Treasury)
}

// --- Outcome Token Storage ---

pub fn get_outcome_token_contract(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::OutcomeTokenContract)
}

pub fn set_outcome_token_contract(env: &Env, contract: &Address) {
    env.storage().persistent().set(&StorageKey::OutcomeTokenContract, contract);
}

// --- Resolution Contract Storage ---

pub fn get_resolution_contract(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&StorageKey::ResolutionContract)
}

pub fn set_resolution_contract(env: &Env, contract: &Address) {
    env.storage().persistent().set(&StorageKey::ResolutionContract, contract);
}

// --- Fee Config Storage ---

pub fn get_fee_rate_bps(env: &Env) -> i128 {
    env.storage().persistent().get(&StorageKey::FeeRateBps).unwrap_or(0)
}

pub fn set_fee_rate_bps(env: &Env, fee_rate_bps: i128) {
    env.storage().persistent().set(&StorageKey::FeeRateBps, &fee_rate_bps);
}

// --- Multi-signer threshold storage (#378) ---

/// Return the ordered list of oracle public keys forming the signing quorum.
pub fn get_threshold_signers(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&StorageKey::ThresholdSigners)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_threshold_signers(env: &Env, signers: &Vec<BytesN<32>>) {
    env.storage().persistent().set(&StorageKey::ThresholdSigners, signers);
}

/// Return the minimum number of signatures required. Defaults to 0 (disabled).
pub fn get_threshold_quorum(env: &Env) -> u32 {
    env.storage().persistent().get(&StorageKey::ThresholdQuorum).unwrap_or(0)
}

pub fn set_threshold_quorum(env: &Env, quorum: u32) {
    env.storage().persistent().set(&StorageKey::ThresholdQuorum, &quorum);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::{AdapterType, MarketStatus};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::String;

    fn init_versioned(env: &Env, contract_id: &Address) {
        env.as_contract(contract_id, || set_version(env));
    }

    #[test]
    fn test_wrong_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::StorageVersion, &0u32);
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn test_missing_version_returns_upgrade_required() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
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
            assert!(!has_admin(&env));
            set_admin(&env, &admin);
            assert!(has_admin(&env));
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
        });
    }

    #[test]
    fn test_market_storage_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        let market_id = 1u32;
        let market = Market {
            id: market_id,
            question: String::from_str(&env, "Will it rain?"),
            end_time: 1000,
            oracle_pubkey: BytesN::from_array(&env, &[0u8; 32]),
            status: MarketStatus::Active,
            result: None,
            creator: Address::generate(&env),
            created_at: 0,
            collateral_token: Address::generate(&env),
            price_bps: 5_000,
            resolver: None,
            resolved_at: None,
            adapter_type: AdapterType::Ed25519,
        };
        env.as_contract(&contract_id, || {
            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());
            let saved = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(saved.id, market.id);
        });
    }

    #[test]
    fn test_threshold_signers_default_empty() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert_eq!(get_threshold_signers(&env).len(), 0);
            assert_eq!(get_threshold_quorum(&env), 0);
        });
    }

    #[test]
    fn test_threshold_signers_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            let k1 = BytesN::from_array(&env, &[1u8; 32]);
            let k2 = BytesN::from_array(&env, &[2u8; 32]);
            let k3 = BytesN::from_array(&env, &[3u8; 32]);
            let mut signers = Vec::new(&env);
            signers.push_back(k1.clone());
            signers.push_back(k2.clone());
            signers.push_back(k3.clone());
            set_threshold_signers(&env, &signers);
            set_threshold_quorum(&env, 2);
            let loaded = get_threshold_signers(&env);
            assert_eq!(loaded.len(), 3);
            assert_eq!(loaded.get(0).unwrap(), k1);
            assert_eq!(get_threshold_quorum(&env), 2);
        });
    }

    #[test]
    fn migration_missing_version_blocks_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let user = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
            assert_eq!(get_market(&env, 1), Err(ContractError::UpgradeRequired));
            assert_eq!(get_position(&env, 1, &user), Err(ContractError::UpgradeRequired));
        });
    }

    #[test]
    fn migration_future_version_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&StorageKey::StorageVersion, &(STORAGE_VERSION + 1));
            assert_eq!(assert_version(&env), Err(ContractError::UpgradeRequired));
        });
    }
}
