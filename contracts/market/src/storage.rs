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
    MarketCounter,
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
    env.storage()
        .persistent()
        .set(&StorageKey::Admin, admin);
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
            set_admin(&env, &admin);
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
}
