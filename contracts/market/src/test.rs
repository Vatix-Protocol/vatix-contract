#![cfg(test)]

// Tests for contract functions
#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as TestAddress, Address, BytesN, Env, String};
    use crate::types::MarketStatus;

    #[test]
    fn test_initialize_market_success() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let collateral_token = <Address as TestAddress>::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin
            crate::storage::set_admin(&env, &admin);

            let current_time = env.ledger().timestamp();
            let end_time = current_time + 7 * 24 * 60 * 60; // 1 week from now
            let question = String::from_str(&env, "Will BTC hit $100k?");
            let oracle_pubkey = BytesN::from_array(&env, &[0u8; 32]);

            let market_id = crate::MarketContract::initialize_market(
                env.clone(),
                admin.clone(),
                question.clone(),
                end_time,
                oracle_pubkey.clone(),
                collateral_token.clone(),
            )
            .expect("should initialize market");

            // Verify market was stored
            let stored_market = crate::storage::get_market(&env, &market_id)
                .expect("market should be stored");

            assert_eq!(stored_market.id, market_id);
            assert_eq!(stored_market.question, question);
            assert_eq!(stored_market.end_time, end_time);
            assert_eq!(stored_market.status, MarketStatus::Active);
            assert_eq!(stored_market.result, None);
            assert_eq!(stored_market.creator, admin);
        });
    }

    #[test]
    fn test_initialize_market_unauthorized() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let non_admin = <Address as TestAddress>::generate(&env);
        let collateral_token = <Address as TestAddress>::generate(&env);

        env.as_contract(&contract_id, || {
            // Set admin to different address
            crate::storage::set_admin(&env, &admin);

            let current_time = env.ledger().timestamp();
            let end_time = current_time + 7 * 24 * 60 * 60;
            let question = String::from_str(&env, "Will BTC hit $100k?");
            let oracle_pubkey = BytesN::from_array(&env, &[0u8; 32]);

            let result = crate::MarketContract::initialize_market(
                env.clone(),
                non_admin,
                question,
                end_time,
                oracle_pubkey,
                collateral_token,
            );

            assert!(result.is_err());
        });
    }

    #[test]
    fn test_initialize_market_invalid_end_time() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let collateral_token = <Address as TestAddress>::generate(&env);

        env.as_contract(&contract_id, || {
            crate::storage::set_admin(&env, &admin);

            let current_time = env.ledger().timestamp();
            let end_time = current_time - 100; // In the past
            let question = String::from_str(&env, "Will BTC hit $100k?");
            let oracle_pubkey = BytesN::from_array(&env, &[0u8; 32]);

            let result = crate::MarketContract::initialize_market(
                env.clone(),
                admin,
                question,
                end_time,
                oracle_pubkey,
                collateral_token,
            );

            assert!(result.is_err());
        });
    }

    #[test]
    fn test_initialize_market_empty_question() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let collateral_token = <Address as TestAddress>::generate(&env);

        env.as_contract(&contract_id, || {
            crate::storage::set_admin(&env, &admin);

            let current_time = env.ledger().timestamp();
            let end_time = current_time + 7 * 24 * 60 * 60;
            let question = String::from_str(&env, ""); // Empty question
            let oracle_pubkey = BytesN::from_array(&env, &[0u8; 32]);

            let result = crate::MarketContract::initialize_market(
                env.clone(),
                admin,
                question,
                end_time,
                oracle_pubkey,
                collateral_token,
            );

            assert!(result.is_err());
        });
    }

    #[test]
    fn test_initialize_market_multiple() {
        let env = Env::default();
        let contract_id = env.register(crate::MarketContract, ());
        let admin = <Address as TestAddress>::generate(&env);
        let collateral_token = <Address as TestAddress>::generate(&env);

        env.as_contract(&contract_id, || {
            crate::storage::set_admin(&env, &admin);

            let current_time = env.ledger().timestamp();
            let end_time = current_time + 7 * 24 * 60 * 60;
            let oracle_pubkey = BytesN::from_array(&env, &[0u8; 32]);

            // Create first market
            let market_id_1 = crate::MarketContract::initialize_market(
                env.clone(),
                admin.clone(),
                String::from_str(&env, "Market 1?"),
                end_time,
                oracle_pubkey.clone(),
                collateral_token.clone(),
            )
            .expect("should create first market");

            // Create second market
            let market_id_2 = crate::MarketContract::initialize_market(
                env.clone(),
                admin.clone(),
                String::from_str(&env, "Market 2?"),
                end_time,
                oracle_pubkey.clone(),
                collateral_token.clone(),
            )
            .expect("should create second market");

            // IDs should be different
            assert_ne!(market_id_1, market_id_2);

            // Both markets should exist
            assert!(crate::storage::get_market(&env, &market_id_1).is_some());
            assert!(crate::storage::get_market(&env, &market_id_2).is_some());
        });
    }
}
