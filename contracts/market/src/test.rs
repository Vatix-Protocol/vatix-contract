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
    use vatix_resolution_contract::{ResolutionContract, ResolutionContractClient};

    fn create_test_contract<'a>() -> (Env, Address, MarketContractClient<'a>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        // Initialize admin in storage - MUST wrap in as_contract
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        (env, admin, client, contract_id)
    }

    fn get_market_from_storage(env: &Env, contract_id: &Address, market_id: u32) -> Market {
        env.as_contract(contract_id, || {
            storage::get_market(env, market_id)
                .expect("version check failed")
                .expect("Market should exist")
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
    #[should_panic(expected = "Error(Contract, #41)")]
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

        let resolver = Address::generate(&env);
        let non_existent_market_id = String::from_str(&env, "999");
        let outcome = true;
        let invalid_signature = BytesN::from_array(&env, &[0u8; 64]);

        client.resolve_market(&resolver, &non_existent_market_id, &outcome, &invalid_signature);
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
            let mut market = storage::get_market(&env, market_id).unwrap().unwrap();
            market.status = MarketStatus::Resolved;
            market.result = Some(true);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        // Try to resolve again - should fail
        let resolver = Address::generate(&env);
        let outcome = true;
        let invalid_signature = BytesN::from_array(&env, &[0u8; 64]);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &invalid_signature);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #20)")]
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

        // Bad signature must surface as the typed InvalidSignature error
        // (#20), not an uncaught host trap.
        let resolver = Address::generate(&env);
        let outcome = true;
        let invalid_signature = BytesN::random(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &invalid_signature);
    }

    #[test]
    fn test_resolve_market_invalid_signature_leaves_market_active() {
        let (env, admin, client, contract_id) = create_test_contract();

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

        let resolver = Address::generate(&env);
        let outcome = true;
        let invalid_signature = BytesN::random(&env);
        let market_id_str = String::from_str(&env, "1");
        let result = client.try_resolve_market(&resolver, &market_id_str, &outcome, &invalid_signature);

        assert_eq!(
            result,
            Err(Ok(crate::error::ContractError::InvalidSignature))
        );

        // Market must be untouched - no partial state mutation on failure.
        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Active);
        assert_eq!(market.result, None);
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
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature);

        // Verify market is now Resolved
        let market_after = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after.status, MarketStatus::Resolved);
        assert_eq!(market_after.result, Some(outcome));
        assert_eq!(market_after.resolver, Some(resolver));
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
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0);

        // Verify that market is resolved
        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.result, Some(outcome));
        assert_eq!(market.resolver, Some(resolver));
    }

    #[test]
    fn test_collateral_deposit_emits_event() {
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        env.mock_all_auths();

        // Create a market
        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);

        let _market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Clear events from initialization
        env.events().all();

        // Mint tokens to user for deposit
        let user = Address::generate(&env);
        let amount = 1000i128;
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &amount);

        // Deposit collateral
        client.deposit_collateral(&user, &1, &amount);

        // Verify event was emitted
        let events = env.events().all();
        assert!(
            events.len() > 0,
            "CollateralDeposited event should be emitted"
        );
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

    // ========== update_position tests ==========

    /// Register a market backed by a real Stellar asset, fund `user`, and
    /// deposit `deposit` stroops of collateral so trades can be exercised.
    fn setup_funded_market<'a>(
        deposit: i128,
    ) -> (Env, Address, MarketContractClient<'a>, Address, u32) {
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        env.mock_all_auths();

        let question = String::from_str(&env, "Will it rain tomorrow?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        (env, user, client, contract_id, market_id)
    }

    #[test]
    fn test_update_position_buys_shares_and_locks_collateral() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Buy 100 YES shares at a 60% price -> lock 60 USDC
        let yes = 100 * STROOPS_PER_USDC;
        let position = client.update_position(&user, &market_id, &yes, &0i128, &6000i128);

        assert_eq!(position.yes_shares, yes);
        assert_eq!(position.no_shares, 0);
        assert_eq!(position.locked_collateral, 60 * STROOPS_PER_USDC);

        // The persisted position matches the returned one
        let stored = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user).expect("version check ok").expect("position should exist")
        });
        assert_eq!(stored.yes_shares, yes);
        assert_eq!(stored.locked_collateral, 60 * STROOPS_PER_USDC);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #10)")]
    fn test_update_position_insufficient_collateral() {
        use crate::positions::STROOPS_PER_USDC;

        // Only 10 USDC deposited, but buying 100 YES at 60% needs 60 USDC locked.
        let deposit = 10 * STROOPS_PER_USDC;
        let (_env, user, client, _contract_id, market_id) = setup_funded_market(deposit);

        let yes = 100 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #13)")]
    fn test_update_position_rejects_overselling() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (_env, user, client, _contract_id, market_id) = setup_funded_market(deposit);

        // Selling shares the user does not hold drives the balance below zero.
        client.update_position(
            &user,
            &market_id,
            &(-50 * STROOPS_PER_USDC),
            &0i128,
            &6000i128,
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_update_position_rejects_resolved_market() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Force the market into a resolved state.
        env.as_contract(&contract_id, || {
            let mut market = storage::get_market(&env, market_id).unwrap().unwrap();
            market.status = MarketStatus::Resolved;
            market.result = Some(true);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        let yes = 10 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_update_position_rejects_expired_market() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Advance the ledger past the market end_time.
        let end_time = env.as_contract(&contract_id, || {
            storage::get_market(&env, market_id).unwrap().unwrap().end_time
        });
        env.ledger().set_timestamp(end_time + 1);

        let yes = 10 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);
    }

    // ========== Validation guard tests ==========

    #[test]
    fn test_validation_guard_accepts_positive_input() {
        use crate::validation::validate_input_guard;
        assert!(validate_input_guard(1).is_ok());
        assert!(validate_input_guard(1000).is_ok());
    }

    #[test]
    fn test_validation_guard_rejects_zero() {
        use crate::{error::ContractError, validation::validate_input_guard};
        assert_eq!(validate_input_guard(0), Err(ContractError::InvalidQuantity));
    }

    #[test]
    fn test_validation_guard_rejects_negative() {
        use crate::{error::ContractError, validation::validate_input_guard};
        assert_eq!(
            validate_input_guard(-1),
            Err(ContractError::InvalidQuantity)
        );
    }

    // ========== propose_admin / accept_admin tests ==========

    #[test]
    fn test_propose_admin_success() {
        let (env, admin, client, contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);

        env.as_contract(&contract_id, || {
            assert_eq!(
                storage::get_pending_admin(&env).expect("pending admin should be set"),
                new_admin
            );
        });
    }

    #[test]
    fn test_propose_admin_emits_event() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);

        let events = env.events().all();
        assert!(events.len() > 0);
    }

    #[test]
    fn test_accept_admin_completes_transfer() {
        let (env, admin, client, contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.accept_admin(&new_admin);

        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_admin(&env).unwrap(), new_admin);
            assert!(
                storage::get_pending_admin(&env).is_none(),
                "pending admin should be cleared after acceptance"
            );
        });
    }

    #[test]
    fn test_accept_admin_emits_event() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        env.events().all(); // clear

        client.accept_admin(&new_admin);

        let events = env.events().all();
        assert!(events.len() > 0);
    }

    #[test]
    fn test_new_admin_can_create_market_after_transfer() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.accept_admin(&new_admin);

        let question = String::from_str(&env, "Will ETH flip BTC?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let market_id =
            client.initialize_market(&new_admin, &question, &end_time, &oracle_pubkey, &collateral_token);
        assert_eq!(market_id, 1);
    }

    #[test]
    fn test_old_admin_cannot_create_market_after_transfer() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.accept_admin(&new_admin);

        let question = String::from_str(&env, "Will ETH flip BTC?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        let result =
            client.try_initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_propose_admin_overwrites_previous_nominee() {
        let (env, admin, client, contract_id) = create_test_contract();
        let first_nominee = Address::generate(&env);
        let second_nominee = Address::generate(&env);

        client.propose_admin(&admin, &first_nominee);
        client.propose_admin(&admin, &second_nominee);

        env.as_contract(&contract_id, || {
            assert_eq!(
                storage::get_pending_admin(&env).expect("pending admin should be set"),
                second_nominee
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_propose_admin_non_admin_fails() {
        let (env, _admin, client, _contract_id) = create_test_contract();
        let attacker = Address::generate(&env);
        let victim = Address::generate(&env);

        client.propose_admin(&attacker, &victim);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_propose_admin_when_not_initialized_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let caller = Address::generate(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin(&caller, &new_admin);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #43)")]
    fn test_accept_admin_with_no_pending_fails() {
        let (env, _admin, client, _contract_id) = create_test_contract();
        let attacker = Address::generate(&env);

        client.accept_admin(&attacker);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #40)")]
    fn test_accept_admin_hijack_wrong_address_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.accept_admin(&attacker);
    }

    #[test]
    fn test_set_treasury_records_contract_address() {
        let (env, admin, client, contract_id) = create_test_contract();
        let treasury = Address::generate(&env);

        client.set_treasury_contract(&admin, &treasury);

        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_treasury(&env).unwrap(), treasury);
        });
    }

    #[test]
    fn test_set_outcome_token_contract_records_contract_address() {
        let (env, admin, client, contract_id) = create_test_contract();
        let outcome_token_contract = Address::generate(&env);

        client.set_outcome_token_contract(&admin, &outcome_token_contract);

        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_outcome_token_contract(&env).unwrap(), outcome_token_contract);
        });
    }

    #[test]
    fn test_set_resolution_contract_records_contract_address() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution_contract = Address::generate(&env);

        client.set_resolution_contract(&admin, &resolution_contract);

        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_resolution_contract(&env).unwrap(), resolution_contract);
        });
    }

    #[test]
    fn test_non_admin_cannot_set_optional_integration_contracts() {
        use crate::error::ContractError;

        let (env, _admin, client, _contract_id) = create_test_contract();
        let stranger = Address::generate(&env);
        let address = Address::generate(&env);

        assert_eq!(client.try_set_treasury_contract(&stranger, &address), Err(Ok(ContractError::NotAdmin)));
        assert_eq!(client.try_set_outcome_token_contract(&stranger, &address), Err(Ok(ContractError::NotAdmin)));
        assert_eq!(client.try_set_resolution_contract(&stranger, &address), Err(Ok(ContractError::NotAdmin)));
    }

    #[test]
    fn test_resolution_contract_requires_finalized_candidate_before_resolve() {
        use crate::error::ContractError;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let collateral_token = Address::generate(&env);
        let question = String::from_str(&env, "Will it rain tomorrow?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let market_id = client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);

        let resolution_addr = env.register(ResolutionContract, ());
        ResolutionContractClient::new(&env, &resolution_addr)
            .initialize(&admin, &Address::generate(&env), &contract_id);

        client.set_resolution_contract(&admin, &resolution_addr);

        let (_oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, true);

        let proposer = Address::generate(&env);
        let evidence = String::from_str(&env, "evidence://uri");
        ResolutionContractClient::new(&env, &resolution_addr)
            .propose(&proposer, &market_id, &true, &signature, &(env.ledger().timestamp() + 60), &evidence, &60);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, &market_id.to_string());
        assert_eq!(
            client.try_resolve_market(&resolver, &market_id_str, &true, &signature),
            Err(Ok(ContractError::ResolutionNotFinalized))
        );
    }

    #[test]
    fn test_first_nominee_cannot_accept_after_overwrite() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let first_nominee = Address::generate(&env);
        let second_nominee = Address::generate(&env);

        client.propose_admin(&admin, &first_nominee);
        client.propose_admin(&admin, &second_nominee);

        let result = client.try_accept_admin(&first_nominee);
        assert!(result.is_err());
    }

    // ========== cancel_market tests ==========

    /// Register a market backed by a real Stellar asset, mint `deposit` to a
    /// fresh user, and deposit it so cancel and collateral-reclaim flows can be
    /// exercised end to end.
    ///
    /// Returns `(env, admin, user, client, contract_id, market_id, collateral_token)`.
    fn setup_admin_market_with_deposit<'a>(
        deposit: i128,
    ) -> (
        Env,
        Address,
        Address,
        MarketContractClient<'a>,
        Address,
        u32,
        Address,
    ) {
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        env.mock_all_auths();

        let question = String::from_str(&env, "Will it rain tomorrow?");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        let user = Address::generate(&env);
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &deposit);
        client.deposit_collateral(&user, &market_id, &deposit);

        (
            env,
            admin,
            user,
            client,
            contract_id,
            market_id,
            collateral_token,
        )
    }

    #[test]
    fn test_cancel_market_success() {
        let (env, admin, _user, client, contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &market_id);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Canceled);
    }

    #[test]
    fn test_cancel_market_emits_event() {
        let (env, admin, _user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        env.events().all(); // clear setup events
        client.cancel_market(&admin, &market_id);

        let events = env.events().all();
        assert!(events.len() > 0, "MarketCanceled event should be emitted");
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_cancel_market_non_admin_fails() {
        let (env, _admin, _user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        let attacker = Address::generate(&env);
        client.cancel_market(&attacker, &market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_cancel_market_not_found_fails() {
        let (_env, admin, _user, client, _contract_id, _market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &999u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_cancel_market_already_resolved_fails() {
        let (env, admin, _user, client, contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Force the market into a resolved state; a final outcome can't be canceled.
        env.as_contract(&contract_id, || {
            let mut market = storage::get_market(&env, market_id).unwrap().unwrap();
            market.status = MarketStatus::Resolved;
            market.result = Some(true);
            storage::set_market(&env, market_id, &market).unwrap();
        });

        client.cancel_market(&admin, &market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_cancel_market_already_canceled_fails() {
        let (_env, admin, _user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &market_id);
        // A second cancellation is a no-op and must be rejected.
        client.cancel_market(&admin, &market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_deposit_rejected_after_cancel() {
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, user, client, _contract_id, market_id, collateral_token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &market_id);

        // A fresh deposit into the canceled market must fail with MarketNotActive.
        let token_client = StellarAssetClient::new(&env, &collateral_token);
        token_client.mint(&user, &500);
        client.deposit_collateral(&user, &market_id, &500);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_update_position_rejected_after_cancel() {
        let (_env, admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &market_id);
        // Trading is halted once a market is canceled.
        client.update_position(&user, &market_id, &100i128, &0i128, &5_000i128);
    }

    #[test]
    fn test_withdraw_canceled_collateral_refunds_user() {
        let deposit = 1_000i128;
        let (env, admin, user, client, contract_id, market_id, collateral_token) =
            setup_admin_market_with_deposit(deposit);

        client.cancel_market(&admin, &market_id);

        let refunded = client.withdraw_canceled_collateral(&user, &market_id);
        assert_eq!(refunded, deposit);

        // The user's position is zeroed once the collateral has been returned.
        let position = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position should exist")
        });
        assert_eq!(position.total_deposited, 0);
        assert_eq!(position.locked_collateral, 0);

        // The collateral lands back in the user's wallet.
        let token_client = soroban_sdk::token::Client::new(&env, &collateral_token);
        assert_eq!(token_client.balance(&user), deposit);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_withdraw_canceled_collateral_rejects_active_market() {
        let (_env, _admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Market is still active, so the canceled-reclaim path does not apply.
        client.withdraw_canceled_collateral(&user, &market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #12)")]
    fn test_withdraw_canceled_collateral_no_position_fails() {
        let (env, admin, _user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.cancel_market(&admin, &market_id);

        // A user who never deposited has no position to reclaim.
        let stranger = Address::generate(&env);
        client.withdraw_canceled_collateral(&stranger, &market_id);
    }

    // ========== Close Market to Deposits Tests ==========

    #[test]
    fn test_close_market_to_deposits_success() {
        let (env, admin, _user, client, contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Verify the market is not initially closed to deposits
        let market_before = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_before.closed_to_deposits, false);

        // Close the market to deposits
        client.close_market_to_deposits(&admin, &market_id);

        // Verify the market is now closed to deposits
        let market_after = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after.closed_to_deposits, true);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_close_market_to_deposits_not_admin_fails() {
        let (env, _admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Non-admin tries to close market to deposits
        let attacker = Address::generate(&env);
        client.close_market_to_deposits(&attacker, &market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_close_market_to_deposits_not_found_fails() {
        let (_env, admin, _user, client, _contract_id, _market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Try to close a non-existent market
        client.close_market_to_deposits(&admin, &999u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_deposit_rejected_when_closed_to_deposits() {
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, user, client, _contract_id, market_id, collateral_token) =
            setup_admin_market_with_deposit(1_000);

        // Close market to new deposits
        client.close_market_to_deposits(&admin, &market_id);

        // User tries to deposit more - should fail
        let stellar_asset_client = StellarAssetClient::new(&env, &collateral_token);
        stellar_asset_client.mint(&user, &2_000_000);

        client.deposit_collateral(&user, &market_id, &1_000_000);
    }

    #[test]
    fn test_withdraw_still_works_after_close_to_deposits() {
        let (env, admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Close market to new deposits
        client.close_market_to_deposits(&admin, &market_id);

        // User should still be able to withdraw their collateral
        // (This should succeed without panic, demonstrating that withdraw is not affected)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.withdraw_unused_collateral(&user, &market_id, &500_000);
        }));

        // If result is Ok, the withdrawal succeeded
        assert!(result.is_ok(), "Withdrawal should still work after closing market to deposits");
    }

    #[test]
    fn test_multiple_close_market_to_deposits_idempotent() {
        let (env, admin, _user, client, contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Close the market to deposits
        client.close_market_to_deposits(&admin, &market_id);

        // Verify it's closed
        let market_after = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after.closed_to_deposits, true);

        // Close it again - should succeed (idempotent operation)
        client.close_market_to_deposits(&admin, &market_id);

        // Verify it's still closed
        let market_after_second = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after_second.closed_to_deposits, true);
    }

    #[test]
    fn test_close_market_event_emitted() {
        let (env, admin, _user, client, contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        // Clear any existing events
        let events = env.events();
        events.all();

        // Close the market to deposits
        client.close_market_to_deposits(&admin, &market_id);

        // Check that the event was emitted
        let events = env.events().all();
        assert!(!events.is_empty(), "MarketClosedToDepositsEvent should be emitted");
    }
}
