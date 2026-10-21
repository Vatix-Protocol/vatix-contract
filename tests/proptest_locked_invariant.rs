// Exercise the production calculation without adding a public contract API.
#[allow(dead_code)]
#[path = "../contracts/market/src/error.rs"]
mod error;
#[allow(dead_code)]
#[path = "../contracts/market/src/types.rs"]
mod types;

#[allow(dead_code)]
#[path = "../contracts/market/src/positions.rs"]
mod positions;
#[allow(dead_code)]
#[path = "../contracts/market/src/storage.rs"]
mod storage;
use vatix_market_contract::MarketContract;

use proptest::prelude::*;
proptest! {
    #[test]
    fn locked_collateral_does_not_exceed_funded_shares(
        yes in 0i128..=i128::MAX / 10_000,
        no in 0i128..=i128::MAX / 10_000,
        price_bps in 0i128..=10_000i128,
    ) {
        let locked = positions::calculate_locked_collateral(yes, no, price_bps).unwrap();
        prop_assert!(locked >= 0);
        prop_assert!(locked <= yes.max(no));
    }
}
