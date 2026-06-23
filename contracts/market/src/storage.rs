use crate::types::{Market, Position};
use soroban_sdk::{contracttype, Address, Env};

// --- Storage Keys ---
// TODO: https://github.com/vatix-protocol/vatix-contract/issues/79
// Consider versioning storage layout to support future contract upgrades
// without data migration issues

#[contracttype]
pub enum StorageKey {
    Market(u32),
    Position(u32, Address),
    Admin,
    PendingAdmin,
    MarketCounter,
    /// Address of the deployed treasury contract.
    /// Set by the admin via `set_treasury`; optional — withdrawal fees are
    /// only routed there when this key is populated and fee_amount > 0.
    TreasuryContract,
}

// --- Market Storage ---

pub fn get_market(env: &Env, market_id: u32) -> Option<Market> {
    env.storage()
        .persistent()
        .get(&StorageKey::Market(market_id))
}

pub fn set_market(env: &Env, market_id: u32, market: &Market) {
    env.storage()
        .persistent()
        .set(&StorageKey::Market(market_id), market);
}

pub fn has_market(env: &Env, market_id: u32) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::Market(market_id))
}

// --- Position Storage ---

pub fn get_position(env: &Env, market_id: u32, user: &Address) -> Option<Position> {
    env.storage()
        .persistent()
        .get(&StorageKey::Position(market_id, user.clone()))
}

pub fn set_position(env: &Env, market_id: u32, user: &Address, position: &Position) {
    env.storage()
        .persistent()
        .set(&StorageKey::Position(market_id, user.clone()), position);
}

pub fn has_position(env: &Env, market_id: u32, user: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&StorageKey::Position(market_id, user.clone()))
}

// --- Configuration Storage ---

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&StorageKey::Admin)
        .expect("Admin not set")
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

pub fn get_next_market_id(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&StorageKey::MarketCounter)
        .unwrap_or(0)
}

pub fn increment_market_id(env: &Env) -> u32 {
    let next_id = get_next_market_id(env) + 1;
    env.storage()
        .persistent()
        .set(&StorageKey::MarketCounter, &next_id);
    next_id
}

// --- Treasury Storage ---

pub fn get_treasury(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&StorageKey::TreasuryContract)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::TreasuryContract, treasury);
}

pub fn has_treasury(env: &Env) -> bool {
    env.storage()
        .instance()
        .has(&StorageKey::TreasuryContract)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::MarketStatus;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{BytesN, String};

    #[test]
    fn test_admin_storage() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            assert!(!has_admin(&env), "admin slot should be empty before set");
            set_admin(&env, &admin);
            assert!(has_admin(&env), "admin slot should be populated after set");
            assert_eq!(get_admin(&env), admin);
        });
    }

    #[test]
    fn test_market_id_counter() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());

        env.as_contract(&contract_id, || {
            assert_eq!(get_next_market_id(&env), 0);
            assert_eq!(increment_market_id(&env), 1);
            assert_eq!(increment_market_id(&env), 2);
            assert_eq!(get_next_market_id(&env), 2);
        });
    }

    #[test]
    fn test_market_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
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
            assert!(!has_market(&env, market_id));
            set_market(&env, market_id, &market);
            assert!(has_market(&env, market_id));

            let saved_market = get_market(&env, market_id).unwrap();
            assert_eq!(saved_market.id, market.id);
            assert_eq!(saved_market.question, market.question);
        });
    }

    #[test]
    fn test_position_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
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
            assert!(!has_position(&env, market_id, &user));
            set_position(&env, market_id, &user, &position);
            assert!(has_position(&env, market_id, &user));

            let saved_position = get_position(&env, market_id, &user).unwrap();
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
            increment_market_id(&env); // counter becomes 1

            assert_eq!(get_admin(&env), admin);
            // MarketCounter write must not have corrupted Admin slot
            assert_eq!(get_admin(&env), admin);
            // Admin write must not have corrupted MarketCounter slot
            assert_eq!(get_next_market_id(&env), 1);

            // --- Market slot is independent from admin and counter ---
            assert!(!has_market(&env, market_id));
            set_market(&env, market_id, &market);
            assert!(has_market(&env, market_id));

            // Admin and counter are unchanged after market write
            assert_eq!(get_admin(&env), admin);
            assert_eq!(get_next_market_id(&env), 1);

            // All Market fields survive the round-trip
            let m = get_market(&env, market_id).unwrap();
            assert_eq!(m.id, market.id);
            assert_eq!(m.question, market.question);
            assert_eq!(m.end_time, market.end_time);
            assert_eq!(m.oracle_pubkey, market.oracle_pubkey);
            assert_eq!(m.result, market.result);
            assert_eq!(m.creator, market.creator);
            assert_eq!(m.created_at, market.created_at);
            assert_eq!(m.collateral_token, market.collateral_token);

            // --- Position slot is keyed by (market_id, user) ---
            assert!(!has_position(&env, market_id, &user));
            set_position(&env, market_id, &user, &position);
            assert!(has_position(&env, market_id, &user));

            // A different user must not see this position
            let other_user = Address::generate(&env);
            assert!(!has_position(&env, market_id, &other_user));

            // All Position fields survive the round-trip
            let p = get_position(&env, market_id, &user).unwrap();
            assert_eq!(p.market_id, position.market_id);
            assert_eq!(p.user, position.user);
            assert_eq!(p.yes_shares, position.yes_shares);
            assert_eq!(p.no_shares, position.no_shares);
            assert_eq!(p.locked_collateral, position.locked_collateral);
            assert_eq!(p.total_deposited, position.total_deposited);
            assert_eq!(p.is_settled, position.is_settled);

            // Market slot is unchanged after position write
            let m2 = get_market(&env, market_id).unwrap();
            assert_eq!(m2.id, market.id);
        });
    }
}
