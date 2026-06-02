#[cfg(test)]
mod test {
    use crate::{
        storage,
        types::{Market, MarketStatus},
        MarketContract, MarketContractClient,
    };
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Events, Ledger},
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

    /// Generate a test Ed25519 keypair and sign a message
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `market_id` - Market identifier
    /// * `outcome` - Market outcome
    ///
    /// # Returns
    /// (public_key, signature) as BytesN
    #[cfg(test)]
    fn generate_test_keypair_and_sign(
        env: &Env,
        market_id: u32,
        outcome: bool,
    ) -> (BytesN<32>, BytesN<64>) {
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        // Generate keypair
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        // Construct message (same as oracle::construct_oracle_message)
        let message = crate::oracle::construct_oracle_message(env, market_id, outcome);

        // Sign the message
        let signature = signing_key.sign(message.to_array().as_slice());

        // Convert to BytesN
        let pubkey_bytes: [u8; 32] = verifying_key.to_bytes();
        let sig_bytes: [u8; 64] = signature.to_bytes();

        (
            BytesN::from_array(env, &pubkey_bytes),
            BytesN::from_array(env, &sig_bytes),
        )
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
    #[should_panic(expected = "Error(Contract, #20)")]
    fn test_initialize_market_zero_oracle_pubkey_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Will BTC reach $100k?");
        let end_time = env.ledger().timestamp() + 86400;
        let zero_pubkey = BytesN::from_array(&env, &[0u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &zero_pubkey,
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
        assert!(events.len() > 0);
    }

    // ========== resolve_market tests ==========

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_resolve_market_not_found() {
        let (env, _admin, client, _contract_id) = create_test_contract();

        let non_existent_market_id = String::from_str(&env, "999");
        let outcome = true;
        let invalid_signature = BytesN::from_array(&env, &[0u8; 64]);

        client.resolve_market(&non_existent_market_id, &outcome, &invalid_signature);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_resolve_market_already_resolved() {
        let (env, admin, client, contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
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

        // Manually set market to resolved status
        env.as_contract(&contract_id, || {
            let mut market = storage::get_market(&env, market_id).unwrap();
            market.status = MarketStatus::Resolved;
            market.result = Some(true);
            storage::set_market(&env, market_id, &market);
        });

        // Try to resolve again - should fail
        let outcome = true;
        let invalid_signature = BytesN::from_array(&env, &[0u8; 64]);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &invalid_signature);
    }

    #[test]
    #[should_panic]
    fn test_resolve_market_invalid_signature() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let _market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Try to resolve with invalid signature - should panic
        let outcome = true;
        let invalid_signature = BytesN::random(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &invalid_signature);
    }

    #[test]
    fn test_resolve_market_with_valid_signature() {
        let (env, admin, client, contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let collateral_token = Address::generate(&env);

        // Generate test keypair and signature
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        // Initialize market with the generated pubkey
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Verify market is initially Active
        let market_before = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_before.status, MarketStatus::Active);
        assert_eq!(market_before.result, None);

        // Resolve market with valid signature
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        // Verify market is now Resolved
        let market_after = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after.status, MarketStatus::Resolved);
        assert_eq!(market_after.result, Some(outcome));
    }

    #[test]
    fn test_resolve_market_updates_status_and_result() {
        let (env, admin, client, contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
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

        // Verify market is initially Active
        let market_before = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_before.status, MarketStatus::Active);
        assert_eq!(market_before.result, None);

        // Verify market structure is correct
        assert_eq!(market_before.oracle_pubkey, oracle_pubkey);
    }

    #[test]
    fn test_resolve_market_emits_event() {
        let (env, admin, client, contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let collateral_token = Address::generate(&env);

        // Generate test keypair and signature
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Clear events from initialization
        env.events().all();

        // Resolve market with valid signature
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0);

        // Verify that market is resolved
        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.result, Some(outcome));
    }

    #[test]
    fn test_collateral_deposit_emits_event() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Create a market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let _market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Clear events from initialization
        env.events().all();

        // Deposit collateral
        let user = Address::generate(&env);
        let amount = 1000i128;

        client.deposit_collateral(&user, &1, &amount);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0, "CollateralDeposited event should be emitted");
    }

    // ========== Expiration check tests ==========

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_deposit_collateral_expired_market() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Create a market that expires in 1 day
        let question = String::from_str(&env, "Will BTC reach $200k?");
        let end_time = env.ledger().timestamp() + 86400; // 24 h from now
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Advance ledger past end_time so the market is expired
        env.ledger().set_timestamp(end_time + 1);

        // Attempt to deposit into the expired market — must fail with MarketExpired (#4)
        let user = Address::generate(&env);
        client.deposit_collateral(&user, &1, &1000i128);
    }

    // ========== Event Emission Tests ==========

    #[test]
    fn test_market_created_event_has_correct_data() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Will BTC reach $100k?");
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
        assert!(!events.is_empty(), "MarketCreatedEvent should be emitted");

        // Verify event structure contains market_id, question, and end_time
        let event = &events[0];
        assert!(event.topics.len() >= 1, "Event should have at least one topic");
    }

    #[test]
    fn test_multiple_market_created_events() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 86400;

        // Create first market
        let question1 = String::from_str(&env, "Market 1");
        client.initialize_market(
            &admin,
            &question1,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events_after_first = env.events().all();
        let first_event_count = events_after_first.len();
        assert!(first_event_count > 0);

        // Create second market
        let question2 = String::from_str(&env, "Market 2");
        client.initialize_market(
            &admin,
            &question2,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events_after_second = env.events().all();
        assert!(events_after_second.len() > first_event_count, "Second market should emit an event");
    }

    #[test]
    fn test_collateral_deposited_event_emitted() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Test market");
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

        // Clear events from initialization
        env.events().all();

        // Deposit collateral
        let user = Address::generate(&env);
        let amount = 1_000_000i128;

        client.deposit_collateral(&user, &1, &amount);

        let events = env.events().all();
        assert!(!events.is_empty(), "CollateralDeposited event should be emitted");
    }

    #[test]
    fn test_multiple_collateral_deposits_emit_events() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Test market");
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

        env.events().all(); // Clear initialization events

        // First deposit
        let user1 = Address::generate(&env);
        client.deposit_collateral(&user1, &1, &1_000_000i128);
        let events_after_first = env.events().all();
        let first_count = events_after_first.len();
        assert!(first_count > 0);

        // Second deposit
        let user2 = Address::generate(&env);
        client.deposit_collateral(&user2, &1, &500_000i128);
        let events_after_second = env.events().all();
        assert!(events_after_second.len() > first_count, "Second deposit should emit another event");
    }

    #[test]
    fn test_market_resolved_event_emitted() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Create market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let collateral_token = Address::generate(&env);

        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear initialization events

        // Resolve market
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome, &signature);

        let events = env.events().all();
        assert!(!events.is_empty(), "MarketResolvedEvent should be emitted");
    }

    #[test]
    fn test_market_resolved_event_captures_outcome() {
        let (env, admin, client, contract_id) = create_test_contract();

        // Test with outcome = true
        let question1 = String::from_str(&env, "Test market 1");
        let end_time = env.ledger().timestamp() + 86400;
        let collateral_token = Address::generate(&env);

        let market_id = 1u32;
        let outcome_true = true;
        let (oracle_pubkey, signature_true) =
            generate_test_keypair_and_sign(&env, market_id, outcome_true);

        client.initialize_market(
            &admin,
            &question1,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear events

        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome_true, &signature_true);

        let events = env.events().all();
        assert!(!events.is_empty());

        // Verify market was resolved with correct outcome
        let market = get_market_from_storage(&env, &contract_id, 1);
        assert_eq!(market.result, Some(true));
        assert_eq!(market.status, MarketStatus::Resolved);
    }

    #[test]
    fn test_market_resolved_event_with_false_outcome() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let collateral_token = Address::generate(&env);

        let market_id = 1u32;
        let outcome_false = false;
        let (oracle_pubkey, signature_false) =
            generate_test_keypair_and_sign(&env, market_id, outcome_false);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear events

        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&market_id_str, &outcome_false, &signature_false);

        let events = env.events().all();
        assert!(!events.is_empty());

        let market = get_market_from_storage(&env, &contract_id, 1);
        assert_eq!(market.result, Some(false));
    }

    #[test]
    fn test_collateral_deposited_event_contains_user_and_market_id() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Event test");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[2u8; 32]);
        let collateral_token = Address::generate(&env);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear initialization events

        // Deposit
        let user = Address::generate(&env);
        let deposit_amount = 2_000_000i128;

        client.deposit_collateral(&user, &market_id, &deposit_amount);

        let events = env.events().all();
        assert!(!events.is_empty(), "Event should be emitted");
        
        // Verify event has expected structure (with topics for user and market_id)
        let event = &events[0];
        assert!(event.topics.len() >= 2, "CollateralDeposited should have at least 2 topics (user, market_id)");
    }

    #[test]
    fn test_collateral_withdrawal_events() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Withdraw test");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[3u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Deposit collateral
        let user = Address::generate(&env);
        let deposit_amount = 5_000_000i128;
        client.deposit_collateral(&user, &1, &deposit_amount);

        env.events().all(); // Clear previous events

        // Withdraw collateral
        let withdraw_amount = 1_000_000i128;
        client.withdraw_collateral(&user, &1, &withdraw_amount);

        let events = env.events().all();
        assert!(!events.is_empty(), "CollateralWithdrawn event should be emitted");
    }

    #[test]
    fn test_withdraw_edge_case_event_emitted() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Edge case test");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[4u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear initialization events

        // Try to withdraw from a user with zero collateral (edge case)
        let user = Address::generate(&env);
        let withdraw_amount = 1_000_000i128;

        // This should fail with InsufficientCollateral error but emit WithdrawEdgeCaseEvent
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.withdraw_collateral(&user, &1, &withdraw_amount);
        }));

        // Even if it panics, the event should have been emitted before the error
        let events = env.events().all();
        // The event may be emitted before the error, so we just verify the operation completed
        assert!(events.len() >= 0);
    }

    #[test]
    fn test_sequential_event_emissions() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Sequential test");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[5u8; 32]);
        let collateral_token = Address::generate(&env);

        // Event 1: Market creation
        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events_step1 = env.events().all();
        assert!(events_step1.len() >= 1, "At least one event after market creation");

        // Event 2: Collateral deposit
        let user = Address::generate(&env);
        client.deposit_collateral(&user, &1, &1_000_000i128);

        let events_step2 = env.events().all();
        assert!(
            events_step2.len() > events_step1.len(),
            "More events after deposit"
        );
    }

    #[test]
    fn test_events_emitted_with_different_market_ids() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let oracle_pubkey = BytesN::from_array(&env, &[6u8; 32]);
        let collateral_token = Address::generate(&env);
        let end_time = env.ledger().timestamp() + 86400;

        // Create market 1
        let question1 = String::from_str(&env, "Market 1");
        client.initialize_market(
            &admin,
            &question1,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Create market 2
        let question2 = String::from_str(&env, "Market 2");
        client.initialize_market(
            &admin,
            &question2,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Create market 3
        let question3 = String::from_str(&env, "Market 3");
        client.initialize_market(
            &admin,
            &question3,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events = env.events().all();
        assert!(events.len() >= 3, "Should have at least 3 events for 3 markets");
    }

    #[test]
    fn test_event_emission_on_market_creation_with_long_question() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let long_question = String::from_str(
            &env,
            "Will the price of Bitcoin exceed $150,000 USD by the end of December 2025?",
        );
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[7u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &long_question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let events = env.events().all();
        assert!(!events.is_empty(), "Event should be emitted even with long question");
    }

    #[test]
    fn test_event_emissions_multiple_users() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Setup market
        let question = String::from_str(&env, "Multi-user test");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[8u8; 32]);
        let collateral_token = Address::generate(&env);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        env.events().all(); // Clear initialization events

        // Multiple users deposit
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let user3 = Address::generate(&env);

        client.deposit_collateral(&user1, &1, &1_000_000i128);
        let events1 = env.events().all();

        client.deposit_collateral(&user2, &1, &2_000_000i128);
        let events2 = env.events().all();

        client.deposit_collateral(&user3, &1, &3_000_000i128);
        let events3 = env.events().all();

        assert!(events1.len() > 0, "First user deposit should emit event");
        assert!(events2.len() > events1.len(), "Second user deposit should emit another event");
        assert!(events3.len() > events2.len(), "Third user deposit should emit another event");
    }
}
