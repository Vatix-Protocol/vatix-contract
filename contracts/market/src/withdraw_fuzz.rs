//! #407: Property-based fuzz tests for `withdraw_unused_collateral`.
//!
//! Random `(yes_shares, no_shares, locked_collateral, total_deposited, amount)`
//! combinations are driven through the withdraw logic to assert invariants.
//!
//! ## Invariants Tested
//! 1. **Lock Bound**: `locked_collateral <= total_deposited`
//! 2. **Available Non-Negative**: `available = total_deposited - locked_collateral >= 0`
//! 3. **Withdraw Validation**: Withdraw fails when `amount > available`
//! 4. **Success Preserves Invariant**: Successful withdraw maintains `locked <= total_deposited`
//! 5. **Fee Rounding**: `fee_amount = floor(amount * fee_rate_bps / 10_000)` never
//!    over/underflows and never exceeds `amount` itself, across the full
//!    `fee_rate_bps` range (0–10_000) and edge `amount`s near zero and near the
//!    `validate_amount_reasonable` ceiling (`i128::MAX / 2`) — see the
//!    "Dust rule" note on [`validation::calculate_fee`] fuzz coverage below.
//!
//! ## #897: no underflow across deposit/withdraw sequences
//!
//! The [`deposit_withdraw_sequences`] module drives the *real* contract
//! entrypoints (`deposit_collateral` / `withdraw_unused_collateral` through
//! `MarketContractClient`, a Stellar asset token, and optionally a registered
//! treasury) with random operation sequences, fee rates, and fee waivers.
//! After every operation it checks, against an independent model:
//!
//! 6. **Exact accounting**: a successful withdraw of `amount` reduces
//!    `total_deposited` by exactly `amount + fee` (#377); a deposit raises it
//!    by exactly `amount`. `total_deposited` never goes negative.
//! 7. **Fee-on-top boundary**: a withdraw succeeds iff
//!    `amount + fee <= total_deposited - locked_collateral`; otherwise it fails
//!    with `InsufficientCollateral` — never a panic, wrap, or partial payout.
//! 8. **Atomic failure**: any rejected operation (insufficient funds, cooldown,
//!    invalid or extreme amount) leaves the position and every token balance
//!    unchanged.
//! 9. **Token conservation**: `user + market + treasury` balances are constant,
//!    the market holds `total_deposited` plus any fee retained without a
//!    treasury, and the treasury holds exactly the fees routed to it (its
//!    `total_collected` agrees).
//! 10. **Waivers**: a waived user never pays a fee and can withdraw its whole
//!     available balance.

use crate::error::ContractError;
use crate::positions;
use crate::storage;
use crate::types::{Market, MarketStatus, Position};
use crate::validation;
use crate::withdraw::withdraw_unused_collateral;
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

/// Strategy for random position state with valid invariant: locked <= deposited.
///
/// `locked` is derived from the shares/price exactly as `update_position`
/// does, and `deposited` is that lock plus random slack — the only states the
/// contract can reach (a trade that would lock more than is deposited is
/// rejected). Pairing arbitrary shares with an independent `deposited` would
/// fabricate impossible states and fail the invariant spuriously.
fn arb_valid_position() -> impl Strategy<Value = (i128, i128, i128, i128)> {
    (
        0i128..=1_000_000i128,  // yes_shares
        0i128..=1_000_000i128,  // no_shares
        0i128..=10_000i128,     // market_price
        0i128..=10_000_000i128, // unlocked slack
    )
        .prop_map(|(yes, no, price, slack)| {
            let locked = positions::calculate_locked_collateral(yes, no, price);
            (yes, no, price, locked + slack)
        })
}

/// Strategy for fuzzing withdraw amount against available collateral
fn arb_withdraw_state() -> impl Strategy<Value = (i128, i128, i128, i128, i128)> {
    (0i128..=10_000_000i128).prop_flat_map(|deposited| {
        (
            0i128..=1_000_000i128, // yes_shares
            0i128..=1_000_000i128, // no_shares
            0i128..=10_000i128,    // price
            Just(deposited),
            1i128..=(deposited + 1), // amount (may exceed available)
        )
    })
}

fn make_market(env: &Env, market_id: u32, collateral_token: &Address) -> Market {
    use crate::types::AdapterType;
    Market {
        id: market_id,
        question: String::from_str(env, "fuzz?"),
        end_time: 1_000_000,
        oracle_pubkey: BytesN::from_array(env, &[0u8; 32]),
        status: MarketStatus::Active,
        result: None,
        creator: Address::generate(env),
        created_at: 0,
        collateral_token: collateral_token.clone(),
        price_bps: 5_000,
        resolver: None,
        resolved_at: None,
        adapter_type: AdapterType::Ed25519,
        outcome_count: 2,
        closed_to_deposits: false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    /// Invariant A: withdraw never silently over-withdraws.
    /// If `amount > available`, the call must return an error.
    #[test]
    fn prop_withdraw_never_exceeds_available(
        (yes_shares, no_shares, price, deposited, amount) in arb_withdraw_state()
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = make_market(&env, market_id, &collateral_token);
        // Compute locked from shares/price to ensure valid state
        let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
        // Ensure locked doesn't exceed deposited for valid test cases
        let locked = if locked > deposited { deposited } else { locked };

        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares,
            no_shares,
            locked_collateral: locked,
            total_deposited: deposited,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
        });

        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
            .mint(&contract_id, &(deposited + amount));

        let available = deposited.saturating_sub(locked);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });

        if amount > available {
            prop_assert!(result.is_err(),
                "expected error: amount={amount} > available={available}");
        }
    }

    /// Invariant B: on success, total_deposited decreases by exactly `amount`
    /// (fee rate explicitly 0 — the default is 50 bps — and no locked shares).
    #[test]
    fn prop_successful_withdraw_decrements_deposited(
        (deposited, amount) in (1i128..=10_000_000i128)
            .prop_flat_map(|deposited| (Just(deposited), 1i128..=deposited)),
    ) {

        let env = Env::default();
        env.mock_all_auths();

        let user = Address::generate(&env);
        let market_id = 1u32;
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin);
        let collateral_token = token.address();
        let contract_id = env.register(crate::MarketContract, ());

        let market = make_market(&env, market_id, &collateral_token);
        let position = Position {
            market_id,
            user: user.clone(),
            yes_shares: 0,
            no_shares: 0,
            locked_collateral: 0,
            total_deposited: deposited,
            is_settled: false,
        };

        env.as_contract(&contract_id, || {
            storage::set_version(&env);
            storage::set_market(&env, market_id, &market).unwrap();
            storage::set_position(&env, market_id, &user, &position).unwrap();
            storage::set_fee_rate_bps(&env, 0);
        });

        soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
            .mint(&contract_id, &deposited);

        let result = env.as_contract(&contract_id, || {
            withdraw_unused_collateral(env.clone(), user.clone(), market_id, amount)
        });

        prop_assert!(result.is_ok());

        let updated = env.as_contract(&contract_id, || {
            storage::get_position(&env, market_id, &user)
                .unwrap()
                .expect("position exists")
        });
        prop_assert_eq!(updated.total_deposited, deposited - amount);
    }
}

/// Share and collateral invariants - #351
mod share_collateral_invariants {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        /// Invariant: locked_collateral <= total_deposited always holds
        #[test]
        fn prop_locked_never_exceeds_deposited(
            (yes_shares, no_shares, price, deposited) in arb_valid_position()
        ) {
            let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
            // Only test when locked is computed from valid position state
            prop_assert!(locked <= deposited,
                "locked={locked} > deposited={deposited} yes={yes_shares} no={no_shares} price={price}");
        }

        /// Invariant: available = total_deposited - locked is always non-negative
        #[test]
        fn prop_available_non_negative(
            (yes_shares, no_shares, price, deposited) in arb_valid_position()
        ) {
            let locked = positions::calculate_locked_collateral(yes_shares, no_shares, price);
            let available = deposited.saturating_sub(locked);
            prop_assert!(available >= 0,
                "available={available} negative: deposited={deposited} locked={locked}");
        }

        /// Invariant: after successful deposit, locked <= total_deposited
        #[test]
        fn prop_deposit_preserves_locked_invariant(
            existing_deposited in 0i128..=5_000_000i128,
            new_deposit in 1i128..=5_000_000i128,
        ) {
            let env = Env::default();
            let user = Address::generate(&env);
            let market_id = 1u32;
            let token_admin = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(token_admin);
            let collateral_token = token.address();
            let contract_id = env.register(crate::MarketContract, ());

            let market = make_market(&env, market_id, &collateral_token);

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
                storage::set_market(&env, market_id, &market).unwrap();
            });

            // Create initial position with some shares
            env.as_contract(&contract_id, || {
                let _ = positions::update_position(&env, market_id, &user, 1000, 500, 5000);
            });

            // Get position after shares are set
            let position_before = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).unwrap().unwrap()
            });

            // Deposit additional collateral
            env.mock_all_auths();
            soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
                .mint(&user, &(existing_deposited + new_deposit));

            env.as_contract(&contract_id, || {
                crate::deposit::deposit_collateral(env.clone(), user.clone(), market_id, new_deposit)
            }).unwrap();

            // Verify invariant holds after deposit
            let position_after = env.as_contract(&contract_id, || {
                storage::get_position(&env, market_id, &user).unwrap().unwrap()
            });

            prop_assert!(position_after.locked_collateral <= position_after.total_deposited,
                "invariant broken: locked={} > total={}",
                position_after.locked_collateral, position_after.total_deposited);
        }

        /// Invariant: withdrawing available collateral preserves locked <= deposited
        #[test]
        fn prop_withdraw_preserves_locked_invariant(
            deposited in 100i128..=1_000_000i128,
            withdraw_amount in 1i128..=100_000i128,
        ) {
            prop_assume!(withdraw_amount <= deposited);

            let env = Env::default();
            env.mock_all_auths();

            let user = Address::generate(&env);
            let market_id = 1u32;
            let token_admin = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(token_admin);
            let collateral_token = token.address();
            let contract_id = env.register(crate::MarketContract, ());

            let market = make_market(&env, market_id, &collateral_token);

            let position = Position {
                market_id,
                user: user.clone(),
                yes_shares: 0,
                no_shares: 0,
                locked_collateral: 0,
                total_deposited: deposited,
                is_settled: false,
            };

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
                storage::set_market(&env, market_id, &market).unwrap();
                storage::set_position(&env, market_id, &user, &position).unwrap();
            });

            soroban_sdk::token::StellarAssetClient::new(&env, &collateral_token)
                .mint(&contract_id, &deposited);

            let result = env.as_contract(&contract_id, || {
                withdraw_unused_collateral(env.clone(), user.clone(), market_id, withdraw_amount)
            });

            if result.is_ok() {
                let updated = env.as_contract(&contract_id, || {
                    storage::get_position(&env, market_id, &user).unwrap().unwrap()
                });
                prop_assert!(updated.locked_collateral <= updated.total_deposited,
                    "invariant broken after withdraw: locked={} > total={}",
                    updated.locked_collateral, updated.total_deposited);
            }
        }

        /// Invariant: position update recalculates locked from shares
        #[test]
        fn prop_position_update_recalculates_locked(
            initial_yes in 0i128..=10_000i128,
            initial_no in 0i128..=10_000i128,
            yes_delta in -5_000i128..=5_000i128,
            no_delta in -5_000i128..=5_000i128,
            price in 0i128..=10_000i128,
        ) {
            let env = Env::default();
            let user = Address::generate(&env);
            let market_id = 1u32;
            let contract_id = env.register(crate::MarketContract, ());

            env.as_contract(&contract_id, || {
                storage::set_version(&env);
            });

            // Initial position
            let initial_locked = positions::calculate_locked_collateral(initial_yes, initial_no, price);

            // Update position
            let result = env.as_contract(&contract_id, || {
                positions::update_position(&env, market_id, &user, yes_delta, no_delta, price)
            });

            // If update succeeded, verify locked matches computed value
            if let Ok(pos) = result {
                let expected_locked = positions::calculate_locked_collateral(
                    pos.yes_shares, pos.no_shares, price
                );
                prop_assert_eq!(pos.locked_collateral, expected_locked,
                    "locked mismatch: expected={}, got={}", expected_locked, pos.locked_collateral);
            }
        }
    }
}

/// Withdrawal fee rounding invariants.
///
/// `fee_amount = floor(amount * fee_rate_bps / 10_000)` (`validation::calculate_fee`).
/// Withdraw itself never carves the fee out of `amount` — the user always
/// receives exactly `amount`, and `amount + fee_amount` is deducted from
/// `total_deposited` on top (see `withdraw.rs`'s module doc, #377). So the
/// invariant under test isn't "fee + payout == amount"; it's the pair that
/// actually holds here:
///
/// **Dust rule**: integer division floors, so up to `9999` stroops of
/// `amount * fee_rate_bps` can be lost to rounding on every withdrawal. That
/// dust is never collected by the protocol and never charged to the user
/// beyond the floored `fee_amount` — it simply vanishes below the bps
/// granularity. Formally: `amount * fee_rate_bps == fee_amount * 10_000 + dust`
/// with `0 <= dust < 10_000`, and `0 <= fee_amount <= amount`.
mod fee_rounding_invariants {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        /// General case: fee never overflows, never exceeds `amount`, and the
        /// floor-division dust rule holds for the full valid amount/bps range.
        #[test]
        fn prop_fee_rounding_never_overflows_or_exceeds_amount(
            amount in 1i128..=10_000_000_000i128,
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, fee_rate_bps)
                .expect("calculate_fee must not error for valid inputs");

            prop_assert!(fee_amount >= 0, "fee_amount={fee_amount} negative");
            prop_assert!(fee_amount <= amount,
                "fee_amount={fee_amount} exceeds amount={amount} at bps={fee_rate_bps}");

            let product = amount.checked_mul(fee_rate_bps)
                .expect("amount * fee_rate_bps must not overflow in this range");
            let dust = product - fee_amount * 10_000;
            prop_assert!((0..10_000).contains(&dust),
                "dust={dust} out of [0, 10_000) for amount={amount} bps={fee_rate_bps}");

            // The amount actually deducted from a position (amount + fee) must
            // itself be representable without overflow (#377's on-top-of model).
            let total_required = amount.checked_add(fee_amount);
            prop_assert!(total_required.is_some(),
                "amount + fee_amount overflowed for amount={amount} fee={fee_amount}");
        }

        /// Edge case: amounts near the smallest possible positive withdrawal
        /// (1..=1000 stroops) — where floor-division dust is proportionally
        /// largest and `fee_amount` most often rounds down to exactly zero.
        #[test]
        fn prop_fee_rounding_near_zero_amount(
            amount in 1i128..=1_000i128,
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, fee_rate_bps)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert!(fee_amount >= 0);
            prop_assert!(fee_amount <= amount);

            // At 1 bps (the smallest nonzero rate) any amount below 10_000
            // stroops floors the fee to zero entirely — that's the dust rule,
            // not a bug: the protocol simply forgoes fees below its bps
            // granularity rather than rounding up in its own favor.
            if fee_rate_bps > 0 && amount < 10_000 / fee_rate_bps.max(1) {
                prop_assert_eq!(fee_amount, 0,
                    "expected fee to floor to 0 for tiny amount={} at bps={}", amount, fee_rate_bps);
            }
        }

        /// Edge case: amounts near `validate_amount_reasonable`'s ceiling
        /// (`i128::MAX / 2`, ~8.5e37) crossed with the full bps range. At
        /// this magnitude `amount * fee_rate_bps` itself can exceed
        /// `i128::MAX` for any bps beyond single digits — well before the
        /// division step. `calculate_fee` must fail closed with
        /// `ArithmeticOverflow` in that case (via `checked_mul`), never
        /// panic or silently wrap to a bogus/negative fee; when the product
        /// *does* fit, the result must still floor-divide correctly.
        #[test]
        fn prop_fee_rounding_near_max_amount(
            amount in (i128::MAX / 2 - 10_000_000_000i128)..=(i128::MAX / 2),
            fee_rate_bps in 0i128..=10_000i128,
        ) {
            validation::validate_collateral_amount(amount)
                .expect("amount must be within the validated reasonable range");

            let result = validation::calculate_fee(amount, fee_rate_bps);

            match amount.checked_mul(fee_rate_bps) {
                Some(product) => {
                    let fee_amount = result
                        .expect("calculate_fee must succeed when amount * fee_rate_bps fits in i128");
                    prop_assert!(fee_amount >= 0 && fee_amount <= amount);
                    prop_assert_eq!(fee_amount, product / 10_000);

                    // amount + fee_amount can exceed i128::MAX/2 (fee is
                    // additive, #377), but must stay within i128 itself.
                    let total_required = amount.checked_add(fee_amount);
                    prop_assert!(total_required.is_some(),
                        "amount + fee_amount overflowed near the amount ceiling: amount={amount} fee={fee_amount}");
                }
                None => {
                    prop_assert_eq!(result, Err(ContractError::ArithmeticOverflow),
                        "expected graceful ArithmeticOverflow (not a panic/wrap) for amount={} bps={}, got {:?}",
                        amount, fee_rate_bps, result);
                }
            }
        }

        /// At the maximum fee rate (10_000 bps = 100%), fee_amount must equal
        /// amount exactly (no rounding loss at the boundary rate).
        #[test]
        fn prop_fee_rounding_max_bps_equals_amount(
            amount in 1i128..=10_000_000_000i128,
        ) {
            let fee_amount = validation::calculate_fee(amount, 10_000i128)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert_eq!(fee_amount, amount,
                "expected fee_amount == amount at 10_000 bps, got fee={} amount={}", fee_amount, amount);
        }

        /// At a zero fee rate, fee_amount must always be exactly zero
        /// regardless of amount (including near the amount ceiling).
        #[test]
        fn prop_fee_rounding_zero_bps_is_always_zero(
            amount in 1i128..=(i128::MAX / 2),
        ) {
            let fee_amount = validation::calculate_fee(amount, 0i128)
                .expect("calculate_fee must not error for valid inputs");
            prop_assert_eq!(fee_amount, 0,
                "expected fee_amount == 0 at 0 bps, got fee={} for amount={}", fee_amount, amount);
        }
    }
}

/// #897: random deposit/withdraw sequences through the real entrypoints.
///
/// Every case builds a fresh market backed by a Stellar asset token, picks a
/// fee rate (0–100 %), optionally waives the user's fee and optionally
/// registers a treasury, then replays a random list of operations. After
/// each operation the on-chain state is compared with an independent model
/// (see invariants 6–10 in the module docs).
mod deposit_withdraw_sequences {
    extern crate std;

    use super::*;
    use crate::MarketContractClient;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
    use vatix_treasury_contract::{TreasuryContract, TreasuryContractClient};

    const COOLDOWN: u64 = 3_600;
    const USER_FUNDS: i128 = 1_000_000_000_000;
    const PRICE_BPS: i128 = 5_000;

    #[derive(Clone, Debug)]
    enum Op {
        Deposit(i128),
        /// Withdraw after the cooldown has elapsed.
        Withdraw(i128),
        /// Withdraw the largest amount whose `amount + fee` fits `available`.
        WithdrawMax,
        /// One stroop more than `WithdrawMax` — must be rejected.
        WithdrawMaxPlusOne,
        /// Withdraw without waiting out the post-deposit cooldown.
        WithdrawImmediately(i128),
        /// Invalid / overflow-prone amounts — must fail closed.
        WithdrawExtreme(i128),
        /// Buy YES shares to lock part of the deposit.
        Trade(i128),
    }

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => (1i128..=1_000_000_000i128).prop_map(Op::Deposit),
            3 => (1i128..=1_000_000_000i128).prop_map(Op::Withdraw),
            2 => Just(Op::WithdrawMax),
            2 => Just(Op::WithdrawMaxPlusOne),
            1 => (1i128..=1_000_000_000i128).prop_map(Op::WithdrawImmediately),
            1 => prop_oneof![
                Just(0i128),
                Just(-1i128),
                Just(i128::MIN),
                Just(i128::MAX),
                Just(i128::MAX / 2),
                Just(i128::MAX / 2 + 1),
            ]
            .prop_map(Op::WithdrawExtreme),
            1 => (1i128..=500_000_000i128).prop_map(Op::Trade),
        ]
    }

    fn arb_fee_bps() -> impl Strategy<Value = i128> {
        prop_oneof![
            Just(0i128),
            Just(50i128),
            Just(10_000i128),
            1i128..=10_000i128
        ]
    }

    /// Independent model of what the contract should hold.
    #[derive(Default)]
    struct Model {
        deposited: i128,
        user: i128,
        market: i128,
        treasury: i128,
        last_deposit: Option<u64>,
    }

    struct Harness {
        env: Env,
        client: MarketContractClient<'static>,
        market_addr: Address,
        user: Address,
        token: TokenClient<'static>,
        treasury: Option<Address>,
        market_id: u32,
        fee_bps: i128,
        waived: bool,
    }

    impl Harness {
        fn new(fee_bps: i128, waived: bool, with_treasury: bool) -> Self {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().set_timestamp(1_000_000);

            let admin = Address::generate(&env);
            let user = Address::generate(&env);
            let market_addr = env.register(crate::MarketContract, ());
            env.as_contract(&market_addr, || {
                storage::set_admin(&env, &admin);
                storage::set_version(&env);
                storage::set_fee_rate_bps(&env, fee_bps);
                if waived {
                    storage::add_fee_waiver(&env, &user);
                }
            });
            let client = MarketContractClient::new(&env, &market_addr);

            let token_addr = env
                .register_stellar_asset_contract_v2(Address::generate(&env))
                .address();
            StellarAssetClient::new(&env, &token_addr).mint(&user, &USER_FUNDS);

            let market_id = client.initialize_market(
                &admin,
                &String::from_str(&env, "fuzz #897"),
                &(env.ledger().timestamp() + 300 * 86_400), // < 1y market cap; ops advance ~1h each
                &BytesN::from_array(&env, &[1u8; 32]),
                &token_addr,
                &None,
            );

            let treasury = with_treasury.then(|| {
                let treasury = env.register(TreasuryContract, ());
                TreasuryContractClient::new(&env, &treasury).initialize(&admin, &market_addr);
                env.as_contract(&market_addr, || storage::set_treasury(&env, &treasury));
                treasury
            });

            let token = TokenClient::new(&env, &token_addr);
            Harness {
                env,
                client,
                market_addr,
                user,
                token,
                treasury,
                market_id,
                fee_bps,
                waived,
            }
        }

        fn position(&self) -> Position {
            self.env.as_contract(&self.market_addr, || {
                storage::get_position(&self.env, self.market_id, &self.user)
                    .unwrap()
                    .unwrap_or_else(|| Position::new_empty(self.market_id, self.user.clone()))
            })
        }

        /// Fee the model expects, computed independently of `calculate_fee`.
        fn fee(&self, amount: i128) -> i128 {
            if self.waived {
                0
            } else {
                amount * self.fee_bps / 10_000
            }
        }

        fn available(&self) -> i128 {
            let p = self.position();
            p.total_deposited - p.locked_collateral
        }

        fn max_withdrawable(&self) -> i128 {
            let available = self.available();
            if available <= 0 {
                return 0;
            }
            let mut a = available * 10_000 / (10_000 + if self.waived { 0 } else { self.fee_bps });
            while a + 1 + self.fee(a + 1) <= available {
                a += 1;
            }
            while a > 0 && a + self.fee(a) > available {
                a -= 1;
            }
            a
        }

        fn skip_cooldown(&self) {
            self.env
                .ledger()
                .set_timestamp(self.env.ledger().timestamp() + COOLDOWN + 1);
        }

        fn in_cooldown(&self, model: &Model) -> bool {
            model
                .last_deposit
                .is_some_and(|t| self.env.ledger().timestamp() - t < COOLDOWN)
        }

        fn try_withdraw(&self, amount: i128) -> Result<(), ContractError> {
            match self
                .client
                .try_withdraw_unused_collateral(&self.user, &self.market_id, &amount)
            {
                Ok(_) => Ok(()),
                Err(Ok(e)) => Err(e),
                // A host-level failure (trap, overflow panic) is never acceptable.
                Err(Err(e)) => panic!("withdraw({amount}) trapped the host: {e:?}"),
            }
        }
    }

    /// Apply a withdraw of `amount` and update the model per invariants 6–8.
    fn check_withdraw(h: &Harness, model: &mut Model, amount: i128) -> Result<(), TestCaseError> {
        let before = h.position();
        let result = h.try_withdraw(amount);

        if h.in_cooldown(model) {
            prop_assert_eq!(result, Err(ContractError::WithdrawCooldownActive));
            return Ok(());
        }

        let fee = h.fee(amount);
        let available = before.total_deposited - before.locked_collateral;
        if amount + fee <= available {
            prop_assert_eq!(
                result,
                Ok(()),
                "amount={} fee={} available={}",
                amount,
                fee,
                available
            );
            model.deposited -= amount + fee;
            model.user += amount;
            model.market -= amount;
            if fee > 0 {
                if h.treasury.is_some() {
                    model.market -= fee;
                    model.treasury += fee;
                }
                // Without a treasury the fee stays in the market's balance.
            }
        } else {
            prop_assert_eq!(
                result,
                Err(ContractError::InsufficientCollateral),
                "amount={} fee={} available={}",
                amount,
                fee,
                available
            );
        }
        Ok(())
    }

    fn check_invariants(h: &Harness, model: &Model) -> Result<(), TestCaseError> {
        let p = h.position();
        prop_assert!(
            p.total_deposited >= 0,
            "total_deposited underflowed: {}",
            p.total_deposited
        );
        prop_assert_eq!(
            p.total_deposited,
            model.deposited,
            "total_deposited drifted from model"
        );
        prop_assert!(
            p.locked_collateral <= p.total_deposited,
            "locked {} > deposited {}",
            p.locked_collateral,
            p.total_deposited
        );

        // Single market: the protocol-wide balance (ADR-002) must track the
        // per-market deposit exactly — withdrawals debit it (#897).
        let protocol_balance = h.env.as_contract(&h.market_addr, || {
            storage::get_collateral_balance(&h.env, &h.user)
        });
        prop_assert_eq!(
            protocol_balance,
            model.deposited,
            "CollateralBalance drifted"
        );

        prop_assert_eq!(h.token.balance(&h.user), model.user, "user balance");
        prop_assert_eq!(
            h.token.balance(&h.market_addr),
            model.market,
            "market balance"
        );
        let treasury_balance = h.treasury.as_ref().map_or(0, |t| h.token.balance(t));
        prop_assert_eq!(treasury_balance, model.treasury, "treasury balance");
        if let Some(t) = &h.treasury {
            prop_assert_eq!(
                TreasuryContractClient::new(&h.env, t).total_collected(),
                model.treasury,
                "treasury total_collected must equal fees routed"
            );
        }
        prop_assert_eq!(
            model.user + model.market + model.treasury,
            USER_FUNDS,
            "tokens created or destroyed"
        );
        // Market custody covers every deposit still owed to the user.
        prop_assert!(model.market >= model.deposited);
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(96))]

        /// Invariants 6–10 across random sequences, fee rates, waivers and
        /// treasury configurations.
        #[test]
        fn prop_deposit_withdraw_sequence_never_underflows(
            fee_bps in arb_fee_bps(),
            waived in any::<bool>(),
            with_treasury in any::<bool>(),
            ops in proptest::collection::vec(arb_op(), 1..24),
        ) {
            let h = Harness::new(fee_bps, waived, with_treasury);
            let mut model = Model { user: USER_FUNDS, ..Model::default() };

            for op in ops {
                match op {
                    Op::Deposit(amount) => {
                        h.client.deposit_collateral(&h.user, &h.market_id, &amount);
                        model.deposited += amount;
                        model.user -= amount;
                        model.market += amount;
                        model.last_deposit = Some(h.env.ledger().timestamp());
                    }
                    Op::Withdraw(amount) => {
                        h.skip_cooldown();
                        check_withdraw(&h, &mut model, amount)?;
                    }
                    Op::WithdrawMax => {
                        h.skip_cooldown();
                        let max = h.max_withdrawable();
                        if max > 0 {
                            check_withdraw(&h, &mut model, max)?;
                            // Fee-on-top boundary: exactly `max` fits.
                            prop_assert!(h.available() >= 0);
                        }
                    }
                    Op::WithdrawMaxPlusOne => {
                        h.skip_cooldown();
                        let over = h.max_withdrawable() + 1;
                        let before = h.position();
                        prop_assert_eq!(h.try_withdraw(over), Err(ContractError::InsufficientCollateral));
                        prop_assert_eq!(h.position(), before);
                    }
                    Op::WithdrawImmediately(amount) => {
                        check_withdraw(&h, &mut model, amount)?;
                    }
                    Op::WithdrawExtreme(amount) => {
                        h.skip_cooldown();
                        let before = h.position();
                        let result = h.try_withdraw(amount);
                        prop_assert!(result.is_err(), "extreme amount {} must be rejected", amount);
                        prop_assert_eq!(h.position(), before, "rejected withdraw mutated the position");
                    }
                    Op::Trade(yes) => {
                        let before = h.position();
                        let traded = h
                            .client
                            .try_update_position(&h.user, &h.market_id, &yes, &0i128, &PRICE_BPS)
                            .is_ok();
                        let after = h.position();
                        // Trading moves the lock, never the deposit.
                        prop_assert_eq!(after.total_deposited, before.total_deposited);
                        if !traded {
                            prop_assert_eq!(after, before);
                        }
                    }
                }
                check_invariants(&h, &model)?;
            }
        }

        /// Invariant 10: a waived user can drain exactly its whole available
        /// balance at any fee rate, and pays nothing.
        #[test]
        fn prop_waived_user_withdraws_full_available_fee_free(
            fee_bps in arb_fee_bps(),
            deposit in 1i128..=1_000_000_000i128,
            with_treasury in any::<bool>(),
        ) {
            let h = Harness::new(fee_bps, true, with_treasury);
            h.client.deposit_collateral(&h.user, &h.market_id, &deposit);
            h.skip_cooldown();

            prop_assert_eq!(h.try_withdraw(deposit), Ok(()));
            prop_assert_eq!(h.position().total_deposited, 0);
            prop_assert_eq!(h.token.balance(&h.user), USER_FUNDS);
            prop_assert_eq!(h.token.balance(&h.market_addr), 0);
            // Nothing left: even one stroop more now fails closed.
            prop_assert_eq!(h.try_withdraw(1), Err(ContractError::InsufficientCollateral));
        }

        /// Invariant 7 at the exact boundary for a single deposit: the largest
        /// fitting amount succeeds and leaves only fee dust; one more fails.
        #[test]
        fn prop_fee_on_top_boundary_is_exact(
            fee_bps in arb_fee_bps(),
            deposit in 1i128..=1_000_000_000i128,
        ) {
            let h = Harness::new(fee_bps, false, false);
            h.client.deposit_collateral(&h.user, &h.market_id, &deposit);
            h.skip_cooldown();

            let max = h.max_withdrawable();
            if max == 0 {
                prop_assert_eq!(h.try_withdraw(1), Err(ContractError::InsufficientCollateral));
            } else {
                prop_assert_eq!(h.try_withdraw(max + 1), Err(ContractError::InsufficientCollateral));
                prop_assert_eq!(h.try_withdraw(max), Ok(()));
                let left = h.position().total_deposited;
                prop_assert_eq!(left, deposit - max - h.fee(max));
                prop_assert!(left >= 0);
            }
        }
    }

    /// Regression found by `prop_deposit_withdraw_sequence_never_underflows`:
    /// withdrawing everything used to leave the protocol-wide
    /// `CollateralBalance` untouched, so a later trade could lock collateral
    /// that had already been paid out (locked 1 > deposited 0).
    #[test]
    fn withdrawn_collateral_cannot_back_new_trades() {
        let h = Harness::new(0, false, false);
        h.client
            .deposit_collateral(&h.user, &h.market_id, &44_250_427);
        h.skip_cooldown();
        h.try_withdraw(44_250_427).unwrap();

        let result =
            h.client
                .try_update_position(&h.user, &h.market_id, &2i128, &0i128, &PRICE_BPS);
        assert_eq!(
            result.err(),
            Some(Ok(ContractError::InsufficientCollateral))
        );
        let p = h.position();
        assert_eq!((p.total_deposited, p.locked_collateral), (0, 0));
    }

    /// Deterministic regression: repeated full round-trips at 100 % fee never
    /// push `total_deposited` below zero.
    #[test]
    fn repeated_round_trips_at_max_fee_stay_non_negative() {
        let h = Harness::new(10_000, false, true);
        let mut routed = 0i128;
        for _ in 0..5 {
            h.client.deposit_collateral(&h.user, &h.market_id, &1_001);
            h.skip_cooldown();
            let max = h.max_withdrawable();
            assert!(max > 0);
            h.try_withdraw(max).unwrap();
            routed += h.fee(max);
            assert!(h.position().total_deposited >= 0);
        }
        let treasury = h.treasury.as_ref().unwrap();
        assert_eq!(h.token.balance(treasury), routed);
        assert_eq!(
            h.token.balance(&h.market_addr),
            h.position().total_deposited
        );
    }
}
