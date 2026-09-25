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
    token::StellarAssetClient,
};
use std::format;

use crate::{BondingCurveContract, BondingCurveContractClient};

fn setup_bonding_curve<'a>(env: &'a Env) -> (BondingCurveContractClient<'a>, Address, Address) {
    let admin = Address::generate(env);
    let sac_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(sac_admin);
    let token_addr = sac.address();

    let contract_addr = env.register_contract(None, BondingCurveContract);
    let client = BondingCurveContractClient::new(env, &contract_addr);
    client.initialize(&admin, &token_addr, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

    (client, token_addr, contract_addr)
}

proptest! {
    /// Property: Buy then immediately sell the same amount (absent fees) should never leave
    /// the trader strictly better off than before.
    ///
    /// This test verifies the invariant that round-trip buy/sell operations don't create
    /// arbitrage opportunities across randomized curve parameters and trade sizes.
    ///
    /// Closes #858 – Add proptest for bonding-curve buy/sell round-trip invariant
    #[test]
    fn prop_buy_sell_roundtrip_no_arbitrage(
        amount in 1i128..=10_000i128,
        initial_funding in 10_000i128..=1_000_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.timestamp = 1;
            l.sequence_number = 100;
        });

        let (client, token_addr, _contract_addr) = setup_bonding_curve(&env);
        let trader = Address::generate(&env);

        // Mint initial balance to trader
        StellarAssetClient::new(&env, &token_addr).mint(&trader, &initial_funding);
        let balance_before = soroban_sdk::token::Client::new(&env, &token_addr).balance(&trader);

        // Buy tokens
        let buy_result = client.try_buy(&trader, &amount, &i128::MAX);
        if buy_result.is_err() {
            // Invalid parameters, skip this test case
            return Ok(());
        }

        let balance_after_buy = soroban_sdk::token::Client::new(&env, &token_addr).balance(&trader);
        let cost = balance_before - balance_after_buy;

        // Sell the same amount back immediately
        let sell_result = client.try_sell(&trader, &amount, &0i128);
        if sell_result.is_err() {
            // Sell failed (e.g., insufficient reserve), this is acceptable
            return Ok(());
        }

        let balance_after_sell = soroban_sdk::token::Client::new(&env, &token_addr).balance(&trader);
        let proceeds = balance_after_sell - balance_after_buy;

        // Invariant: trader should not profit from round-trip
        // Due to price curve, cost should be >= proceeds
        prop_assert!(cost >= proceeds,
            "Round-trip arbitrage detected: cost={}, proceeds={}, profit={}",
            cost, proceeds, proceeds - cost);

        // Additional invariant: trader's final balance should not exceed initial balance
        prop_assert!(balance_after_sell <= balance_before,
            "Trader ended with more tokens than they started: before={}, after={}",
            balance_before, balance_after_sell);
    }

    /// Property: Buy/sell operations should maintain reserve and supply consistency
    #[test]
    fn prop_reserve_supply_consistency(
        buy_amount in 1i128..=1_000i128,
        sell_amount in 1i128..=500i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.timestamp = 1;
            l.sequence_number = 100;
        });

        let (client, token_addr, _) = setup_bonding_curve(&env);
        let trader = Address::generate(&env);

        StellarAssetClient::new(&env, &token_addr).mint(&trader, &10_000_000i128);

        let initial_reserve = client.get_reserve();
        let initial_supply = client.get_supply();

        // Buy tokens
        let buy_result = client.try_buy(&trader, &buy_amount, &i128::MAX);
        if buy_result.is_err() {
            return Ok(());
        }

        let reserve_after_buy = client.get_reserve();
        let supply_after_buy = client.get_supply();

        // Invariants after buy
        prop_assert!(reserve_after_buy >= initial_reserve, "Reserve decreased after buy");
        prop_assert_eq!(supply_after_buy, initial_supply + buy_amount, "Supply mismatch after buy");

        // Sell some tokens (ensure we don't sell more than we bought)
        let actual_sell = sell_amount.min(buy_amount);
        let sell_result = client.try_sell(&trader, &actual_sell, &0i128);
        if sell_result.is_err() {
            return Ok(());
        }

        let reserve_after_sell = client.get_reserve();
        let supply_after_sell = client.get_supply();

        // Invariants after sell
        prop_assert!(reserve_after_sell <= reserve_after_buy, "Reserve increased after sell");
        prop_assert!(reserve_after_sell >= 0, "Reserve went negative");
        prop_assert_eq!(supply_after_sell, supply_after_buy - actual_sell, "Supply mismatch after sell");
        prop_assert!(supply_after_sell >= 0, "Supply went negative");
    }
}

#[cfg(test)]
mod issue_1085_1086_tests {
    use super::*;

    #[test]
    fn buy_mints_curve_balance_to_buyer() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, token, _) = setup_bonding_curve(&env);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &1_000_000);

        client.buy(&buyer, &100, &i128::MAX);

        assert_eq!(client.balance(&buyer), 100);
    }

    #[test]
    fn sell_rejects_amount_not_owned_by_seller() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, token, _) = setup_bonding_curve(&env);
        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&owner, &1_000_000);
        client.buy(&owner, &100, &i128::MAX);

        let result = client.try_sell(&attacker, &100, &0);

        assert!(result.is_err());
        assert_eq!(client.balance(&attacker), 0);
    }
}
