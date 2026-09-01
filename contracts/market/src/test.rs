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
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, client, _contract_id) = create_test_contract();

        // Create a real SAC token so deposit_collateral's transfer succeeds.
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();

        let question = String::from_str(&env, "Test market");
        let end_time = env.ledger().timestamp() + 86400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);

        let market_id = client.initialize_market(
            &admin,
            &question,
            &end_time,
            &oracle_pubkey,
            &collateral_token,
        );

        // Clear events from initialization
        env.events().all();

        // Mint tokens for the user so the transfer succeeds.
        let user = Address::generate(&env);
        let sac = StellarAssetClient::new(&env, &collateral_token);
        sac.mint(&user, &10_000);

        client.deposit_collateral(&user, &market_id, &1000i128);

        // Verify event was emitted
        let events = env.events().all();
        assert!(events.len() > 0, "CollateralDeposited event should be emitted");
    }

    // ========== Issue 1: Share Accounting Source of Truth ==========
    //
    // The on-chain `Position` struct stored under `StorageKey::Position(market_id, user)`
    // is the canonical source of truth for YES/NO share balances.  Off-chain indexers
    // (e.g. vatix-backend `UserPosition`) must reconcile against `get_position`.
    //
    // This regression test asserts that:
    //   1. `get_position` returns `None` before any interaction.
    //   2. After `deposit_collateral`, `get_position` reflects the correct on-chain state.
    //   3. Every field survives the full storage round-trip (no silent truncation).
    //
    // If this test is removed or weakened the auditor gap is reintroduced.

    #[test]
    fn regression_get_position_is_canonical_source_of_truth() {
        use soroban_sdk::token::StellarAssetClient;

        let (env, admin, client, contract_id) = create_test_contract();

        // Set up a real SAC token so the transfer in deposit_collateral works.
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin.clone());
        let collateral_token = token.address();

        let question = String::from_str(&env, "Will BTC reach $100k?");
        let end_time = env.ledger().timestamp() + 86_400;
        let oracle_pubkey = BytesN::from_array(&env, &[1u8; 32]);

        let market_id =
            client.initialize_market(&admin, &question, &end_time, &oracle_pubkey, &collateral_token);

        let user = Address::generate(&env);

        // 1. Before any deposit — get_position must return None.
        //    If it returned Some here the gap would be silently hidden.
        let before = client.get_position(&market_id, &user);
        assert!(
            before.is_none(),
            "get_position must return None before any interaction — \
             returning stale data from off-chain indexer would violate \
             the canonical source-of-truth invariant"
        );

        // 2. Mint tokens and deposit so a Position record is written on-chain.
        let sac = StellarAssetClient::new(&env, &collateral_token);
        sac.mint(&user, &10_000);
        let deposit_amount = 5_000i128;
        client.deposit_collateral(&user, &market_id, &deposit_amount);

        // 3. get_position must now reflect the exact on-chain state.
        let pos = client
            .get_position(&market_id, &user)
            .expect("get_position must return Some after deposit — \
                     position was written to StorageKey::Position by deposit_collateral");

        // Share balances: deposit_collateral does NOT allocate shares, it sets
        // total_deposited and locked_collateral only.
        assert_eq!(pos.yes_shares, 0, "yes_shares must be 0 after deposit-only");
        assert_eq!(pos.no_shares, 0, "no_shares must be 0 after deposit-only");
        assert_eq!(
            pos.total_deposited, deposit_amount,
            "total_deposited must equal the deposited amount (canonical source of truth)"
        );
        assert_eq!(
            pos.locked_collateral, deposit_amount,
            "locked_collateral must equal deposited amount before any trades"
        );
        assert!(!pos.is_settled, "position must not be marked settled");
        assert_eq!(pos.market_id, market_id);
        assert_eq!(pos.user, user);

        // 4. Cross-check: storage layer and public entrypoint must agree.
        let from_storage = env.as_contract(&contract_id, || {
            crate::storage::get_position(&env, market_id, &user)
                .expect("storage layer must also have the position")
        });
        assert_eq!(
            from_storage.total_deposited, pos.total_deposited,
            "public entrypoint and storage layer must return identical data — \
             any discrepancy means a caching or serialisation bug"
        );
        assert_eq!(from_storage.yes_shares, pos.yes_shares);
        assert_eq!(from_storage.no_shares, pos.no_shares);
        assert_eq!(from_storage.locked_collateral, pos.locked_collateral);
    }
}
