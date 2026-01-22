use soroban_sdk::{symbol_short, Address, Env, String, Symbol};
use crate::types::{Market, Position};

const MARKETS_KEY: Symbol = symbol_short!("MARKETS");
const POSITIONS_KEY: Symbol = symbol_short!("POSITIONS");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");
const COUNTER_KEY: Symbol = symbol_short!("COUNTER");

// --- Market Storage ---

pub fn get_market(env: &Env, market_id: &String) -> Option<Market> {
    env.storage()
        .persistent()
        .get(&(MARKETS_KEY, market_id.clone()))
}

pub fn set_market(env: &Env, market_id: &String, market: &Market) {
    env.storage()
        .persistent()
        .set(&(MARKETS_KEY, market_id.clone()), market);
}

pub fn has_market(env: &Env, market_id: &String) -> bool {
    env.storage()
        .persistent()
        .has(&(MARKETS_KEY, market_id.clone()))
}

// --- Position Storage ---

pub fn get_position(env: &Env, market_id: &String, user: &Address) -> Option<Position> {
    env.storage()
        .persistent()
        .get(&(POSITIONS_KEY, market_id.clone(), user.clone()))
}

pub fn set_position(env: &Env, market_id: &String, user: &Address, position: &Position) {
    env.storage()
        .persistent()
        .set(&(POSITIONS_KEY, market_id.clone(), user.clone()), position);
}

pub fn has_position(env: &Env, market_id: &String, user: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&(POSITIONS_KEY, market_id.clone(), user.clone()))
}

// --- Configuration Storage ---

pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&ADMIN_KEY)
        .expect("Admin not set")
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&ADMIN_KEY, admin);
}

pub fn get_next_market_id(env: &Env) -> u32 {
    env.storage().persistent().get(&COUNTER_KEY).unwrap_or(0)
}

pub fn increment_market_id(env: &Env) -> u32 {
    let next_id = get_next_market_id(env) + 1;
    env.storage().persistent().set(&COUNTER_KEY, &next_id);
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
        let market_id = String::from_str(&env, "market-1");
        let creator = Address::generate(&env);
        let collateral_token = Address::generate(&env);

        let market = Market {
            id: market_id.clone(),
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
            assert!(!has_market(&env, &market_id));
            set_market(&env, &market_id, &market);
            assert!(has_market(&env, &market_id));
            
            let saved_market = get_market(&env, &market_id).unwrap();
            assert_eq!(saved_market.id, market.id);
            assert_eq!(saved_market.question, market.question);
        });
    }

    #[test]
    fn test_position_storage() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let market_id = String::from_str(&env, "market-1");
        let user = Address::generate(&env);

        let position = Position {
            market_id: market_id.clone(),
            user: user.clone(),
            yes_shares: 100,
            no_shares: 0,
            locked_collateral: 100,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            assert!(!has_position(&env, &market_id, &user));
            set_position(&env, &market_id, &user, &position);
            assert!(has_position(&env, &market_id, &user));

            let saved_position = get_position(&env, &market_id, &user).unwrap();
            assert_eq!(saved_position.yes_shares, 100);
            assert_eq!(saved_position.market_id, market_id);
        });
    }
}
