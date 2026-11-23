#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

extern crate std;

use std::format;

use crate::{LotteryContract, LotteryContractClient};
use proptest::prelude::*;
use soroban_sdk::{Address, Env, Vec, testutils::Address as _};

fn single_split(env: &Env) -> Vec<u32> {
    Vec::from_array(env, [10_000u32])
}

proptest! {
    /// Any positive ticket price should be accepted at initialization.
    #[test]
    fn prop_positive_ticket_price_accepted(ticket_price in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let addr = env.register(LotteryContract, ());
        let client = LotteryContractClient::new(&env, &addr);

        let result = client.try_initialize(
            &admin,
            &token_addr,
            &ticket_price,
            &1u32,
            &single_split(&env),
            &None,
            &None,
            &None,
        );
        prop_assert!(result.is_ok());
    }

    /// Zero and negative ticket prices must be rejected.
    #[test]
    fn prop_non_positive_ticket_price_rejected(ticket_price in i128::MIN..=0i128) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let addr = env.register(LotteryContract, ());
        let client = LotteryContractClient::new(&env, &addr);

        let result = client.try_initialize(
            &admin,
            &token_addr,
            &ticket_price,
            &1u32,
            &single_split(&env),
            &None,
            &None,
            &None,
        );
        prop_assert!(result.is_err());
    }

    /// Zero winner count must always be rejected.
    #[test]
    fn prop_zero_winner_count_rejected(ticket_price in 1i128..=1_000i128) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let addr = env.register(LotteryContract, ());
        let client = LotteryContractClient::new(&env, &addr);

        let result = client.try_initialize(
            &admin,
            &token_addr,
            &ticket_price,
            &0u32,
            &single_split(&env),
            &None,
            &None,
            &None,
        );
        prop_assert!(result.is_err());
    }

    /// Property: batch buying is equivalent to buying one-by-one.
    #[test]
    fn prop_batch_equals_individual(
        count in 1u32..=10u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let lottery_addr = env.register_contract(None, LotteryContract);
        let client = LotteryContractClient::new(&env, &lottery_addr);

        let splits: soroban_sdk::Vec<u32> = {
            let mut v = Vec::new(&env);
            v.push_back(10_000u32);
            v
        };

        client.initialize(
            &admin,
            &token_addr,
            &10i128,
            &1u32,
            &splits,
            &None,
            &None,
        );

        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token_addr).mint(&buyer, &(10i128 * count as i128));
        client.buy_tickets_batch(&buyer, &count);

        prop_assert_eq!(client.get_ticket_count(&buyer), count);
        prop_assert_eq!(client.get_info().total_tickets, count);
    }
}
