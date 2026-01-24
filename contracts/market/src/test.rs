#[cfg(test)]
mod test {
    use crate::{
        storage,
        types::{Market, MarketStatus},
        MarketContract, MarketContractClient,
    };
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, BytesN, Env, String,
    };

    fn create_test_contract<'a>() -> (Env, Address, MarketContractClient<'a>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        // Initialize admin in storage - MUST wrap in as_contract
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
        });

        (env, admin, client, contract_id)
    }

    fn get_market_from_storage(env: &Env, contract_id: &Address, market_id: u32) -> Market {
        env.as_contract(contract_id, || {
            storage::get_market(env, market_id).expect("Market should exist")
        })
    }

    // Rest of tests remain the same...
    #[test]
    fn test_initialize_market_success() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Will BTC reach $100k by March?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        assert_eq!(market_id, 1);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.id, 1);
        assert_eq!(market.question, question);
        assert_eq!(market.end_time, end_time);
        assert_eq!(market.oracle_pubkey, oracle_pubkey);
        assert_eq!(market.status, MarketStatus::Active);
        assert_eq!(market.result, None);
        assert_eq!(market.creator, admin);
        assert_eq!(market.collateral_token, collateral_token);
    }

    #[test]
    fn test_initialize_market_increments_counter() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Question 1");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let market_id_1 = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );
        assert_eq!(market_id_1, 1);

        let question_2 = String::from_str(&env, "Question 2");
        let market_id_2 = client.initialize_market(
            &admin,
            &question_2,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );
        assert_eq!(market_id_2, 2);

        let question_3 = String::from_str(&env, "Question 3");
        let market_id_3 = client.initialize_market(
            &admin,
            &question_3,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );
        assert_eq!(market_id_3, 3);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_initialize_market_non_admin_fails() {
        let (env, _admin, client, _contract_id) = create_test_contract();

        let non_admin = Address::generate(&env);
        let question = String::from_str(&env, "Will BTC reach $100k?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &non_admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #33)")]
    fn test_initialize_market_empty_question_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let empty_question = String::from_str(&env, "");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &empty_question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #32)")]
    fn test_initialize_market_past_end_time_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Will BTC reach $100k?");

        // Set ledger timestamp to non-zero first
        env.ledger().set_timestamp(1000); // Set to 1000 so we can subtract

        let past_end_time = env.ledger().timestamp() - 1;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &past_end_time,
            &oracle_pubkey,
            &collateral_token,
        );
    }

    #[test]
    fn test_initialize_market_stores_correct_timestamp() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let current_time = env.ledger().timestamp();

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.created_at, current_time);
    }

    #[test]
    fn test_initialize_market_different_collateral_tokens() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market with USDC");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let usdc_token = Address::generate(&env);

        let market_id =
            client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &usdc_token);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.collateral_token, usdc_token);
    }

    #[test]
    fn test_initialize_market_event_emitted() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Event test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events = env.events().all();
        assert!(events.len() > 0, "Market creation should emit an event");
        
        // Verify the event contains the expected data structure
        let event = &events[0];
        assert!(event.topics.len() >= 2, "Event should have at least 2 topics");
    }
}
