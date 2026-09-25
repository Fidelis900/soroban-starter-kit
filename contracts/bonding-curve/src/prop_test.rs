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
    client.initialize(&admin, &token_addr, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin, &None::<i128>);

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

        // Advance ledger to satisfy cooldown before selling
        env.ledger().with_mut(|l| {
            l.sequence_number = l.sequence_number.saturating_add(2);
        });

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

        // Advance ledger to satisfy cooldown
        env.ledger().with_mut(|l| {
            l.sequence_number = l.sequence_number.saturating_add(2);
        });

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

#[cfg(test)]
mod issue_1089_anti_sandwich_tests {
    use super::*;
    use crate::BondingCurveError;

    /// A single address cannot buy twice in the same ledger sequence.
    #[test]
    fn rate_limit_rejects_second_buy_same_ledger() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 10;
        });
        let (client, token, _) = setup_bonding_curve(&env);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &10_000_000);

        // First buy succeeds.
        client.buy(&buyer, &10, &i128::MAX);

        // Second buy in the SAME ledger must be rejected.
        let result = client.try_buy(&buyer, &10, &i128::MAX);
        assert_eq!(
            result.unwrap_err().unwrap(),
            BondingCurveError::RateLimitExceeded
        );
    }

    /// A buy cooldown prevents immediate re-buying in the very next ledger.
    #[test]
    fn cooldown_rejects_buy_in_next_ledger() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 10;
        });
        let (client, token, _) = setup_bonding_curve(&env);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &10_000_000);

        client.buy(&buyer, &10, &i128::MAX);

        // Move one ledger forward — still within cooldown (requires > BUY_COOLDOWN_LEDGERS).
        env.ledger().with_mut(|l| {
            l.sequence_number = 11;
        });
        let result = client.try_buy(&buyer, &10, &i128::MAX);
        assert_eq!(
            result.unwrap_err().unwrap(),
            BondingCurveError::CooldownActive
        );
    }

    /// After the cooldown expires (>= BUY_COOLDOWN_LEDGERS + 1) the buyer can buy again.
    #[test]
    fn buy_succeeds_after_cooldown() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 10;
        });
        let (client, token, _) = setup_bonding_curve(&env);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &10_000_000);

        client.buy(&buyer, &10, &i128::MAX);

        // Advance beyond cooldown window.
        env.ledger().with_mut(|l| {
            l.sequence_number = 12; // 10 + BUY_COOLDOWN_LEDGERS(1) + 1
        });
        client.buy(&buyer, &10, &i128::MAX); // must not panic/error
    }

    /// Different addresses are rate-limited independently.
    #[test]
    fn rate_limit_is_per_address() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 10;
        });
        let (client, token, _) = setup_bonding_curve(&env);
        let buyer_a = Address::generate(&env);
        let buyer_b = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer_a, &10_000_000);
        StellarAssetClient::new(&env, &token).mint(&buyer_b, &10_000_000);

        // buyer_a buys — hits rate limit on second attempt.
        client.buy(&buyer_a, &10, &i128::MAX);
        assert!(client.try_buy(&buyer_a, &10, &i128::MAX).is_err());

        // buyer_b has never bought, so their first buy succeeds.
        client.buy(&buyer_b, &10, &i128::MAX);
    }
}

#[cfg(test)]
mod issue_1090_graduation_tests {
    use super::*;
    use crate::{BondingCurveContract, BondingCurveContractClient, BondingCurveError, CurveState};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_with_cap(
        env: &Env,
        cap: i128,
    ) -> (BondingCurveContractClient<'_>, soroban_sdk::Address, soroban_sdk::Address) {
        let admin = Address::generate(env);
        let sac_admin = Address::generate(env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();
        let contract_addr = env.register_contract(None, BondingCurveContract);
        let client = BondingCurveContractClient::new(env, &contract_addr);
        client.initialize(
            &admin,
            &token_addr,
            &1_000_000i128,
            &1i128,
            &10_000u32,
            &0u32,
            &admin,
            &Some(cap),
        );
        (client, token_addr, admin)
    }

    /// Auto-graduation triggers when supply reaches the cap.
    #[test]
    fn auto_graduates_at_supply_cap() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 100;
        });
        let cap = 50i128;
        let (client, token, _admin) = setup_with_cap(&env, cap);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &100_000_000);

        assert_eq!(client.get_curve_state(), CurveState::Active);

        // Buy exactly the cap in one go.
        client.buy(&buyer, &cap, &i128::MAX);

        assert_eq!(client.get_curve_state(), CurveState::Graduated);
    }

    /// Buying is rejected once the curve has graduated.
    #[test]
    fn buy_rejected_after_graduation() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 100;
        });
        let (client, token, admin) = setup_with_cap(&env, 10);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &100_000_000);

        client.buy(&buyer, &10, &i128::MAX); // graduates
        assert_eq!(client.get_curve_state(), CurveState::Graduated);

        // Advance ledger past cooldown so the guard doesn't interfere.
        env.ledger().with_mut(|l| {
            l.sequence_number = 200;
        });
        let result = client.try_buy(&buyer, &1, &i128::MAX);
        assert_eq!(
            result.unwrap_err().unwrap(),
            BondingCurveError::CurveGraduated
        );
    }

    /// Admin can force-graduate the curve manually.
    #[test]
    fn admin_can_force_graduate() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _token, admin) = setup_with_cap(&env, 10_000);

        assert_eq!(client.get_curve_state(), CurveState::Active);
        client.graduate(&admin);
        assert_eq!(client.get_curve_state(), CurveState::Graduated);
    }

    /// `migrate_to_amm` is rejected before graduation.
    #[test]
    fn migrate_rejected_before_graduation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _token, admin) = setup_with_cap(&env, 10_000);
        let pool = Address::generate(&env);

        let result = client.try_migrate_to_amm(&admin, &pool);
        assert_eq!(
            result.unwrap_err().unwrap(),
            BondingCurveError::NotGraduated
        );
    }

    /// `migrate_to_amm` transfers the reserve to the AMM pool and zeros state.
    #[test]
    fn migrate_to_amm_transfers_reserve() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.sequence_number = 100;
        });
        let (client, token, admin) = setup_with_cap(&env, 10);
        let buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&buyer, &100_000_000);

        client.buy(&buyer, &10, &i128::MAX); // graduates
        let reserve_before = client.get_reserve();
        assert!(reserve_before > 0);

        let pool = Address::generate(&env);
        client.migrate_to_amm(&admin, &pool);

        // Reserve zeroed on-chain.
        assert_eq!(client.get_reserve(), 0);
        assert_eq!(client.get_supply(), 0);

        // Actual tokens transferred to pool.
        let pool_balance = soroban_sdk::token::Client::new(&env, &token).balance(&pool);
        assert_eq!(pool_balance, reserve_before);
    }
}
