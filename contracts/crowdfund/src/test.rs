#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

fn setup(env: &Env) -> (CrowdfundContractClient, Address, Address, Address, Address) {
    let creator = Address::generate(env);
    let contributor1 = Address::generate(env);
    let contributor2 = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(creator.clone());
    let token = sac.address();
    StellarAssetClient::new(env, &token).mint(&contributor1, &10_000);
    StellarAssetClient::new(env, &token).mint(&contributor2, &10_000);

    let addr = env.register_contract(None, CrowdfundContract);
    let client = CrowdfundContractClient::new(env, &addr);

    (client, creator, contributor1, contributor2, token)
}

// ---------------------------------------------------------------------------
// Goal-met path
// ---------------------------------------------------------------------------

#[test]
fn test_goal_met_creator_claims() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    let goal = 5_000_i128;
    client.initialize(&creator, &token, &goal, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &3_000, &None);
    client.pledge(&c2, &2_500, &None);

    assert_eq!(client.get_pledge(&c1), 3_000);
    assert_eq!(client.get_info().total_pledged, 5_500);

    // Advance past deadline
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.claim();

    // Claimed flag set
    assert!(client.get_info().claimed);
}

#[test]
fn test_pledge_increments_total() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 50;
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &400, &None);
    assert_eq!(client.get_info().total_pledged, 400);
    client.pledge(&c1, &200, &None);
    assert_eq!(client.get_info().total_pledged, 600);
    assert_eq!(client.get_pledge(&c1), 600);
}

#[test]
fn test_withdraw_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 50;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &3_000, &None);
    client.withdraw(&c1);

    assert_eq!(client.get_pledge(&c1), 0);
    assert_eq!(client.get_info().total_pledged, 0);
}

// ---------------------------------------------------------------------------
// Goal-not-met path
// ---------------------------------------------------------------------------

#[test]
fn test_goal_not_met_contributors_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &1_000, &None);
    client.pledge(&c2, &500, &None);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    client.refund(&c1);
    client.refund(&c2);

    assert_eq!(client.get_pledge(&c1), 0);
    assert_eq!(client.get_pledge(&c2), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_pledge_after_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.pledge(&c1, &500, &None);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_claim_before_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &500, &deadline, &Vec::new(&env), &None);
    client.pledge(&c1, &1_000, &None);
    // deadline not reached
    client.claim();
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_claim_goal_not_met_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);
    client.pledge(&c1, &100, &None);
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.claim();
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_refund_when_goal_met_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.initialize(&creator, &token, &500, &deadline, &Vec::new(&env), &None);
    client.pledge(&c1, &1_000, &None);
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.refund(&c1);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);
}

// ---------------------------------------------------------------------------
// Issue #1170: Creator early campaign cancellation
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_campaign_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &3_000, &None);
    client.pledge(&c2, &2_000, &None);

    // Creator cancels campaign
    client.cancel_campaign();

    let info = client.get_info();
    assert!(info.cancelled);

    // Contributors can immediately refund without waiting for deadline
    client.refund(&c1);
    client.refund(&c2);

    assert_eq!(client.get_pledge(&c1), 0);
    assert_eq!(client.get_pledge(&c2), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_cancel_campaign_after_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.cancel_campaign();
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_pledge_after_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.cancel_campaign();
    client.pledge(&c1, &1_000, &None);
}

// ---------------------------------------------------------------------------
// Issue #1171: Tiered reward perks tracking
// ---------------------------------------------------------------------------

#[test]
fn test_tier_selection_and_tracking() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    let mut tiers = Vec::new(&env);
    tiers.push_back(FundingTier {
        threshold: 1_000,
        description: soroban_sdk::String::from_str(&env, "Bronze"),
        max_backers: Some(2),
    });
    tiers.push_back(FundingTier {
        threshold: 5_000,
        description: soroban_sdk::String::from_str(&env, "Silver"),
        max_backers: None,
    });

    client.initialize(&creator, &token, &10_000, &deadline, &tiers, &None);

    // c1 claims tier 0 (Bronze)
    client.pledge(&c1, &1_000, &Some(0));
    assert_eq!(client.claim_reward_perk(&c1), Some(0));

    // c2 claims tier 0 (Bronze)
    client.pledge(&c2, &1_000, &Some(0));
    assert_eq!(client.claim_reward_perk(&c2), Some(0));

    let info = client.get_info();
    assert_eq!(info.tiers.get(0).unwrap().current_backers, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_tier_capacity_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _, _, token) = setup(&env);

    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(creator.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(&env, &token_addr).mint(&c1, &10_000);
    StellarAssetClient::new(&env, &token_addr).mint(&c2, &10_000);
    StellarAssetClient::new(&env, &token_addr).mint(&c3, &10_000);

    let deadline = env.ledger().sequence() + 100;
    let mut tiers = Vec::new(&env);
    tiers.push_back(FundingTier {
        threshold: 100,
        description: soroban_sdk::String::from_str(&env, "Limited"),
        max_backers: Some(2),
    });

    let addr = env.register_contract(None, CrowdfundContract);
    let client = CrowdfundContractClient::new(&env, &addr);
    env.mock_all_auths();

    client.initialize(&creator, &token_addr, &1_000, &deadline, &tiers, &None);

    client.pledge(&c1, &100, &Some(0));
    client.pledge(&c2, &100, &Some(0));
    // Third pledge should fail - tier capacity exceeded
    client.pledge(&c3, &100, &Some(0));
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_invalid_tier_id() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &1_000, &deadline, &Vec::new(&env), &None);

    // Try to claim tier 5 when no tiers exist
    client.pledge(&c1, &100, &Some(5));
}

// ---------------------------------------------------------------------------
// Issue #1173: Batch refund distribution
// ---------------------------------------------------------------------------

#[test]
fn test_refund_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &500, &None);
    client.pledge(&c2, &600, &None);

    // Advance past deadline - goal not met
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    let mut pledgers = Vec::new(&env);
    pledgers.push_back(c1.clone());
    pledgers.push_back(c2.clone());

    // Anyone can call batch refund
    client.refund_batch(&pledgers);

    assert_eq!(client.get_pledge(&c1), 0);
    assert_eq!(client.get_pledge(&c2), 0);
}

#[test]
fn test_refund_batch_on_cancelled_campaign() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, c1, c2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.initialize(&creator, &token, &10_000, &deadline, &Vec::new(&env), &None);

    client.pledge(&c1, &500, &None);
    client.pledge(&c2, &600, &None);

    // Creator cancels
    client.cancel_campaign();

    let mut pledgers = Vec::new(&env);
    pledgers.push_back(c1.clone());
    pledgers.push_back(c2.clone());

    // Immediate batch refund works
    client.refund_batch(&pledgers);

    assert_eq!(client.get_pledge(&c1), 0);
    assert_eq!(client.get_pledge(&c2), 0);
}

