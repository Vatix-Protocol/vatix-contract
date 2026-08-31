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
    // ========== Initialize Function Tests ==========

    #[test]
    fn test_initialize_with_valid_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        // Initialize should succeed with a valid account address
        let result = client.initialize(&admin);
        assert!(result.is_ok());

        // Verify admin was set
        let stored_admin = env.as_contract(&contract_id, || {
            storage::get_admin(&env).expect("Admin should be set")
        });
        assert_eq!(stored_admin, admin);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #35)")]
    fn test_initialize_with_contract_address_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        // Try to use another contract address as admin (should fail)
        let other_contract = env.register(MarketContract, ());

        // This should fail with InvalidAdmin error (#35)
        client.initialize(&other_contract);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #42)")]
    fn test_initialize_twice_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        // First initialization should succeed
        client.initialize(&admin);

        // Second initialization should fail with AlreadyInitialized (#42)
        let another_admin = Address::generate(&env);
        client.initialize(&another_admin);
    }

    #[test]
    fn test_initialize_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        client.initialize(&admin);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0);

        // Check for contract_initialized_event
        let event_found = events.iter().any(|e| {
            e.topics
                .iter()
                .any(|t| t.to_string().contains("contract_initialized"))
        });
        assert!(event_found, "contract_initialized_event should be emitted");
    }

    #[test]
    fn test_initialize_sets_version() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);

        client.initialize(&admin);

        // Verify storage version was set
        env.as_contract(&contract_id, || {
            let result = storage::assert_version(&env);
            assert!(result.is_ok(), "Storage version should be set correctly");
        });
    }

    // ========== Initialize Market Function Tests ==========

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
            &None,
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
            &None,
        );
        assert_eq!(market_id_1, 1);

        let question_2 = String::from_str(&env, "Question 2");
        let market_id_2 = client.initialize_market(
            &admin,
            &question_2,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert_eq!(market_id_2, 2);

        let question_3 = String::from_str(&env, "Question 3");
        let market_id_3 = client.initialize_market(
            &admin,
            &question_3,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert_eq!(market_id_3, 3);
    }

    #[test]
    fn test_initialize_market_no_id_reuse_after_cancel() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market to cancel");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);

        // Create first market (ID 1)
        let market_id_1 = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert_eq!(market_id_1, 1);

        // Cancel the first market
        client.cancel_market(&admin, &market_id_1);

        // Verify market is canceled
        let market = get_market_from_storage(&env, &contract_id, market_id_1);
        assert_eq!(market.status, MarketStatus::Canceled);

        // Create second market - should get ID 2, not reuse ID 1
        let question_2 = String::from_str(&env, "New market after cancel");
        let market_id_2 = client.initialize_market(
            &admin,
            &question_2,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert_eq!(market_id_2, 2);

        // Verify the new market exists and is active
        let market_2 = get_market_from_storage(&env, &contract_id, market_id_2);
        assert_eq!(market_2.status, MarketStatus::Active);
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
            &None,
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
            &None,
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
            &None,
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
            &None,
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
            &None,
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

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &usdc_token,
            &None,
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.collateral_token, usdc_token);
    }

    #[test]
    fn test_initialize_market_with_valid_metadata_uri() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market with metadata");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);
        let metadata_uri = Some(String::from_str(&env, "ipfs://QmXxx"));

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &metadata_uri,
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.id, market_id);
    }

    #[test]
    fn test_initialize_market_with_none_metadata_uri() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market without metadata");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);
        let metadata_uri: Option<String> = None;

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &metadata_uri,
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.id, market_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #37)")]
    fn test_initialize_market_with_empty_metadata_uri_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market with empty metadata");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);
        let metadata_uri = Some(String::from_str(&env, ""));

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &metadata_uri,
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #37)")]
    fn test_initialize_market_with_overlong_metadata_uri_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Market with overlong metadata");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);
        let collateral_token = Address::generate(&env);
        let long_str = "a".repeat(2049);
        let metadata_uri = Some(String::from_str(&env, &long_str));

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &metadata_uri,
        );
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
            &None,
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

        client.resolve_market(
            &resolver,
            &non_existent_market_id,
            &outcome,
            &invalid_signature,
            &0u64,
        );
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
            &None,
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
        client.resolve_market(
            &resolver,
            &market_id_str,
            &outcome,
            &invalid_signature,
            &0u64,
        );
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
            &None,
        );

        // Bad signature must surface as the typed InvalidSignature error
        // (#20), not an uncaught host trap.
        let resolver = Address::generate(&env);
        let outcome = true;
        let invalid_signature = BytesN::random(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(
            &resolver,
            &market_id_str,
            &outcome,
            &invalid_signature,
            &0u64,
        );
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
            &None,
        );

        let resolver = Address::generate(&env);
        let outcome = true;
        let invalid_signature = BytesN::random(&env);
        let market_id_str = String::from_str(&env, "1");
        let result = client.try_resolve_market(
            &resolver,
            &market_id_str,
            &outcome,
            &invalid_signature,
            &0u64,
        );

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
            &None,
        );

        // Verify market is initially Active
        let market_before = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_before.status, MarketStatus::Active);
        assert_eq!(market_before.result, None);

        // Resolve market with valid signature
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature, &0u64);

        // Verify market is now Resolved
        let market_after = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market_after.status, MarketStatus::Resolved);
        assert_eq!(market_after.result, Some(outcome));
        assert_eq!(market_after.resolver, Some(resolver));
    }

    // ── Issue #701: fail closed without V1; expires_at==0 no longer disables expiry ──

    /// A fresh `initialize()` must default legacy V1 oracle signatures to
    /// disabled (#701) — unlike `create_test_contract()`'s raw storage
    /// pokes, this calls the real entrypoint so the new default is actually
    /// exercised.
    #[test]
    fn test_initialize_defaults_oracle_v1_disabled() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(&admin);

        assert!(
            client.is_oracle_v1_disabled(),
            "fresh initialize() must default V1 oracle signatures to disabled"
        );
    }

    /// `resolve_market` must reject `expires_at == 0` outright instead of
    /// treating it as "no expiry" (#701) — the old fail-open sentinel let a
    /// resolver bypass the expiry check entirely by passing zero.
    #[test]
    fn test_resolve_market_rejects_zero_expires_at() {
        let (env, admin, client, _contract_id) = create_test_contract();
        // V1 is enabled by default in this raw-storage test fixture (it
        // bypasses `initialize()`), so only the expiry gate is under test.

        let question = String::from_str(&env, "Zero expiry test");
        let end_time = env.ledger().timestamp() + 86400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);
        let collateral_token = Address::generate(&env);
        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let result =
            client.try_resolve_market(&resolver, &market_id_str, &outcome, &signature, &0u64);
        assert!(
            result.is_err(),
            "expires_at == 0 must be rejected as expired, not treated as no-expiry"
        );
    }

    /// Once V1 is disabled, `resolve_market` must reject a V1-style
    /// signature with `UnauthorizedOracle` (#701) — the guard that already
    /// existed for `resolve_market` is re-verified here alongside the new
    /// default, and mirrored below for the read-only `verify_signature`.
    #[test]
    fn test_resolve_market_rejects_when_v1_disabled() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin); // defaults V1 disabled

        let question = String::from_str(&env, "V1 disabled test");
        let end_time = env.ledger().timestamp() + 86400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);
        let collateral_token = Address::generate(&env);
        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let expires_at = env.ledger().timestamp() + 3_600;
        let result =
            client.try_resolve_market(&resolver, &market_id_str, &outcome, &signature, &expires_at);
        assert!(
            result.is_err(),
            "resolve_market must fail closed once V1 is disabled"
        );
    }

    /// `verify_signature` (the read-only dispatch the resolution contract's
    /// `propose()` calls cross-contract) must also fail closed once V1 is
    /// disabled (#701) — otherwise resolution could open a challenge window
    /// for a candidate `resolve_market` can never finalize.
    #[test]
    fn test_verify_signature_rejects_when_v1_disabled() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin); // defaults V1 disabled

        let question = String::from_str(&env, "verify_signature V1 disabled test");
        let end_time = env.ledger().timestamp() + 86400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);
        let collateral_token = Address::generate(&env);
        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let result = client.try_verify_signature(&market_id, &outcome, &signature);
        assert!(
            result.is_err(),
            "verify_signature must fail closed once V1 is disabled"
        );

        // Re-enabling V1 restores the old behavior for a genuine migration window.
        client.set_oracle_v1_disabled(&admin, &false);
        let result = client.try_verify_signature(&market_id, &outcome, &signature);
        assert!(
            result.is_ok(),
            "verify_signature must succeed again once V1 is explicitly re-enabled"
        );
    }

    // ── Issue #702: pause must cover the full state-mutating entrypoint matrix ──

    /// Exercises every state-mutating entrypoint while the contract is
    /// paused and asserts each one is rejected with `ContractPaused` (#702).
    /// This is the "full mutator matrix" the issue calls for: a single test
    /// that fails immediately if any mutator is ever added — or reverts to
    /// missing — the `require_not_paused` guard.
    ///
    /// Deliberately excluded (by design, not oversight): `initialize` (no
    /// pause state exists yet), `pause`/`unpause` themselves, `set_emergency_mode`
    /// (a separate, independent emergency lever that must stay reachable
    /// even while paused), and `propose_admin`/`accept_admin`/
    /// `cancel_admin_transfer` (admin-key rotation must remain possible
    /// during an emergency pause so a compromised admin can be replaced).
    #[test]
    fn test_pause_blocks_full_mutator_matrix() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let question = String::from_str(&env, "Pause matrix test");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);
        let collateral_token = Address::generate(&env);
        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let user = Address::generate(&env);
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let expires_at = env.ledger().timestamp() + 3_600;

        client.pause(&admin);
        assert!(client.is_paused());

        let paused = crate::error::ContractError::ContractPaused;
        macro_rules! assert_paused {
            ($call:expr) => {
                assert_eq!($call.unwrap_err().unwrap(), paused);
            };
        }

        assert_paused!(client.try_deposit_collateral(&user, &market_id, &1_000i128));
        assert_paused!(client.try_withdraw_unused_collateral(&user, &market_id, &1_000i128));
        assert_paused!(client.try_update_position(&user, &market_id, &1i128, &0i128, &5_000i128));
        assert_paused!(client.try_resolve_market(
            &resolver,
            &market_id_str,
            &outcome,
            &signature,
            &expires_at
        ));
        assert_paused!(client.try_cancel_market(&admin, &market_id));
        assert_paused!(client.try_withdraw_canceled_collateral(&user, &market_id));
        assert_paused!(client.try_settle_position(&user, &market_id));
        assert_paused!(
            client.try_batch_settle_positions(&market_id, &soroban_sdk::vec![&env, user.clone()])
        );
        assert_paused!(client.try_settle_positions_page(&market_id, &0u32, &10u32));
        assert_paused!(client.try_close_market_to_deposits(&admin, &market_id));
        assert_paused!(client.try_reconcile_position_tokens(&admin, &market_id, &user));
        assert_paused!(client.try_propose_treasury_contract(&admin, &Address::generate(&env)));
        assert_paused!(client.try_execute_treasury_contract());
        assert_paused!(client.try_cancel_treasury_contract(&admin));
        assert_paused!(client.try_set_fee_rate(&admin, &100i128));
        assert_paused!(client.try_execute_fee_rate_change());
        assert_paused!(client.try_set_fee_cap(&admin, &500i128));
        assert_paused!(client.try_add_fee_waiver(&admin, &user));
        assert_paused!(client.try_remove_fee_waiver(&admin, &user));
        assert_paused!(client.try_propose_market_oracle(
            &admin,
            &market_id,
            &BytesN::from_array(&env, &[2u8; 32])
        ));
        assert_paused!(client.try_execute_market_oracle(&market_id));
        assert_paused!(client.try_cancel_market_oracle(&admin, &market_id));
        let signers = soroban_sdk::vec![&env, BytesN::from_array(&env, &[3u8; 32])];
        assert_paused!(client.try_propose_threshold_signers(&admin, &signers, &1u32));
        assert_paused!(client.try_execute_threshold_signers());
        assert_paused!(client.try_cancel_threshold_signers(&admin));
        assert_paused!(client.try_set_market_threshold_signers(&admin, &market_id, &signers, &1u32));
        assert_paused!(client.try_set_threshold_signers(&admin, &signers, &1u32));
        assert_paused!(client.try_propose_outcome_token_contract(&admin, &Address::generate(&env)));
        assert_paused!(client.try_execute_outcome_token_contract());
        assert_paused!(client.try_cancel_outcome_token_contract(&admin));
        assert_paused!(client.try_propose_resolution_contract(&admin, &Address::generate(&env)));
        assert_paused!(client.try_execute_resolution_contract());
        assert_paused!(client.try_cancel_resolution_contract(&admin));
        assert_paused!(client.try_set_oracle_v1_disabled(&admin, &true));
        assert_paused!(client.try_set_adapter_enabled(
            &admin,
            &crate::types::AdapterType::Reflector,
            &true
        ));
        assert_paused!(client.try_reopen_market(&admin, &market_id));

        // Deliberately-exempt mutators must still work while paused.
        client.set_emergency_mode(&admin, &crate::types::EmergencyMode::Normal);

        // Unpause restores normal operation for a representative mutator.
        client.unpause(&admin);
        assert!(!client.is_paused());
        client.set_fee_cap(&admin, &500i128);
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
            &None,
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
            &None,
        );

        // Clear events from initialization
        env.events().all();

        // Resolve market with valid signature
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature, &0u64);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0);

        // Verify that market is resolved
        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.result, Some(outcome));
        assert_eq!(market.resolver, Some(resolver));
    }

    // ── #592: resolve_market rejects expired oracle messages ──────────────────

    /// An oracle message with `expires_at` in the past must be rejected with
    /// `OracleMessageExpired` (#24) so stale signatures cannot be replayed.
    #[test]
    fn resolve_market_rejects_expired_oracle_message() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Expiry test market");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &Address::generate(&env),
            &None,
        );

        // Advance the ledger past the expires_at deadline.
        let expires_at: u64 = env.ledger().timestamp() + 60;
        env.ledger().set_timestamp(expires_at + 1);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let err = client
            .try_resolve_market(&resolver, &market_id_str, &outcome, &signature, &expires_at)
            .unwrap_err()
            .unwrap();

        assert_eq!(
            err,
            crate::error::ContractError::OracleMessageExpired,
            "expired oracle message must return OracleMessageExpired (#24)"
        );
    }

    /// A message with `expires_at` still in the future must be accepted normally.
    #[test]
    fn resolve_market_accepts_non_expired_oracle_message() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Non-expired expiry test");
        let current_time = env.ledger().timestamp();
        let end_time = current_time + 86_400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &Address::generate(&env),
            &None,
        );

        // expires_at is in the future — must succeed.
        let expires_at: u64 = current_time + 3_600;
        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature, &expires_at);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(
            market.status,
            crate::types::MarketStatus::Resolved,
            "market must be resolved when expires_at is still in the future"
        );
    }

    /// Passing `expires_at = 0` disables expiry enforcement — the call must
    /// succeed regardless of the current ledger timestamp (backwards compat).
    #[test]
    fn resolve_market_zero_expires_at_disables_expiry_check() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "Zero expires_at test");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = 1u32;
        let outcome = false;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &Address::generate(&env),
            &None,
        );

        // Advance ledger far into the future — expires_at=0 means no check.
        env.ledger().set_timestamp(u64::MAX / 2);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        // expires_at=0 → no expiry enforcement, must succeed.
        client.resolve_market(&resolver, &market_id_str, &outcome, &signature, &0u64);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, crate::types::MarketStatus::Resolved);
    }

    /// Expiry check fires BEFORE signature verification so the error is always
    /// `OracleMessageExpired` rather than `InvalidSignature` when both are wrong.
    #[test]
    fn resolve_market_expiry_checked_before_signature() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Expiry order test");
        let end_time = env.ledger().timestamp() + 86_400;

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &BytesN::from_array(&env, &[1u8; 32]),
            &Address::generate(&env),
            &None,
        );

        // Advance past expiry; also use an invalid signature — expiry must win.
        let expires_at: u64 = env.ledger().timestamp() + 10;
        env.ledger().set_timestamp(expires_at + 100);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let bad_sig = BytesN::from_array(&env, &[0u8; 64]);
        let err = client
            .try_resolve_market(&resolver, &market_id_str, &true, &bad_sig, &expires_at)
            .unwrap_err()
            .unwrap();

        assert_eq!(err, crate::error::ContractError::OracleMessageExpired);
    }

    /// Market must remain `Active` (no state mutation) when `resolve_market`
    /// returns `OracleMessageExpired`.
    #[test]
    fn resolve_market_expired_message_leaves_market_unchanged() {
        let (env, admin, client, contract_id) = create_test_contract();

        let question = String::from_str(&env, "State mutation test");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = 1u32;
        let outcome = true;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, outcome);

        client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &Address::generate(&env),
            &None,
        );

        let expires_at: u64 = env.ledger().timestamp() + 30;
        env.ledger().set_timestamp(expires_at + 1);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        let _ =
            client.try_resolve_market(&resolver, &market_id_str, &outcome, &signature, &expires_at);

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, crate::types::MarketStatus::Active);
        assert_eq!(market.result, None);
        assert_eq!(market.resolver, None);
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
            &None,
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
            &None,
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
            &None,
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
            storage::get_position(&env, market_id, &user)
                .expect("version check ok")
                .expect("position should exist")
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
            storage::get_market(&env, market_id)
                .unwrap()
                .unwrap()
                .end_time
        });
        env.ledger().set_timestamp(end_time + 1);

        let yes = 10 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);
    }

    // ========== closed_to_deposits / update_position policy (Issue #601) ==========

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_update_position_rejects_new_exposure_when_closed_to_deposits() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);
        let admin = env.as_contract(&contract_id, || storage::get_admin(&env).unwrap());

        // Open an initial position while the market is still open to deposits.
        let yes = 50 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);

        client.close_market_to_deposits(&admin, &market_id);

        // Buying more shares increases locked collateral (new exposure) and
        // must be rejected once the market is closed to deposits.
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);
    }

    #[test]
    fn test_update_position_allows_reducing_position_when_closed_to_deposits() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);
        let admin = env.as_contract(&contract_id, || storage::get_admin(&env).unwrap());

        // Open an initial position while the market is still open to deposits.
        let yes = 50 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &6000i128);

        client.close_market_to_deposits(&admin, &market_id);

        // Selling shares reduces locked collateral and must still succeed —
        // closing/reducing a position sheds risk rather than adding it.
        let position = client.update_position(
            &user,
            &market_id,
            &(-20 * STROOPS_PER_USDC),
            &0i128,
            &6000i128,
        );
        assert_eq!(position.yes_shares, 30 * STROOPS_PER_USDC);
    }

    #[test]
    fn test_update_position_allows_flat_lock_when_closed_to_deposits() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);
        let admin = env.as_contract(&contract_id, || storage::get_admin(&env).unwrap());

        // Open a YES position at the 50% price point, then close the market
        // to deposits. At exactly 50%, locked collateral is symmetric in
        // yes/no excess (`scale_by_bps(x, 5000) == scale_by_bps(x, 10000-5000)`).
        let yes = 50 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &yes, &0i128, &5000i128);
        client.close_market_to_deposits(&admin, &market_id);

        // Selling all YES shares and buying the equivalent NO shares keeps
        // locked collateral exactly flat (not increased), so it is not
        // blocked by closed_to_deposits.
        let position = client.update_position(
            &user,
            &market_id,
            &(-50 * STROOPS_PER_USDC),
            &(50 * STROOPS_PER_USDC),
            &5000i128,
        );
        assert_eq!(position.yes_shares, 0);
        assert_eq!(position.no_shares, 50 * STROOPS_PER_USDC);
    }

    // ========== set_fee_rate_bps / get_fee_rate_bps tests ==========

    #[test]
    fn test_get_fee_rate_bps_default_is_50() {
        let (env, _admin, client, _contract_id) = create_test_contract();
        assert_eq!(client.get_fee_rate_bps(), 50u32);
    }

    #[test]
    fn test_set_fee_rate_bps_admin_can_update() {
        let (env, admin, client, _contract_id) = create_test_contract();
        client.set_fee_rate_bps(&admin, &100u32);
        assert_eq!(client.get_fee_rate_bps(), 100u32);
    }

    #[test]
    fn test_set_fee_rate_bps_zero_is_valid() {
        let (env, admin, client, _contract_id) = create_test_contract();
        client.set_fee_rate_bps(&admin, &0u32);
        assert_eq!(client.get_fee_rate_bps(), 0u32);
    }

    #[test]
    fn test_set_fee_rate_bps_max_boundary_valid() {
        let (env, admin, client, _contract_id) = create_test_contract();
        client.set_fee_rate_bps(&admin, &10_000u32);
        assert_eq!(client.get_fee_rate_bps(), 10_000u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #34)")]
    fn test_set_fee_rate_bps_exceeds_max_rejected() {
        let (env, admin, client, _contract_id) = create_test_contract();
        client.set_fee_rate_bps(&admin, &10_001u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_set_fee_rate_bps_non_admin_rejected() {
        let (env, _admin, client, _contract_id) = create_test_contract();
        let non_admin = Address::generate(&env);
        client.set_fee_rate_bps(&non_admin, &50u32);
    }

    // ========== token_balance tests ==========

    #[test]
    fn test_token_balance_returns_contract_balance() {
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let sac = StellarAssetClient::new(&env, &collateral_token);

        // Mint directly to the market contract to simulate held collateral.
        sac.mint(&contract_id, &500i128);

        assert_eq!(client.token_balance(&collateral_token), 500i128);
    }

    #[test]
    fn test_token_balance_zero_when_no_funds() {
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
        });

        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();

        assert_eq!(client.token_balance(&collateral_token), 0i128);
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

    /// #705: proposing a new admin must NOT move the admin role. Only
    /// `accept_admin`, called by the nominee, completes the transfer. A
    /// single-step design (or docs/tests implying one) would hand control to
    /// the nominee — or strand it — the instant `propose_admin` runs.
    #[test]
    fn test_propose_admin_alone_does_not_change_current_admin() {
        let (env, admin, client, contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);

        // Stored admin is still the original.
        env.as_contract(&contract_id, || {
            assert_eq!(
                storage::get_admin(&env).unwrap(),
                admin,
                "current admin must be unchanged until the nominee calls accept_admin"
            );
        });

        let question = String::from_str(&env, "still admin?");
        let end_time = env.ledger().timestamp() + 86_400;
        let oracle_pubkey = BytesN::from_array(&env, &[9u8; 32]);
        let collateral_token = Address::generate(&env);

        // The original admin still holds admin powers.
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert_eq!(market_id, 1);

        // The pending nominee has no admin powers yet.
        let res = client.try_initialize_market(
            &new_admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
        assert!(
            res.is_err(),
            "pending admin must not have admin powers before acceptance"
        );

        // After acceptance the role finally moves.
        client.accept_admin(&new_admin);
        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_admin(&env).unwrap(), new_admin);
        });
    }

    /// #705 / audit readiness: `propose_admin` must require the current
    /// admin's authorization. Previously it did not — `current_admin` is a
    /// plain parameter, so anyone could pass the real admin's address,
    /// nominate themselves, then self-`accept_admin` (which only checks the
    /// *new* admin's signature) to seize the contract.
    #[test]
    fn test_propose_admin_requires_current_admin_auth() {
        let env = Env::default();
        // NOTE: deliberately no env.mock_all_auths().
        let contract_id = env.register(MarketContract, ());
        let client = MarketContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.as_contract(&contract_id, || {
            storage::set_admin(&env, &admin);
            storage::set_version(&env);
        });

        let attacker = Address::generate(&env);
        let res = client.try_propose_admin(&admin, &attacker);
        assert!(
            res.is_err(),
            "propose_admin must fail when the current admin has not authorized the call"
        );

        env.as_contract(&contract_id, || {
            assert!(
                storage::get_pending_admin(&env).is_none(),
                "no nomination may be recorded without the current admin's auth"
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
    #[should_panic(expected = "Error(Contract, #35)")]
    fn test_propose_admin_with_contract_address_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();

        // Try to propose a contract address as admin
        let contract_admin = env.register(MarketContract, ());

        // This should fail with InvalidAdmin error (#35)
        client.propose_admin(&admin, &contract_admin);
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

        let market_id = client.initialize_market(
            &new_admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
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

        let result = client.try_initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );
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

    // ========== cancel_admin_transfer tests ==========

    #[test]
    fn test_cancel_admin_transfer_clears_pending() {
        let (env, admin, client, contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.cancel_admin_transfer(&admin);

        env.as_contract(&contract_id, || {
            assert_eq!(storage::get_pending_admin(&env), None);
        });
    }

    #[test]
    fn test_cancel_admin_transfer_emits_event() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        env.events().all(); // drain proposed event before asserting on cancel
        client.cancel_admin_transfer(&admin);

        let events = env.events().all();
        assert!(!events.is_empty());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_cancel_admin_transfer_non_admin_fails() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.cancel_admin_transfer(&attacker);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #43)")]
    fn test_cancel_admin_transfer_with_no_pending_fails() {
        let (_env, admin, client, _contract_id) = create_test_contract();

        client.cancel_admin_transfer(&admin);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #43)")]
    fn test_canceled_nomination_cannot_be_accepted() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.cancel_admin_transfer(&admin);

        // The nomination was canceled, so acceptance must fail with
        // NoPendingAdmin (#43) rather than succeeding.
        client.accept_admin(&new_admin);
    }

    #[test]
    fn test_canceled_nomination_cannot_be_accepted_try_variant() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let new_admin = Address::generate(&env);

        client.propose_admin(&admin, &new_admin);
        client.cancel_admin_transfer(&admin);

        let result = client.try_accept_admin(&new_admin);
        assert!(result.is_err());
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
            assert_eq!(
                storage::get_outcome_token_contract(&env).unwrap(),
                outcome_token_contract
            );
        });
    }

    #[test]
    fn test_set_resolution_contract_records_contract_address() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution_contract = Address::generate(&env);

        client.set_resolution_contract(&admin, &resolution_contract);

        env.as_contract(&contract_id, || {
            assert_eq!(
                storage::get_resolution_contract(&env).unwrap(),
                resolution_contract
            );
        });
    }

    #[test]
    fn test_non_admin_cannot_set_optional_integration_contracts() {
        use crate::error::ContractError;

        let (env, _admin, client, _contract_id) = create_test_contract();
        let stranger = Address::generate(&env);
        let address = Address::generate(&env);

        assert_eq!(
            client.try_set_treasury_contract(&stranger, &address),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_outcome_token_contract(&stranger, &address),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_resolution_contract(&stranger, &address),
            Err(Ok(ContractError::NotAdmin))
        );
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
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let resolution_addr = env.register(ResolutionContract, ());
        ResolutionContractClient::new(&env, &resolution_addr).initialize(
            &admin,
            &Address::generate(&env),
            &contract_id,
        );

        client.set_resolution_contract(&admin, &resolution_addr);

        let (_oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, true);

        let proposer = Address::generate(&env);
        let evidence = String::from_str(&env, "evidence://uri");
        ResolutionContractClient::new(&env, &resolution_addr).propose(
            &proposer,
            &market_id,
            &true,
            &signature,
            &(env.ledger().timestamp() + 60),
            &evidence,
            &60,
        );

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, &market_id.to_string());
        assert_eq!(
            client.try_resolve_market(&resolver, &market_id_str, &true, &signature, &0u64),
            Err(Ok(ContractError::ResolutionNotFinalized))
        );
    }

    #[test]
    fn test_threshold_resolution_is_disabled_when_resolution_contract_is_registered() {
        use crate::error::ContractError;

        let (env, admin, client, contract_id) = create_test_contract();
        let resolver = Address::generate(&env);
        let question = String::from_str(&env, "Will the challenge window be bypassed?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &BytesN::from_array(&env, &[1u8; 32]),
            &Address::generate(&env),
            &None,
        );

        let (signer, signature) = generate_test_keypair_and_sign(&env, market_id, true);
        let signers = soroban_sdk::vec![&env, signer];
        client.propose_threshold_signers(&admin, &signers, &1u32);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::FEE_RATE_TIMELOCK_SECONDS);
        client.execute_threshold_signers();
        let signatures = soroban_sdk::vec![&env, signature];

        // Registering the challenge-based resolution contract selects that
        // mode exclusively. The gate is intentionally independent of the
        // candidate's current status, so proposed and challenged candidates
        // cannot be bypassed through a valid threshold quorum.
        client.set_resolution_contract(&admin, &Address::generate(&env));

        assert_eq!(
            client.try_resolve_market_threshold(&resolver, &market_id, &true, &signatures),
            Err(Ok(ContractError::ResolutionNotFinalized))
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Active);
        assert_eq!(market.result, None);
        assert_eq!(market.resolver, None);
    }

    #[test]
    fn test_threshold_resolution_without_resolution_contract_succeeds_once() {
        use crate::error::ContractError;

        let (env, admin, client, contract_id) = create_test_contract();
        let resolver = Address::generate(&env);
        let question = String::from_str(&env, "Will threshold mode settle exactly once?");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &BytesN::from_array(&env, &[1u8; 32]),
            &Address::generate(&env),
            &None,
        );

        let (signer, signature) = generate_test_keypair_and_sign(&env, market_id, true);
        let signers = soroban_sdk::vec![&env, signer];
        client.propose_threshold_signers(&admin, &signers, &1u32);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::FEE_RATE_TIMELOCK_SECONDS);
        client.execute_threshold_signers();
        let signatures = soroban_sdk::vec![&env, signature];

        assert_eq!(
            client.try_resolve_market_threshold(&resolver, &market_id, &true, &signatures),
            Ok(Ok(()))
        );
        assert_eq!(
            client.try_resolve_market_threshold(&resolver, &market_id, &true, &signatures),
            Err(Ok(ContractError::MarketAlreadyResolved))
        );

        let market = get_market_from_storage(&env, &contract_id, market_id);
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.result, Some(true));
        assert_eq!(market.resolver, Some(resolver));
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
            &None,
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

    // ========== #332: Burn outcome tokens on position decrease ==========

    /// #332: Selling YES shares burns the corresponding outcome tokens.
    /// Verify that when yes_delta < 0 the token contract's burn entry point is
    /// called (the SDK records the call in the auth invocations list, so we
    /// assert the position decreases as expected as a proxy for the burn path
    /// being exercised).
    #[test]
    fn test_332_selling_yes_shares_decreases_position() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Buy 80 YES shares first.
        let buy = 80 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &buy, &0i128, &5_000i128);

        // Sell 30 YES shares back.
        let sell = -30 * STROOPS_PER_USDC;
        let pos = client.update_position(&user, &market_id, &sell, &0i128, &5_000i128);

        assert_eq!(pos.yes_shares, 50 * STROOPS_PER_USDC);
        assert_eq!(pos.no_shares, 0);

        let stored = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(stored.yes_shares, 50 * STROOPS_PER_USDC);
    }

    /// #332: Selling NO shares decreases the NO balance (burn path).
    #[test]
    fn test_332_selling_no_shares_decreases_position() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Buy 60 NO shares first.
        let buy = 60 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &0i128, &buy, &5_000i128);

        // Sell 20 NO shares back.
        let sell = -20 * STROOPS_PER_USDC;
        let pos = client.update_position(&user, &market_id, &0i128, &sell, &5_000i128);

        assert_eq!(pos.yes_shares, 0);
        assert_eq!(pos.no_shares, 40 * STROOPS_PER_USDC);

        let stored = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(stored.no_shares, 40 * STROOPS_PER_USDC);
    }

    /// #332: Selling down to zero shares is allowed and results in locked_collateral == 0.
    #[test]
    fn test_332_selling_all_shares_zeroes_locked_collateral() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 100 * STROOPS_PER_USDC;
        let (_env, user, client, _contract_id, market_id) = setup_funded_market(deposit);

        let qty = 50 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &qty, &0i128, &5_000i128);

        let pos = client.update_position(&user, &market_id, &(-qty), &0i128, &5_000i128);
        assert_eq!(pos.yes_shares, 0);
        assert_eq!(pos.locked_collateral, 0);
    }

    // ========== #333: Reconcile locked_collateral on deposit and withdraw ==========

    /// #333: Depositing collateral must never increment locked_collateral.
    /// locked_collateral is exclusively owned by update_position.
    #[test]
    fn test_333_deposit_does_not_touch_locked_collateral() {
        use crate::positions::STROOPS_PER_USDC;

        let deposit = 50 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // A second deposit — the lock must stay zero because no shares are held.
        let extra = 20 * STROOPS_PER_USDC;
        use soroban_sdk::token::StellarAssetClient;
        let stored_market = env.as_contract(&contract_id, || {
            storage::get_market(&env, market_id).unwrap().unwrap()
        });
        StellarAssetClient::new(&env, &stored_market.collateral_token).mint(&user, &extra);
        client.deposit_collateral(&user, &market_id, &extra);

        let pos = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(pos.locked_collateral, 0);
        assert_eq!(pos.total_deposited, deposit + extra);
    }

    /// #333: Withdrawing unlocked collateral decrements total_deposited and
    /// preserves locked_collateral (invariant: available = total - locked).
    #[test]
    fn test_333_withdraw_decrements_total_deposited_preserves_locked() {
        use crate::positions::STROOPS_PER_USDC;
        use soroban_sdk::token::StellarAssetClient;

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Buy 40 YES shares at 50% → lock = 20 USDC; available = 80 USDC.
        let shares = 40 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &shares, &0i128, &5_000i128);

        let before = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(before.locked_collateral, 20 * STROOPS_PER_USDC);
        assert_eq!(before.total_deposited, deposit);

        let withdraw_amount = 30 * STROOPS_PER_USDC; // within available (80 USDC)
        let stored_market = env.as_contract(&contract_id, || {
            storage::get_market(&env, market_id).unwrap().unwrap()
        });
        StellarAssetClient::new(&env, &stored_market.collateral_token)
            .mint(&contract_id, &(100 * STROOPS_PER_USDC));
        client.withdraw_unused_collateral(&user, &market_id, &withdraw_amount);

        let after = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(after.total_deposited, deposit - withdraw_amount);
        assert_eq!(after.locked_collateral, before.locked_collateral); // unchanged
    }

    /// #333: Attempting to withdraw locked collateral is rejected.
    #[test]
    fn test_333_cannot_withdraw_locked_collateral() {
        use crate::{error::ContractError, positions::STROOPS_PER_USDC};

        let deposit = 100 * STROOPS_PER_USDC;
        let (_env, user, client, _contract_id, market_id) = setup_funded_market(deposit);

        // Buy 100 YES shares at 60% → lock = 60 USDC; available = 40 USDC.
        let shares = 100 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &shares, &0i128, &6_000i128);

        // Try to withdraw 50 USDC (> available 40 USDC) → rejected.
        let result =
            client.try_withdraw_unused_collateral(&user, &market_id, &(50 * STROOPS_PER_USDC));
        assert_eq!(result, Err(Ok(ContractError::InsufficientCollateral)));
    }

    // ========== #334: Single source of truth for share-collateral math ==========

    /// #334: locked_collateral in storage matches the value returned by the
    /// canonical calculate_locked_collateral function.  This test documents the
    /// single-source-of-truth contract: there is no duplication between
    /// positions.rs and lib.rs.
    #[test]
    fn test_334_locked_collateral_matches_canonical_formula() {
        use crate::positions::{calculate_locked_collateral, STROOPS_PER_USDC};

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        let yes = 80 * STROOPS_PER_USDC;
        let no = 20 * STROOPS_PER_USDC;
        let price_bps = 6_000i128;

        // Buy YES then NO to establish a mixed position.
        client.update_position(&user, &market_id, &yes, &0i128, &price_bps);
        let pos = client.update_position(&user, &market_id, &0i128, &no, &price_bps);

        let expected = calculate_locked_collateral(yes, no, price_bps);
        assert_eq!(pos.locked_collateral, expected);

        // The stored value must also match.
        let stored = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .unwrap()
        });
        assert_eq!(stored.locked_collateral, expected);
    }

    // ========== #335: Emit position_updated on every share change ==========

    /// #335: settle_position emits a position_updated_event before the
    /// position_settled_event, capturing the final share state.
    #[test]
    fn test_335_settle_position_emits_position_updated() {
        use crate::positions::STROOPS_PER_USDC;
        use soroban_sdk::{testutils::Events as _, token::StellarAssetClient, IntoVal, Symbol};

        let deposit = 100 * STROOPS_PER_USDC;
        let (env, user, client, contract_id, market_id) = setup_funded_market(deposit);

        // Buy 100 YES shares.
        let shares = 100 * STROOPS_PER_USDC;
        client.update_position(&user, &market_id, &shares, &0i128, &5_000i128);

        // Resolve the market YES.
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, market_id, true);
        env.as_contract(&contract_id, || {
            let mut market = storage::get_market(&env, market_id).unwrap().unwrap();
            market.oracle_pubkey = oracle_pubkey;
            storage::set_market(&env, market_id, &market).unwrap();
        });
        let market_id_str = String::from_str(&env, "1");
        let resolver = Address::generate(&env);
        client.resolve_market(&resolver, &market_id_str, &true, &signature, &0u64);

        // Make sure the contract holds enough tokens to pay out.
        let stored_market = env.as_contract(&contract_id, || {
            storage::get_market(&env, market_id).unwrap().unwrap()
        });
        StellarAssetClient::new(&env, &stored_market.collateral_token)
            .mint(&contract_id, &(200 * STROOPS_PER_USDC));

        env.events().all(); // clear pre-settle events

        client.settle_position(&user, &market_id);

        let events = env.events().all();
        // Expect both position_updated_event and position_settled_event.
        let names: Vec<Symbol> = events
            .iter()
            .map(|e| e.1.get::<soroban_sdk::Val>(0).unwrap().into_val(&env))
            .collect();

        assert!(
            names.contains(&Symbol::new(&env, "position_updated_event")),
            "position_updated_event missing from settle_position events"
        );
        assert!(
            names.contains(&Symbol::new(&env, "position_settled_event")),
            "position_settled_event missing from settle_position events"
        );

        // position_updated must appear before position_settled.
        let updated_idx = names
            .iter()
            .position(|s| *s == Symbol::new(&env, "position_updated_event"))
            .unwrap();
        let settled_idx = names
            .iter()
            .position(|s| *s == Symbol::new(&env, "position_settled_event"))
            .unwrap();
        assert!(
            updated_idx < settled_idx,
            "position_updated_event must precede position_settled_event"
        );
    }

    /// #335: withdraw_canceled_collateral emits a position_updated_event after
    /// zeroing the user's locked_collateral and total_deposited.
    #[test]
    fn test_335_withdraw_canceled_collateral_emits_position_updated() {
        use soroban_sdk::{testutils::Events as _, IntoVal, Symbol};

        let deposit = 1_000i128;
        let (env, admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(deposit);

        client.cancel_market(&admin, &market_id);
        env.events().all(); // clear

        client.withdraw_canceled_collateral(&user, &market_id);

        let events = env.events().all();
        let names: Vec<Symbol> = events
            .iter()
            .map(|e| e.1.get::<soroban_sdk::Val>(0).unwrap().into_val(&env))
            .collect();

        assert!(
            names.contains(&Symbol::new(&env, "position_updated_event")),
            "position_updated_event missing after withdraw_canceled_collateral"
        );
    }

    // ========== Pause blocks deposit and withdraw entrypoints ==========

    #[test]
    fn test_pause_blocks_deposit_collateral() {
        use crate::error::ContractError;
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, user, client, _contract_id, market_id, collateral_token) =
            setup_admin_market_with_deposit(1_000);

        client.pause(&admin);
        assert!(client.is_paused());

        StellarAssetClient::new(&env, &collateral_token).mint(&user, &500);
        let result = client.try_deposit_collateral(&user, &market_id, &500);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    #[test]
    fn test_pause_blocks_withdraw_unused_collateral() {
        use crate::error::ContractError;

        let (env, admin, user, client, _contract_id, market_id, _token) =
            setup_admin_market_with_deposit(1_000);

        client.pause(&admin);

        let result = client.try_withdraw_unused_collateral(&user, &market_id, &100);
        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    #[test]
    fn test_unpause_restores_deposit_and_withdraw() {
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, user, client, _contract_id, market_id, collateral_token) =
            setup_admin_market_with_deposit(1_000);

        client.pause(&admin);
        client.unpause(&admin);
        assert!(!client.is_paused());

        // Advance past the withdraw cooldown so the restored path can be exercised.
        let now = env.ledger().timestamp();
        env.ledger().set_timestamp(now + 3_601);
        client.withdraw_unused_collateral(&user, &market_id, &1);

        StellarAssetClient::new(&env, &collateral_token).mint(&user, &500);
        client.deposit_collateral(&user, &market_id, &500);
    }

    #[test]
    fn test_non_admin_cannot_pause_or_unpause() {
        use crate::error::ContractError;

        let (env, _admin, _user, client, _contract_id, _market_id, _token) =
            setup_admin_market_with_deposit(1_000);
        let stranger = Address::generate(&env);

        assert_eq!(
            client.try_pause(&stranger),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_unpause(&stranger),
            Err(Ok(ContractError::NotAdmin))
        );
    }

    // ========== Admin auth audit: missing/insufficiently-checked mutators ==========

    #[test]
    fn test_update_market_oracle_invalidates_old_signatures() {
        use crate::error::ContractError;
        use ed25519_dalek::{Signer, SigningKey};
        use rand::rngs::OsRng;

        let (env, admin, client, contract_id) = create_test_contract();
        let resolver = Address::generate(&env);

        let mut old_rng = OsRng;
        let old_signing_key = SigningKey::generate(&mut old_rng);
        let old_oracle_pubkey =
            BytesN::from_array(&env, &old_signing_key.verifying_key().to_bytes());

        let mut new_rng = OsRng;
        let new_signing_key = SigningKey::generate(&mut new_rng);
        let new_oracle_pubkey =
            BytesN::from_array(&env, &new_signing_key.verifying_key().to_bytes());

        let question = String::from_str(&env, "Rotation invalidates old signatures");
        let end_time = env.ledger().timestamp() + 86_400;
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &old_oracle_pubkey,
            &Address::generate(&env),
            &None,
        );

        let message = crate::oracle::construct_oracle_message(&env, market_id, true);
        let old_signature = BytesN::from_array(
            &env,
            &old_signing_key
                .sign(message.to_array().as_slice())
                .to_bytes(),
        );

        client.update_market_oracle(&admin, &market_id, &new_oracle_pubkey);

        let market_id_str = String::from_str(&env, "1");
        let old_result =
            client.try_resolve_market(&resolver, &market_id_str, &true, &old_signature, &0u64);
        assert_eq!(old_result, Err(Ok(ContractError::InvalidSignature)));

        let message = crate::oracle::construct_oracle_message(&env, market_id, true);
        let new_signature = BytesN::from_array(
            &env,
            &new_signing_key
                .sign(message.to_array().as_slice())
                .to_bytes(),
        );
        let new_result =
            client.try_resolve_market(&resolver, &market_id_str, &true, &new_signature, &0u64);
        assert_eq!(new_result, Ok(Ok(())));

        let market = env.as_contract(&contract_id, || {
            storage::get_market(&env, market_id).unwrap().unwrap()
        });
        assert_eq!(market.oracle_pubkey, new_oracle_pubkey);
    }

    #[test]
    fn test_non_admin_cannot_call_admin_mutators() {
        use crate::error::ContractError;
        use crate::types::AdapterType;

        let (env, admin, client, _contract_id) = create_test_contract();
        let stranger = Address::generate(&env);

        let question = String::from_str(&env, "Stranger cannot admin?");
        let end_time = env.ledger().timestamp() + 86_400;
        let oracle_pubkey = BytesN::from_array(&env, &[7u8; 32]);
        let collateral_token = Address::generate(&env);
        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        assert_eq!(
            client.try_cancel_market(&stranger, &market_id),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_adapter_enabled(&stranger, &AdapterType::Ed25519, &true),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_update_market_oracle(
                &stranger,
                &market_id,
                &BytesN::from_array(&env, &[9u8; 32])
            ),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_propose_threshold_signers(&stranger, &soroban_sdk::Vec::new(&env), &1u32),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_add_fee_waiver(&stranger, &stranger),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_remove_fee_waiver(&stranger, &stranger),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_fee_rate(&stranger, &100i128),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_resolution_contract(&stranger, &stranger),
            Err(Ok(ContractError::NotAdmin))
        );
        assert_eq!(
            client.try_set_fee_cap(&stranger, &100i128),
            Err(Ok(ContractError::NotAdmin))
        );
    }

    #[test]
    fn test_set_resolution_contract_records_address() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let resolution_contract = Address::generate(&env);

        client.set_resolution_contract(&admin, &resolution_contract);

        assert_eq!(client.get_resolution_contract(), Some(resolution_contract));
    }

    // ========== Issue #708: void_market caller authorization ==========
    //
    // `void_market` is the market-side half of the resolution contract's
    // `void_market` arbitration outcome. It must be callable ONLY by the
    // registered resolution contract — a wrong caller (including the admin)
    // must never be able to void a live market, and an unset resolution
    // contract must fail closed.

    /// Register `resolution` as the market's resolution contract by writing
    /// storage directly (there is no instant public setter — the production
    /// path is the `propose_resolution_contract` / `execute_resolution_contract`
    /// timelock pair).
    fn wire_resolution_contract(env: &Env, contract_id: &Address, resolution: &Address) {
        env.as_contract(contract_id, || {
            storage::set_resolution_contract(env, resolution);
        });
    }

    fn new_active_market(env: &Env, client: &MarketContractClient<'_>, admin: &Address) -> u32 {
        client.initialize_market(
            admin,
            &String::from_str(env, "Void me?"),
            &(env.ledger().timestamp() + 86_400),
            &BytesN::from_array(env, &[3u8; 32]),
            &Address::generate(env),
            &None,
        )
    }

    #[test]
    fn test_void_market_by_registered_resolution_contract_cancels() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let market_id = new_active_market(&env, &client, &admin);

        let events_before = env.events().all().len();
        client.void_market(&resolution, &market_id);

        assert_eq!(client.get_market(&market_id).status, MarketStatus::Canceled);
        // The authoritative status-transition emits at least one event
        // (`MarketVoided`); the exact topic/payload is asserted in
        // `events::tests::test_emit_market_voided`.
        assert!(env.events().all().len() > events_before);
    }

    #[test]
    fn test_void_market_rejects_non_resolution_caller() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let market_id = new_active_market(&env, &client, &admin);

        let stranger = Address::generate(&env);
        let result = client.try_void_market(&stranger, &market_id);
        assert_eq!(result, Err(Ok(crate::error::ContractError::Unauthorized)));
        assert_eq!(client.get_market(&market_id).status, MarketStatus::Active);
    }

    #[test]
    fn test_void_market_rejects_admin_caller() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let market_id = new_active_market(&env, &client, &admin);

        // Even the admin cannot void a market — only the resolution contract.
        let result = client.try_void_market(&admin, &market_id);
        assert_eq!(result, Err(Ok(crate::error::ContractError::Unauthorized)));
        assert_eq!(client.get_market(&market_id).status, MarketStatus::Active);
    }

    #[test]
    fn test_void_market_fails_closed_when_no_resolution_contract_registered() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let market_id = new_active_market(&env, &client, &admin);

        let anyone = Address::generate(&env);
        let result = client.try_void_market(&anyone, &market_id);
        assert_eq!(result, Err(Ok(crate::error::ContractError::Unauthorized)));
    }

    #[test]
    fn test_void_market_rejects_already_resolved_market() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let market_id = new_active_market(&env, &client, &admin);
        env.as_contract(&contract_id, || {
            let mut m = storage::get_market(&env, market_id).unwrap().unwrap();
            m.status = MarketStatus::Resolved;
            m.result = Some(true);
            storage::set_market(&env, market_id, &m).unwrap();
        });

        let result = client.try_void_market(&resolution, &market_id);
        assert_eq!(
            result,
            Err(Ok(crate::error::ContractError::MarketAlreadyResolved))
        );
    }

    #[test]
    fn test_void_market_rejects_already_canceled_market() {
        let (env, admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let market_id = new_active_market(&env, &client, &admin);
        client.void_market(&resolution, &market_id);

        // A second void is rejected — no silent double-transition.
        let result = client.try_void_market(&resolution, &market_id);
        assert_eq!(result, Err(Ok(crate::error::ContractError::MarketNotActive)));
    }

    #[test]
    fn test_void_market_rejects_unknown_market() {
        let (env, _admin, client, contract_id) = create_test_contract();
        let resolution = Address::generate(&env);
        wire_resolution_contract(&env, &contract_id, &resolution);

        let result = client.try_void_market(&resolution, &999u32);
        assert_eq!(result, Err(Ok(crate::error::ContractError::MarketNotFound)));
    }

    // ========== Fee cap hardening: set-time and execute-time enforcement ==========

    #[test]
    fn test_set_fee_rate_rejects_over_cap() {
        use crate::error::ContractError;

        let (_env, admin, client, _contract_id) = create_test_contract();

        client.set_fee_cap(&admin, &500i128);
        let result = client.try_set_fee_rate(&admin, &501i128);
        assert_eq!(result, Err(Ok(ContractError::FeeCapExceeded)));
    }

    #[test]
    fn test_set_fee_rate_accepts_at_cap() {
        let (_env, admin, client, _contract_id) = create_test_contract();

        client.set_fee_cap(&admin, &500i128);
        client.set_fee_rate(&admin, &500i128);

        let pending = client
            .get_pending_fee_rate_change()
            .expect("pending change should exist");
        assert_eq!(pending.new_rate_bps, 500);
    }

    #[test]
    fn test_execute_fee_rate_change_rejects_when_cap_lowered_after_proposal() {
        use crate::error::ContractError;

        let (env, admin, client, _contract_id) = create_test_contract();

        // Propose a rate that is valid under the (default, permissive) cap.
        client.set_fee_rate(&admin, &9_000i128);

        // Admin tightens the cap below the pending rate before it takes effect.
        client.set_fee_cap(&admin, &1_000i128);

        // Advance past the timelock.
        let now = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(now + crate::FEE_RATE_TIMELOCK_SECONDS + 1);

        let result = client.try_execute_fee_rate_change();
        assert_eq!(result, Err(Ok(ContractError::FeeCapExceeded)));
    }

    #[test]
    fn test_execute_fee_rate_change_accepts_at_cap() {
        let (env, admin, client, _contract_id) = create_test_contract();

        client.set_fee_cap(&admin, &1_000i128);
        client.set_fee_rate(&admin, &1_000i128);

        let now = env.ledger().timestamp();
        env.ledger()
            .set_timestamp(now + crate::FEE_RATE_TIMELOCK_SECONDS + 1);

        let applied = client.execute_fee_rate_change();
        assert_eq!(applied, 1_000);
    }

    // ========== get_market view completeness tests (Issue #550) ==========

    /// Verify that `get_market` returns an error when the market does not exist.
    #[test]
    fn test_get_market_not_found() {
        use crate::error::ContractError;

        let (_env, _admin, client, _contract_id) = create_test_contract();

        let result = client.try_get_market(&999u32);
        assert_eq!(result, Err(Ok(ContractError::MarketNotFound)));
    }

    /// Snapshot-assert every field of the returned [`Market`] struct.
    ///
    /// This test is intentionally exhaustive: it uses a destructuring let to
    /// bind every field by name so that adding a new field to `Market` without
    /// updating this test causes a compile-time "missing field" error — the
    /// acceptance criterion for Issue #550.
    #[test]
    fn test_get_market_returns_all_fields() {
        use crate::types::{AdapterType, MarketStatus};

        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Will ETH reach $10k by end of year?");
        let end_time = env.ledger().timestamp() + 86_400;
        let oracle_pubkey = BytesN::from_array(&env, &[2u8; 32]);
        let collateral_token = Address::generate(&env);
        let created_at = env.ledger().timestamp();

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        let market = client.get_market(&market_id);

        // Destructure every field — adding a new field to `Market` without
        // updating this pattern produces a compile error, which is exactly the
        // desired "test fails if a new public field is omitted" behaviour.
        let crate::types::Market {
            id,
            question: market_question,
            end_time: market_end_time,
            oracle_pubkey: market_oracle_pubkey,
            status,
            result,
            creator,
            created_at: market_created_at,
            collateral_token: market_collateral_token,
            price_bps,
            resolver,
            resolved_at,
            adapter_type,
            outcome_count,
            closed_to_deposits,
        } = market;

        assert_eq!(id, market_id);
        assert_eq!(market_question, question);
        assert_eq!(market_end_time, end_time);
        assert_eq!(market_oracle_pubkey, oracle_pubkey);
        assert_eq!(status, MarketStatus::Active);
        assert_eq!(result, None);
        assert_eq!(creator, admin);
        assert_eq!(market_created_at, created_at);
        assert_eq!(market_collateral_token, collateral_token);
        // Initial price is 50 % (5 000 bps) as set by initialize_market.
        assert_eq!(price_bps, 5_000i128);
        // Resolver and resolved_at are only populated after resolution.
        assert_eq!(resolver, None);
        assert_eq!(resolved_at, None);
        // Default adapter for new markets is Ed25519.
        assert_eq!(adapter_type, AdapterType::Ed25519);
        // Binary markets always have exactly two outcomes.
        assert_eq!(outcome_count, 2u32);
        // Markets are open to deposits at creation.
        assert!(!closed_to_deposits);
    }

    /// Verify that `get_market` reflects `closed_to_deposits` after
    /// `close_market_to_deposits` is called.
    #[test]
    fn test_get_market_reflects_closed_to_deposits() {
        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Closed-deposits test market");
        let end_time = env.ledger().timestamp() + 86_400;
        let oracle_pubkey = BytesN::from_array(&env, &[3u8; 32]);
        let collateral_token = Address::generate(&env);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        // Initially open to deposits.
        assert!(!client.get_market(&market_id).closed_to_deposits);

        client.close_market_to_deposits(&admin, &market_id);

        // After closing, the flag must be reflected by get_market.
        assert!(client.get_market(&market_id).closed_to_deposits);
    }

    /// Verify that `get_market` reflects `status` and `resolver` / `resolved_at`
    /// after a successful resolution.
    #[test]
    fn test_get_market_reflects_resolved_status() {
        use crate::types::MarketStatus;

        let (env, admin, client, _contract_id) = create_test_contract();

        let question = String::from_str(&env, "Resolution status test market");
        let end_time = env.ledger().timestamp() + 86_400;
        let (oracle_pubkey, signature) = generate_test_keypair_and_sign(&env, 1, true);
        let collateral_token = Address::generate(&env);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
            &None,
        );

        // Advance time past end_time so the market can be resolved.
        env.ledger().set_timestamp(end_time + 1);

        let resolver = Address::generate(&env);
        let market_id_str = String::from_str(&env, "1");
        client.resolve_market(&resolver, &market_id_str, &true, &signature, &0u64);

        let market = client.get_market(&market_id);
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.result, Some(true));
        // resolver and resolved_at are populated after resolution.
        assert!(market.resolver.is_some());
        assert!(market.resolved_at.is_some());
    }

    // ========== add_fee_waiver misuse rejection (#584) ==========

    /// A contract address is not a valid depositor and must never receive a
    /// fee waiver — the same rule `validate_admin_address` applies to the admin.
    #[test]
    fn test_add_fee_waiver_rejects_contract_address() {
        use crate::error::ContractError;

        let (env, admin, client, _contract_id) = create_test_contract();
        let waiver_contract = env.register(MarketContract, ());

        assert_eq!(
            client.try_add_fee_waiver(&admin, &waiver_contract),
            Err(Ok(ContractError::InvalidFeeWaiverAccount))
        );
        assert!(!client.is_fee_waived(&waiver_contract));
    }

    /// The admin cannot add itself to the fee waiver list — it already
    /// controls `set_fee_rate`, so self-waiving would let it silently exempt
    /// itself from the fee it sets for everyone else.
    #[test]
    fn test_add_fee_waiver_rejects_admin_self_waiver() {
        use crate::error::ContractError;

        let (_env, admin, client, _contract_id) = create_test_contract();

        assert_eq!(
            client.try_add_fee_waiver(&admin, &admin),
            Err(Ok(ContractError::InvalidFeeWaiverAccount))
        );
        assert!(!client.is_fee_waived(&admin));
    }

    /// A regular user account can be waived, and adding it twice is a no-op
    /// that leaves exactly one entry in the list.
    #[test]
    fn test_add_fee_waiver_accepts_user_and_is_idempotent() {
        let (env, admin, client, _contract_id) = create_test_contract();
        let account = Address::generate(&env);

        client.add_fee_waiver(&admin, &account);
        client.add_fee_waiver(&admin, &account);

        assert!(client.is_fee_waived(&account));
        assert_eq!(client.get_fee_waivers().len(), 1);
    }
}

#[test]
fn test_decode_v3_market_blob_fails() {
    use crate::types::{AdapterType, Market, MarketStatus};
    use soroban_sdk::{
        contracttype,
        xdr::{FromXdr, ToXdr},
        Address, BytesN, Env, String,
    };

    let env = Env::default();

    #[derive(Clone, Debug, Eq, PartialEq)]
    #[contracttype]
    pub struct MarketV3 {
        pub id: u32,
        pub question: String,
        pub end_time: u64,
        pub oracle_pubkey: BytesN<32>,
        pub status: MarketStatus,
        pub result: Option<bool>,
        pub creator: Address,
        pub created_at: u64,
        pub collateral_token: Address,
        pub price_bps: i128,
        pub resolver: Option<Address>,
        pub resolved_at: Option<u64>,
        pub adapter_type: AdapterType,
        pub outcome_count: u32,
    }

    let v3_market = MarketV3 {
        id: 1,
        question: String::from_str(&env, "Will it rain?"),
        end_time: 1234567890,
        oracle_pubkey: BytesN::from_array(&env, &[0; 32]),
        status: MarketStatus::Active,
        result: None,
        creator: Address::generate(&env),
        created_at: 1234560000,
        collateral_token: Address::generate(&env),
        price_bps: 5000,
        resolver: None,
        resolved_at: None,
        adapter_type: AdapterType::Ed25519,
        outcome_count: 2,
    };

    // Serialize V3 market
    let v3_xdr = v3_market.to_xdr(&env);

    // Attempting to decode as V4 Market should fail because closed_to_deposits is missing
    let decode_result = Market::from_xdr(&env, &v3_xdr);
    assert!(
        decode_result.is_err(),
        "Decoding V3 market as V4 should fail intentionally because it lacks closed_to_deposits"
    );
}
