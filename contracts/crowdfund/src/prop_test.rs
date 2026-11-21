#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use crate::{CrowdfundContract, CrowdfundContractClient};
use proptest::prelude::*;
use soroban_sdk::{Address, Env, testutils::Address as _, token::StellarAssetClient};

proptest! {
    /// Property: Total refunded never exceeds total pledged
    /// Closes #961 – crowdfund pledge/refund accounting invariant
    #[test]
    fn prop_refunds_never_exceed_pledges(
        pledges in proptest::collection::vec(10i128..=1_000i128, 1..=5),
        goal in 10_000i128..=100_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let creator = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let crowdfund_addr = env.register_contract(None, CrowdfundContract);
        let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

        let deadline = env.ledger().sequence() + 1000;
        client.initialize(&creator, &token_addr, &goal, &deadline, &soroban_sdk::Vec::new(&env), &None, &None);

        let mut total_pledged = 0i128;
        let mut backers = vec![];

        // Pledge funds
        for pledge in pledges {
            let backer = Address::generate(&env);
            StellarAssetClient::new(&env, &token_addr).mint(&backer, &pledge);

            if client.try_pledge(&backer, &pledge).is_ok() {
                total_pledged += pledge;
                backers.push(backer);
            }
        }

        // Fail the campaign (advance past deadline without meeting goal)
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

        // Refund all backers
        let mut total_refunded = 0i128;
        for backer in backers {
            let balance_before = soroban_sdk::token::Client::new(&env, &token_addr).balance(&backer);
            let _ = client.try_refund(&backer);
            let balance_after = soroban_sdk::token::Client::new(&env, &token_addr).balance(&backer);
            total_refunded += balance_after - balance_before;
        }

        // Invariant: refunded <= pledged
        prop_assert!(total_refunded <= total_pledged,
            "Refunds exceeded pledges: pledged={}, refunded={}",
            total_pledged, total_refunded);
    }

    /// Property: Pledge amounts never go negative
    #[test]
    fn prop_pledges_never_negative(
        pledge_amount in 10i128..=10_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let creator = Address::generate(&env);
        let backer = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        StellarAssetClient::new(&env, &token_addr).mint(&backer, &pledge_amount);

        let crowdfund_addr = env.register_contract(None, CrowdfundContract);
        let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

        let deadline = env.ledger().sequence() + 1000;
        client.initialize(&creator, &token_addr, &100_000i128, &deadline, &soroban_sdk::Vec::new(&env), &None, &None);

        let _ = client.try_pledge(&backer, &pledge_amount);

        let pledge = client.get_pledge(backer);
        prop_assert!(pledge >= 0, "Pledge amount went negative: {}", pledge);
    }

    /// Property: Pledges after the campaign deadline are always rejected
    /// Closes #1463 – late pledges must fail with CampaignExpired
    #[test]
    fn prop_pledge_after_deadline_rejected(
        deadline_offset in 0u32..=1_000u32,
        late_offset in 1u32..=1_000u32,
        pledge_amount in 10i128..=10_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let creator = Address::generate(&env);
        let backer = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        StellarAssetClient::new(&env, &token_addr).mint(&backer, &pledge_amount);

        let crowdfund_addr = env.register_contract(None, CrowdfundContract);
        let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

        let deadline = env.ledger().sequence() + deadline_offset;
        client.initialize(&creator, &token_addr, &100_000i128, &deadline, &soroban_sdk::Vec::new(&env), &None, &None);

        // Advance past the deadline
        env.ledger().with_mut(|l| l.sequence_number = deadline + late_offset);

        let result = client.try_pledge(&backer, &pledge_amount);
        prop_assert!(result.is_err(), "Late pledge should be rejected");
        prop_assert_eq!(
            result.unwrap_err().unwrap(),
            crate::ContractError::CampaignExpired,
            "Late pledge must return CampaignExpired"
        );

        // No funds should have been recorded for the backer
        prop_assert_eq!(client.get_pledge(backer), 0i128);
    }

    /// Property: Milestone release never transfers more than the tranche amount
    /// and never releases a tranche without a >50% approval quorum.
    /// Closes #1462 – milestone-gated disbursement tranches
    #[test]
    fn prop_milestone_release_respects_tranche_and_quorum(
        tranche_amount in 10i128..=10_000i128,
        approvals in 0u32..=10u32,
        rejections in 0u32..=10u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let creator = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let crowdfund_addr = env.register_contract(None, CrowdfundContract);
        let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

        let goal = tranche_amount;
        let deadline = env.ledger().sequence() + 1000;
        client.initialize(&creator, &token_addr, &goal, &deadline, &soroban_sdk::Vec::new(&env), &None, &None);

        // Fund the campaign so the tranche is available
        let funder = Address::generate(&env);
        StellarAssetClient::new(&env, &token_addr).mint(&funder, &tranche_amount);
        client.pledge(&funder, &tranche_amount);

        // Advance past the deadline so the campaign is successful
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

        // Cast randomized approval / rejection votes
        let mut total_votes = 0u32;
        for _ in 0..approvals {
            let voter = Address::generate(&env);
            if client.try_vote_milestone(&voter, &0u32, &true).is_ok() {
                total_votes += 1;
            }
        }
        for _ in 0..rejections {
            let voter = Address::generate(&env);
            if client.try_vote_milestone(&voter, &0u32, &false).is_ok() {
                total_votes += 1;
            }
        }

        let creator_before = soroban_sdk::token::Client::new(&env, &token_addr).balance(&creator);
        let result = client.try_release_milestone(&0u32);
        let creator_after = soroban_sdk::token::Client::new(&env, &token_addr).balance(&creator);
        let transferred = creator_after - creator_before;

        // Never transfer more than the tranche amount
        prop_assert!(transferred <= tranche_amount,
            "Released {} exceeds tranche {}",
            transferred, tranche_amount);

        // Quorum: approvals must strictly exceed half of all votes cast
        let quorum_met = approvals * 2 > total_votes;
        if quorum_met {
            prop_assert!(result.is_ok(), "Release should succeed with quorum");
            prop_assert_eq!(transferred, tranche_amount);
        } else {
            prop_assert!(result.is_err(), "Release must fail without quorum");
            prop_assert_eq!(transferred, 0i128);
        }
    }

    /// Property: Checked arithmetic prevents overflow near i128::MAX
    /// Closes #1167
    #[test]
    fn prop_pledge_overflow_rejected(
        amount1 in (i128::MAX - 10_000)..=i128::MAX,
        amount2 in 1i128..=10_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let creator = Address::generate(&env);
        let backer1 = Address::generate(&env);
        let backer2 = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        StellarAssetClient::new(&env, &token_addr).mint(&backer1, &i128::MAX);
        StellarAssetClient::new(&env, &token_addr).mint(&backer2, &amount2);

        let crowdfund_addr = env.register_contract(None, CrowdfundContract);
        let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

        let deadline = env.ledger().sequence() + 1000;
        client.initialize(&creator, &token_addr, &i128::MAX, &deadline, &soroban_sdk::Vec::new(&env), &None, &None);

        // First large pledge
        let result1 = client.try_pledge(&backer1, &amount1);

        if result1.is_ok() {
            // Second pledge that would overflow
            let result2 = client.try_pledge(&backer2, &amount2);

            // Should either succeed (if no overflow) or fail with Overflow error
            if amount1.checked_add(amount2).is_none() {
                prop_assert!(result2.is_err(), "Overflow should be rejected");
            }
        }
    }
}
