use crate::error::ContractError;
use crate::types::{Market, Position};
use soroban_sdk::{contracttype, Address, BytesN, Env, Vec};

/// Bump this constant whenever the storage layout changes in a breaking way.
/// `initialize()` writes this value; every storage accessor asserts it.
///
/// # Migration Guide
///
/// **IMPORTANT:** See `STORAGE_MIGRATION_GUIDE.md` for comprehensive documentation
/// on when and how to bump this version, including:
/// - When to increment the version
/// - Step-by-step migration procedures for testnet and mainnet
/// - Testing strategies
/// - Rollback and recovery procedures
/// - Common pitfalls and how to avoid them
///
/// # Quick Reference
///
/// ## Always bump version when:
/// - Adding/removing fields in storage types (Market, Position, etc.)
/// - Changing field types or semantics
/// - Adding new StorageKey variants
/// - Changing how existing data is computed or interpreted
///
/// ## Migration procedure (testnet):
/// 1. Increment `STORAGE_VERSION` in this file
/// 2. Document the change in `MIGRATION.md`
/// 3. Build the contract: `stellar contract build`
/// 4. Deploy: `stellar contract deploy --wasm <path> --network testnet`
/// 5. Initialize: `stellar contract invoke ... -- initialize --admin <addr>`
/// 6. Verify old deployment returns `UpgradeRequired` error
///
/// ## Current version: 3
///
/// ### Version history:
/// - **v3:** Added Treasury, Outcome Token, Resolution Contract, Threshold Signers
/// - **v2:** Fixed locked_collateral semantics (#262)
/// - **v1:** Initial storage layout
///
/// See `STORAGE_MIGRATION_GUIDE.md` and `MIGRATION.md` for detailed history.
pub const STORAGE_VERSION: u32 = 3;

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
    /// Address of the deployed resolution contract that gates resolve_market.
    ResolutionContract,
    /// Ordered list of oracle public keys forming the multi-signer quorum (#378).
    ThresholdSigners,
    /// Minimum number of valid signatures required to resolve a market (#378).
    ThresholdQuorum,
    /// Flag indicating the contract is paused for emergency maintenance.
    /// When true, all state-mutating operations are rejected.
    Paused,
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
    crate::validation::validate_outcome_count(market.outcome_count)?;
    env.storage()
        .persistent()
        .set(&StorageKey::Market(market_id), market);
    Ok(())
}

pub fn has_market(env: &Env, market_id: u32) -> Result<bool, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().has(&StorageKey::Market(market_id)))
}

// --- Position Storage ---

pub fn get_position(
    env: &Env,
    market_id: u32,
    user: &Address,
) -> Result<Option<Position>, ContractError> {
    assert_version(env)?;
    Ok(env.storage().persistent().get(&StorageKey::Position(market_id, user.clone())))
}

pub fn set_position(
    env: &Env,
    market_id: u32,
    user: &Address,
    position: &Position,
) -> Result<(), ContractError> {
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

/// Register (or replace) the treasury contract address for protocol fee routing.
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

// --- Threshold Signers Storage ---

pub fn get_threshold_signers(env: &Env) -> Vec<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&StorageKey::ThresholdSigners)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_threshold_signers(env: &Env, signers: &Vec<BytesN<32>>) {
    env.storage().persistent().set(&StorageKey::ThresholdSigners, signers);
}

pub fn get_threshold_quorum(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&StorageKey::ThresholdQuorum)
        .unwrap_or(0)
}

pub fn set_threshold_quorum(env: &Env, quorum: u32) {
    env.storage().persistent().set(&StorageKey::ThresholdQuorum, &quorum);
}

// --- Fee Config Storage ---

pub fn get_fee_rate_bps(env: &Env) -> i128 {
    env.storage().persistent().get(&StorageKey::FeeRateBps).unwrap_or(0)
}

pub fn set_fee_rate_bps(env: &Env, fee_rate_bps: i128) {
    env.storage().persistent().set(&StorageKey::FeeRateBps, &fee_rate_bps);
}


// --- Pause Storage ---

/// Check whether the contract is in a paused state.
pub fn is_paused(env: &Env) -> bool {
    env.storage().persistent().get(&StorageKey::Paused).unwrap_or(false)
}

/// Pause or unpause the contract (emergency halt).
pub fn set_paused(env: &Env, paused: bool) {
    env.storage().persistent().set(&StorageKey::Paused, &paused);
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
    fn test_fee_rate_bps_defaults_to_50() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            assert_eq!(
                get_fee_rate_bps(&env),
                DEFAULT_FEE_RATE_BPS,
                "fee rate must default to 50 bps"
            );
        });
    }

    #[test]
    fn test_fee_rate_bps_round_trip() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            set_fee_rate_bps(&env, 100);
            assert_eq!(get_fee_rate_bps(&env), 100);

            set_fee_rate_bps(&env, 0);
            assert_eq!(get_fee_rate_bps(&env), 0);

            set_fee_rate_bps(&env, MAX_FEE_RATE_BPS);
            assert_eq!(get_fee_rate_bps(&env), MAX_FEE_RATE_BPS);
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
            outcome_count: 2,
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
            outcome_count: 2,
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
            set_admin(&env, &admin);
            increment_market_id(&env).unwrap();

            assert_eq!(get_admin(&env).unwrap(), admin);
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            assert!(!has_market(&env, market_id).unwrap());
            set_market(&env, market_id, &market).unwrap();
            assert!(has_market(&env, market_id).unwrap());

            assert_eq!(get_admin(&env).unwrap(), admin);
            assert_eq!(get_next_market_id(&env).unwrap(), 1);

            let m = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m.id, market.id);
            assert_eq!(m.question, market.question);
            assert_eq!(m.end_time, market.end_time);
            assert_eq!(m.oracle_pubkey, market.oracle_pubkey);
            assert_eq!(m.result, market.result);
            assert_eq!(m.creator, market.creator);
            assert_eq!(m.created_at, market.created_at);
            assert_eq!(m.collateral_token, market.collateral_token);

            assert!(!has_position(&env, market_id, &user).unwrap());
            set_position(&env, market_id, &user, &position).unwrap();
            assert!(has_position(&env, market_id, &user).unwrap());

            let other_user = Address::generate(&env);
            assert!(!has_position(&env, market_id, &other_user).unwrap());

            let p = get_position(&env, market_id, &user).unwrap().unwrap();
            assert_eq!(p.market_id, position.market_id);
            assert_eq!(p.user, position.user);
            assert_eq!(p.yes_shares, position.yes_shares);
            assert_eq!(p.no_shares, position.no_shares);
            assert_eq!(p.locked_collateral, position.locked_collateral);
            assert_eq!(p.total_deposited, position.total_deposited);
            assert_eq!(p.is_settled, position.is_settled);

            let m2 = get_market(&env, market_id).unwrap().unwrap();
            assert_eq!(m2.id, market.id);
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
            adapter_type: AdapterType::Ed25519,
            outcome_count: 2,
        };

        env.as_contract(&contract_id, || {
            set_version(&env);
            assert_eq!(assert_version(&env), Ok(()));

            set_market(&env, 1, &market).unwrap();
            let m = get_market(&env, 1).unwrap().unwrap();
            assert_eq!(m.id, 1);
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

            set_treasury(&env, &treasury);
            assert!(has_treasury(&env));
            assert_eq!(get_treasury(&env), Some(treasury.clone()));
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

    // ── Pause storage helpers ─────────────────────────────────────────────

    #[test]
    fn test_pause_defaults_to_false() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            assert!(!is_paused(&env), "fresh contract should not be paused");
        });
    }

    #[test]
    fn test_pause_can_be_set_and_cleared() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            set_paused(&env, true);
            assert!(is_paused(&env));
            set_paused(&env, false);
            assert!(!is_paused(&env));
        });
    }

    #[test]
    fn test_pause_toggle_returns_to_unpaused() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        init_versioned(&env, &contract_id);
        env.as_contract(&contract_id, || {
            set_paused(&env, true);
            assert!(is_paused(&env));
            set_paused(&env, true);
            assert!(is_paused(&env), "pausing again should stay paused");
            set_paused(&env, false);
            assert!(!is_paused(&env));
        });
    }
}
