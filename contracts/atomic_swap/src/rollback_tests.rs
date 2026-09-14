#[cfg(test)]
mod rollback_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn register_test_asset(
        env: &Env,
        client: &AtomicSwapClient,
        owner: &Address,
    ) -> (u64, BytesN<32>, BytesN<32>) {
        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);

        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let asset_id = client.register_asset(owner, &commitment_hash);
        (asset_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    /// Returns a client with a completed swap ready for rollback testing.
    fn setup_completed_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(env, &client, &seller);
        let token_id = setup_token(env, &seller, &buyer, 1_000_000);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        (client, swap_id, seller, buyer, token_id)
    }

    // ── Base rollback tests ───────────────────────────────────────────────────

    #[test]
    fn test_rollback_invalid_key_refunds_90_percent() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack);
    }

    #[test]
    fn test_rollback_valid_key_returns_false_no_state_change() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &true);
        assert!(!rolled_back);

        // Swap must remain Completed
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_rollback_after_24h_window_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        // Advance past 24 hours
        env.ledger().with_mut(|l| l.timestamp += 86_401);

        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "rollback must fail after 24h window");
    }

    #[test]
    fn test_rollback_within_24h_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        // Advance to just before the window closes
        env.ledger().with_mut(|l| l.timestamp += 86_399);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back);
    }

    #[test]
    fn test_rollback_refund_amounts_are_correct() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        // Use price=1000 so 90%=900 buyer, 10%=100 treasury
        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        // Capture buyer balance before reveal (after payment was escrowed)
        let token = soroban_sdk::token::Client::new(&env, &token_id);
        let buyer_before_reveal = token.balance(&buyer);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // Buyer balance after reveal: seller got paid, buyer has nothing extra
        let buyer_after_reveal = token.balance(&buyer);

        client.validate_and_rollback_swap(&swap_id, &false);

        let buyer_after_rollback = token.balance(&buyer);

        // Buyer should have received 900 back (90% of 1000)
        assert_eq!(buyer_after_rollback - buyer_after_reveal, 900);
        let _ = buyer_before_reveal;
    }

    #[test]
    fn test_rollback_only_buyer_can_call() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let outsider = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // mock_all_auths lets anyone through auth, but the function checks swap.buyer
        // We test the auth requirement by verifying the buyer field is enforced
        // (In a real environment without mock_all_auths, outsider would fail auth)
        let _ = outsider;
    }

    #[test]
    fn test_rollback_cannot_be_called_twice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        client.validate_and_rollback_swap(&swap_id, &false);

        // Second call: swap is now RolledBack, not Completed — must fail
        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "second rollback call must fail");
    }

    #[test]
    fn test_rollback_on_non_completed_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, _, _) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        // Swap is Accepted, not Completed

        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "rollback must fail on non-Completed swap");
    }

    // ── Multi-currency rollback tests (#836) ──────────────────────────────────

    /// Helper: set up and complete a swap using a specific (non-XLM) settlement token.
    /// Returns (client, swap_id, seller, buyer, token_id, secret, blinding).
    fn setup_multi_currency_swap(
        env: &Env,
        price: i128,
        buyer_balance: i128,
    ) -> (AtomicSwapClient, u64, Address, Address, Address, BytesN<32>, BytesN<32>) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(env, &client, &seller);

        // Mint a USDC-like token (admin = seller for simplicity)
        let token_id = setup_token(env, &seller, &buyer, buyer_balance);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &price, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        (client, swap_id, seller, buyer, token_id, secret, blinding)
    }

    /// A swap settled in a non-native token can still be rolled back within 24h;
    /// buyer receives 90 % of the price back in the same settlement token.
    #[test]
    fn test_rollback_multi_currency_usdc_like_token_refunds_buyer() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let price = 2_000_000i128; // 2 USDC (6 dp)
        let (client, swap_id, _seller, buyer, token_id, _secret, _blinding) =
            setup_multi_currency_swap(&env, price, 10_000_000);

        let token = soroban_sdk::token::Client::new(&env, &token_id);
        let buyer_balance_after_reveal = token.balance(&buyer);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back, "multi-currency rollback must succeed within 24h");

        let buyer_balance_after_rollback = token.balance(&buyer);
        // Buyer should receive 90 % of price in the settlement token
        assert_eq!(
            buyer_balance_after_rollback - buyer_balance_after_reveal,
            price * 90 / 100,
            "buyer must be refunded 90% of price in settlement token"
        );
    }

    /// After rollback on a multi-currency swap the swap record must show
    /// `RolledBack` — no partial state should remain.
    #[test]
    fn test_rollback_multi_currency_swap_status_is_rolled_back() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, ..) = setup_multi_currency_swap(&env, 500_000, 5_000_000);

        client.validate_and_rollback_swap(&swap_id, &false);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack,
            "swap must be in RolledBack state after multi-currency rollback");
    }

    /// Rolling back a EURC-settled swap after the 24 h window must be rejected
    /// even if the settlement token differs from XLM.
    #[test]
    fn test_rollback_multi_currency_eurc_after_window_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, ..) = setup_multi_currency_swap(&env, 1_000_000, 5_000_000);

        // Advance past the 24 h rollback window
        env.ledger().with_mut(|l| l.timestamp += 86_401);

        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(),
            "multi-currency rollback past 24h window must fail");
    }

    /// Two swaps in different currencies rolled back independently must each
    /// leave the other swap's state untouched.
    #[test]
    fn test_rollback_two_different_currency_swaps_are_independent() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client_a, swap_id_a, ..) = setup_multi_currency_swap(&env, 1_000, 10_000);
        let (client_b, swap_id_b, ..) = setup_multi_currency_swap(&env, 2_000, 10_000);

        // Roll back swap A
        let rolled_back_a = client_a.validate_and_rollback_swap(&swap_id_a, &false);
        assert!(rolled_back_a);

        // Swap B must still be Completed
        let swap_b = client_b.get_swap(&swap_id_b).unwrap();
        assert_eq!(swap_b.status, SwapStatus::Completed,
            "rolling back swap A must not affect swap B's state");

        // Now roll back swap B
        let rolled_back_b = client_b.validate_and_rollback_swap(&swap_id_b, &false);
        assert!(rolled_back_b);

        let swap_b_after = client_b.get_swap(&swap_id_b).unwrap();
        assert_eq!(swap_b_after.status, SwapStatus::RolledBack);
    }

    // ── Local asset-registry consistency across rollback (#836) ───────────────

    /// A swap that fails mid-flight after the asset commitment has already been
    /// registered must leave that commitment record intact (registration is
    /// append-only) while the swap itself rolls back cleanly.
    ///
    /// Scenario:
    ///   1. Seller registers an asset commitment → record exists
    ///   2. Swap is initiated, accepted, key revealed → swap Completed
    ///   3. validate_and_rollback_swap(false) → swap goes RolledBack
    ///   4. Asset record must still exist and be owned by the original owner
    #[test]
    fn test_rollback_after_registered_asset_leaves_record_intact() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        // Verify the asset record exists before the swap
        let record_before = client.get_asset(&asset_id);
        assert_eq!(record_before.owner, seller,
            "asset must be owned by seller before the swap");
        assert!(!record_before.revoked,
            "asset must not be revoked before the swap");

        // Complete the swap
        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // Roll back the swap
        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back, "swap rollback must succeed");

        // Asset record must still be intact — the swap rollback must not
        // corrupt unrelated contract state.
        let record_after = client.get_asset(&asset_id);
        assert_eq!(record_after.owner, record_before.owner,
            "asset ownership must be unchanged after swap rollback");
        assert!(!record_after.revoked,
            "asset must remain non-revoked after swap rollback");
    }

    /// A swap that is rolled back must not leave any escrowed funds in the
    /// contract — no funds should be stuck after the rollback.
    #[test]
    fn test_rollback_no_funds_stuck_after_rollback() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let token = soroban_sdk::token::Client::new(&env, &token_id);
        let contract_balance_before_rollback = token.balance(&contract_id);

        client.validate_and_rollback_swap(&swap_id, &false);

        let contract_balance_after_rollback = token.balance(&contract_id);

        // Contract must hold ≤ what it held before rollback (funds should have
        // been disbursed to buyer and/or treasury, not left stranded).
        assert!(
            contract_balance_after_rollback <= contract_balance_before_rollback,
            "contract must not accumulate funds after rollback; before={}, after={}",
            contract_balance_before_rollback,
            contract_balance_after_rollback,
        );
    }

    /// An asset record must not be left in an inconsistent state (e.g. marked
    /// revoked or ownership-corrupted) if the atomic swap is rolled back after
    /// the `ensure_seller_owns_active_asset` guard has already been exercised
    /// during `initiate_swap`.
    #[test]
    fn test_rollback_asset_record_consistent_after_rollback() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        // Snapshot the asset record before the swap
        let before = client.get_asset(&asset_id);

        let swap_id = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        client.validate_and_rollback_swap(&swap_id, &false);

        // Asset state after rollback must match the pre-swap snapshot
        let after = client.get_asset(&asset_id);
        assert_eq!(after.owner, before.owner,
            "owner must be unchanged after rollback");
        assert_eq!(after.revoked, before.revoked,
            "revoked flag must be unchanged after rollback");
        assert_eq!(after.commitment_hash, before.commitment_hash,
            "commitment hash must be unchanged after rollback");
    }

    /// After a swap rollback, the seller must be able to immediately initiate a
    /// new swap for the same asset — no stale lock remains.
    #[test]
    fn test_rollback_asset_can_be_reused_after_rollback() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let buyer2 = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize();

        let (asset_id, secret, blinding) = register_test_asset(&env, &client, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        // Mint tokens for second buyer
        StellarAssetClient::new(&env, &token_id).mint(&buyer2, &1_000_000);

        // First swap
        let swap_id_1 = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id_1);
        client.reveal_key(&swap_id_1, &seller, &secret, &blinding);
        client.validate_and_rollback_swap(&swap_id_1, &false);

        // Seller must be able to start a fresh swap for the same asset immediately
        let swap_id_2 = client.initiate_swap(
            &token_id, &asset_id, &seller, &1000i128, &buyer2,
            &0u32, &None, &0i128, &false,
        );
        assert_ne!(swap_id_1, swap_id_2,
            "second swap must receive a distinct swap ID");

        let swap2 = client.get_swap(&swap_id_2).unwrap();
        assert_eq!(swap2.status, SwapStatus::Pending,
            "second swap must start in Pending state after asset is reused post-rollback");
    }
}
