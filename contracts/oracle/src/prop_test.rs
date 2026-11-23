#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
};

use crate::{OracleContract, OracleContractClient};

fn setup_oracle<'a>(env: &'a Env) -> (OracleContractClient<'a>, Address) {
    let admin = Address::generate(env);
    let oracle_addr = env.register(OracleContract, ());
    let client = OracleContractClient::new(env, &oracle_addr);
    client.initialize(&admin, &100u32);
    (client, admin)
}

proptest! {
    /// Any positive price should be accepted and retrievable without stale price error.
    #[test]
    fn prop_positive_price_accepted(price in 1i128..=i128::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_oracle(&env);

        let result = client.try_update_price(&price);
        prop_assert!(result.is_ok());

        let retrieved = client.get_price();
        prop_assert_eq!(retrieved, price);
    }

    /// Zero and negative prices must be rejected with InvalidPrice (#1258).
    #[test]
    fn prop_non_positive_price_rejected(price in i128::MIN..=0i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_oracle(&env);

        let result = client.try_update_price(&price);
        prop_assert!(result.is_err());
    }

    /// Rapid sequential positive price updates should always leave get_price consistent
    /// with the last accepted value.
    #[test]
    fn prop_rapid_sequential_updates_consistent(
        prices in proptest::collection::vec(1i128..=i128::MAX, 1..=20),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_oracle(&env);

        let mut last_price = 0i128;

        for price in prices.iter() {
            env.ledger().with_mut(|l| {
                l.timestamp += 1;
                l.sequence_number += 1;
            });

            let result = client.try_update_price(price);
            prop_assert!(result.is_ok());
            last_price = *price;
        }

        let final_price = client.get_price();
        prop_assert_eq!(final_price, last_price);
    }

    /// get_price and get_price_data should always return the same price value.
    #[test]
    fn prop_get_price_consistency(price in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup_oracle(&env);

        client.update_price(&price);

        let price_from_get = client.get_price();
        let price_from_data = client.get_price_data().price;

        prop_assert_eq!(price_from_get, price_from_data);
        prop_assert_eq!(price_from_get, price);
    }
}

// ─── #1194 – TWAP duplicate-timestamp regression tests ───────────────────────

#[cfg(test)]
mod twap_dedup_tests {
    use soroban_sdk::{Env, Vec as SdkVec};
    use crate::{PriceObservation, twap};

    /// When all observations share the same timestamp the TWAP must equal
    /// the last recorded price (not panic or silently return 0).
    #[test]
    fn twap_all_same_timestamp_returns_last_price() {
        let env = Env::default();
        let mut obs: SdkVec<PriceObservation> = SdkVec::new(&env);
        obs.push_back(PriceObservation { price: 100, timestamp: 42 });
        obs.push_back(PriceObservation { price: 200, timestamp: 42 });
        obs.push_back(PriceObservation { price: 300, timestamp: 42 });

        // `now` is also the same timestamp — total_weight collapses to 0.
        let result = twap(&obs, 42);
        assert_eq!(result, 300, "must fall back to last price when all timestamps equal");
    }

    /// When `now` equals the last observation's timestamp the trailing
    /// zero-duration window is skipped and the TWAP is computed from the
    /// valid prefix only.
    #[test]
    fn twap_now_equals_last_timestamp_skips_trailing_zero() {
        let env = Env::default();
        let mut obs: SdkVec<PriceObservation> = SdkVec::new(&env);
        // Observation at t=10 (price=100) held for 10 ledgers.
        obs.push_back(PriceObservation { price: 100, timestamp: 10 });
        // Observation at t=20 (price=200) — `now` is also 20, trailing window = 0.
        obs.push_back(PriceObservation { price: 200, timestamp: 20 });

        // Only the first window (t=10..20, weight=10) contributes.
        let result = twap(&obs, 20);
        assert_eq!(result, 100, "trailing zero-duration window must be skipped");
    }

    /// Normal observations with distinct timestamps produce a correct TWAP.
    #[test]
    fn twap_distinct_timestamps_correct() {
        let env = Env::default();
        let mut obs: SdkVec<PriceObservation> = SdkVec::new(&env);
        // price=100 held for 10 ledgers (t=0..10)
        obs.push_back(PriceObservation { price: 100, timestamp: 0 });
        // price=200 held for 10 ledgers (t=10..20)
        obs.push_back(PriceObservation { price: 200, timestamp: 10 });
        // `now` = 20 → total window = 20, weighted_sum = 100*10 + 200*10 = 3000
        let result = twap(&obs, 20);
        assert_eq!(result, 150);
    }

    /// Push-history deduplication: two submissions at the same timestamp
    /// must NOT create two entries — only one averaged entry must exist.
    #[test]
    fn push_history_deduplicates_same_timestamp() {
        use crate::{OracleContract, OracleContractClient};
        use soroban_sdk::Address;
        use soroban_sdk::testutils::Address as _;

        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let oracle_addr = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &oracle_addr);
        client.initialize(&admin, &500u32);

        // Two update_price calls in the same ledger share the same timestamp.
        client.update_price(&100i128);
        client.update_price(&200i128);

        // There should be exactly one history entry (the merged one).
        let history = client.get_history();
        assert_eq!(history.len(), 1, "duplicate-timestamp entries must be merged");
        // Merged price = (100 + 200) / 2 = 150.
        assert_eq!(history.get_unchecked(0).price, 150);
    }
}