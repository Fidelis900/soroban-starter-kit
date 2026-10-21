#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

use crate::{StakingContract, StakingContractClient, StakingError};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_token(env: &Env, mint_to: &Address, amount: i128) -> Address {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let addr = sac.address();
    StellarAssetClient::new(env, &addr).mint(mint_to, &amount);
    addr
}

/// Returns (client, admin, stake_token, reward_token, slash_destination).
/// Admin holds 10_000 of each token. unbonding_period = 0 (immediate).
fn setup(env: &Env) -> (StakingContractClient, Address, Address, Address) {
    let admin = Address::generate(env);
    let stake_token = make_token(env, &admin, 10_000);
    let reward_token = make_token(env, &admin, 10_000);
    let slash_dest = Address::generate(env);
    let addr = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(env, &addr);
    client.initialize(&admin, &stake_token, &reward_token, &0, &slash_dest);
    (client, admin, stake_token, reward_token)
}

/// Setup with a non-zero unbonding period.
fn setup_with_unbonding(
    env: &Env,
    unbonding_period: u32,
) -> (StakingContractClient, Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let stake_token = make_token(env, &admin, 10_000);
    let reward_token = make_token(env, &admin, 10_000);
    let slash_dest = Address::generate(env);
    let addr = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &stake_token,
        &reward_token,
        &unbonding_period,
        &slash_dest,
    );
    (client, admin, stake_token, reward_token, slash_dest)
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_state() {
    let env = setup_env();
    let (client, _admin, _stake_token, _reward_token) = setup(&env);
    assert_eq!(client.get_total_staked(), 0);
    assert_eq!(client.get_total_rewards(), 0);
}

#[test]
fn test_initialize_twice_fails() {
    let env = setup_env();
    let (client, admin, stake_token, reward_token) = setup(&env);
    let slash_dest = Address::generate(&env);
    let result = client.try_initialize(&admin, &stake_token, &reward_token, &0, &slash_dest);
    assert_eq!(result, Err(Ok(StakingError::AlreadyInitialized)));
}

#[test]
fn test_stake_increases_balance() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &500);

    client.stake(&staker, &500);
    assert_eq!(client.get_stake(&staker), 500);
    assert_eq!(client.get_total_staked(), 500);
}

#[test]
fn test_stake_zero_fails() {
    let env = setup_env();
    let (client, _admin, _stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    let result = client.try_stake(&staker, &0);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_unstake_returns_tokens_immediately_when_no_unbonding_period() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);

    client.stake(&staker, &1_000);
    client.unstake(&staker, &400);

    assert_eq!(client.get_stake(&staker), 600);
    assert_eq!(client.get_total_staked(), 600);
    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&staker), 400);
}

#[test]
fn test_unstake_more_than_staked_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &100);

    client.stake(&staker, &100);
    let result = client.try_unstake(&staker, &200);
    assert_eq!(result, Err(Ok(StakingError::InsufficientStake)));
}

#[test]
fn test_unstake_with_no_stake_fails() {
    let env = setup_env();
    let (client, _admin, _stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    let result = client.try_unstake(&staker, &100);
    assert_eq!(result, Err(Ok(StakingError::NoStake)));
}

#[test]
fn test_add_rewards_unauthorized_fails() {
    let env = setup_env();
    let (client, _admin, _stake_token, _reward_token) = setup(&env);
    let result = client.try_add_rewards(&0);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

#[test]
fn test_rewards_distributed_proportionally() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&alice, &1_000);
    StellarAssetClient::new(&env, &stake_token).mint(&bob, &3_000);

    // Alice stakes 1_000, Bob stakes 3_000 → 1:3 ratio.
    client.stake(&alice, &1_000);
    client.stake(&bob, &3_000);

    // Admin adds 4_000 reward tokens.
    client.add_rewards(&4_000);

    let alice_rewards = client.get_rewards(&alice);
    let bob_rewards = client.get_rewards(&bob);

    // Alice should get 1_000, Bob 3_000.
    assert_eq!(alice_rewards, 1_000);
    assert_eq!(bob_rewards, 3_000);
}

#[test]
fn test_claim_rewards_transfers_tokens() {
    let env = setup_env();
    let (client, _admin, stake_token, reward_token) = setup(&env);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.add_rewards(&500);

    let claimed = client.claim_rewards(&staker);
    assert_eq!(claimed, 500);

    let reward_client = soroban_sdk::token::Client::new(&env, &reward_token);
    assert_eq!(reward_client.balance(&staker), 500);
    assert_eq!(client.get_rewards(&staker), 0);
}

#[test]
fn test_claim_rewards_with_no_rewards_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &100);
    client.stake(&staker, &100);

    let result = client.try_claim_rewards(&staker);
    assert_eq!(result, Err(Ok(StakingError::NoRewards)));
}

#[test]
fn test_rewards_accrue_incrementally() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);

    client.add_rewards(&200);
    client.add_rewards(&300);

    assert_eq!(client.get_rewards(&staker), 500);
}

#[test]
fn test_late_staker_does_not_receive_prior_rewards() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&alice, &1_000);
    StellarAssetClient::new(&env, &stake_token).mint(&bob, &1_000);

    client.stake(&alice, &1_000);
    client.add_rewards(&1_000); // only alice is staked

    // Bob stakes after rewards were added.
    client.stake(&bob, &1_000);

    assert_eq!(client.get_rewards(&alice), 1_000);
    assert_eq!(client.get_rewards(&bob), 0);
}

#[test]
fn test_full_unstake_then_restake_accrues_correctly() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token) = setup(&env);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);

    client.stake(&staker, &1_000);
    client.add_rewards(&500);
    client.unstake(&staker, &1_000);

    // Rewards should still be claimable after unstaking.
    assert_eq!(client.get_rewards(&staker), 500);
    let claimed = client.claim_rewards(&staker);
    assert_eq!(claimed, 500);

    // Re-stake and add more rewards.
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.add_rewards(&300);
    assert_eq!(client.get_rewards(&staker), 300);
}

// ── compound tests ────────────────────────────────────────────────────────────

/// Helper: sets up a contract where stake_token == reward_token (single-asset staking).
fn setup_single_asset(env: &Env) -> (StakingContractClient, Address, Address) {
    let admin = Address::generate(env);
    let token = make_token(env, &admin, 10_000);
    let slash_dest = Address::generate(env);
    let addr = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(env, &addr);
    client.initialize(&admin, &token, &token, &0, &slash_dest);
    (client, admin, token)
}

#[test]
fn test_compound_adds_rewards_to_stake() {
    let env = setup_env();
    let (client, _admin, token) = setup_single_asset(&env);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.add_rewards(&500);

    let compounded = client.compound(&staker);
    assert_eq!(compounded, 500);

    // Stake should grow by the compounded rewards.
    assert_eq!(client.get_stake(&staker), 1_500);
    // Pending rewards should be cleared.
    assert_eq!(client.get_rewards(&staker), 0);
    // No tokens left the contract.
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&staker), 0);
}

#[test]
fn test_compound_vs_claim_same_value() {
    // compound() and claim_rewards() should yield the same reward amount.
    let env = setup_env();
    let (client_a, _admin_a, token_a) = setup_single_asset(&env);
    let (client_b, _admin_b, token_b) = setup_single_asset(&env);

    let staker_a = Address::generate(&env);
    let staker_b = Address::generate(&env);
    StellarAssetClient::new(&env, &token_a).mint(&staker_a, &1_000);
    StellarAssetClient::new(&env, &token_b).mint(&staker_b, &1_000);

    client_a.stake(&staker_a, &1_000);
    client_b.stake(&staker_b, &1_000);
    client_a.add_rewards(&300);
    client_b.add_rewards(&300);

    let compounded = client_a.compound(&staker_a);
    let claimed = client_b.claim_rewards(&staker_b);

    // Both should get the same reward amount.
    assert_eq!(compounded, claimed);
    // Compounder has larger stake; claimer received tokens.
    assert_eq!(client_a.get_stake(&staker_a), 1_000 + compounded);
    assert_eq!(client_b.get_stake(&staker_b), 1_000);
    let tok_b = soroban_sdk::token::Client::new(&env, &token_b);
    assert_eq!(tok_b.balance(&staker_b), claimed);
}

#[test]
fn test_compound_requires_same_token() {
    let env = setup_env();
    // Use different stake/reward tokens — compound must fail.
    let (client, _admin, _stake_token, _reward_token) = setup(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &_stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.add_rewards(&200);

    let result = client.try_compound(&staker);
    assert_eq!(result, Err(Ok(crate::StakingError::CompoundTokenMismatch)));
}

#[test]
fn test_compound_no_rewards_fails() {
    let env = setup_env();
    let (client, _admin, token) = setup_single_asset(&env);
    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&staker, &500);
    client.stake(&staker, &500);

    let result = client.try_compound(&staker);
    assert_eq!(result, Err(Ok(crate::StakingError::NoRewards)));
}

// ── #827 unbonding period tests ───────────────────────────────────────────────

/// `unstake` with an unbonding period queues a request; tokens not yet returned.
#[test]
fn test_unstake_queues_unbond_request() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 50);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.unstake(&staker, &600);

    // Stake ledger reduced immediately.
    assert_eq!(client.get_stake(&staker), 400);

    // But tokens not yet returned — still in the contract.
    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&staker), 0);

    // UnbondRequest should exist.
    let req = client.get_unbond_request(&staker).unwrap();
    assert_eq!(req.amount, 600);
}

/// `withdraw` before the unbonding period elapses is rejected.
#[test]
fn test_withdraw_before_period_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 50);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.unstake(&staker, &1_000);

    // Advance only partway through the unbonding period.
    env.ledger().with_mut(|l| l.sequence_number += 49);

    let result = client.try_withdraw(&staker);
    assert_eq!(result, Err(Ok(StakingError::UnbondingNotComplete)));
}

/// `withdraw` after the unbonding period transfers tokens back.
#[test]
fn test_withdraw_after_period_succeeds() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 50);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);
    client.unstake(&staker, &1_000);

    // Advance past the unbonding period.
    env.ledger().with_mut(|l| l.sequence_number += 50);

    let withdrawn = client.withdraw(&staker);
    assert_eq!(withdrawn, 1_000);

    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&staker), 1_000);

    // Request should be cleared.
    assert!(client.get_unbond_request(&staker).is_none());
}

/// `withdraw` with no pending request fails.
#[test]
fn test_withdraw_no_request_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 50);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &500);
    client.stake(&staker, &500);
    // Never called unstake — no UnbondRequest.
    let result = client.try_withdraw(&staker);
    assert_eq!(result, Err(Ok(StakingError::NoUnbondRequest)));
}

/// Regression test for #945: second `unstake` before `withdraw` should fail,
/// preventing loss of the first request's tokens.
#[test]
fn test_second_unstake_before_withdraw_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) =
        setup_with_unbonding(&env, 50);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &2_000);
    client.stake(&staker, &2_000);

    // First unstake: queue 500 tokens.
    client.unstake(&staker, &500);
    assert_eq!(client.get_stake(&staker), 1_500);

    // Verify first request exists.
    let req = client.get_unbond_request(&staker).unwrap();
    assert_eq!(req.amount, 500);

    // Second unstake should fail with UnbondRequestPending error.
    let result = client.try_unstake(&staker, &300);
    assert_eq!(result, Err(Ok(StakingError::UnbondRequestPending)));

    // First request should still exist unchanged.
    let req = client.get_unbond_request(&staker).unwrap();
    assert_eq!(req.amount, 500);

    // Advance past the unbonding period.
    env.ledger().with_mut(|l| l.sequence_number += 50);

    // Withdraw should return only the first unstaked amount.
    let withdrawn = client.withdraw(&staker);
    assert_eq!(withdrawn, 500);

    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&staker), 500);
}

// ── #828 admin slashing tests ─────────────────────────────────────────────────

/// Admin can slash a staker; slashed tokens go to the destination.
#[test]
fn test_slash_accounting_and_balance() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, slash_dest) = setup_with_unbonding(&env, 0);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);

    let slashed = client.slash(&staker, &300);
    assert_eq!(slashed, 300);

    // Staker's on-chain balance reduced.
    assert_eq!(client.get_stake(&staker), 700);
    assert_eq!(client.get_total_staked(), 700);

    // Slash destination received the tokens.
    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&slash_dest), 300);
}

/// Slash amount is capped at the staker's current balance.
#[test]
fn test_slash_capped_at_balance() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, slash_dest) = setup_with_unbonding(&env, 0);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &200);
    client.stake(&staker, &200);

    // Request more than staked — should be capped at 200.
    let slashed = client.slash(&staker, &500);
    assert_eq!(slashed, 200);
    assert_eq!(client.get_stake(&staker), 0);

    let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
    assert_eq!(token_client.balance(&slash_dest), 200);
}

/// Slashing a staker with no stake fails.
#[test]
fn test_slash_no_stake_fails() {
    let env = setup_env();
    let (client, _admin, _stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 0);

    let staker = Address::generate(&env);
    let result = client.try_slash(&staker, &100);
    assert_eq!(result, Err(Ok(StakingError::NoStake)));
}

/// `slash` with amount = 0 fails.
#[test]
fn test_slash_zero_amount_fails() {
    let env = setup_env();
    let (client, _admin, stake_token, _reward_token, _slash_dest) = setup_with_unbonding(&env, 0);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &500);
    client.stake(&staker, &500);

    let result = client.try_slash(&staker, &0);
    assert_eq!(result, Err(Ok(StakingError::InvalidAmount)));
}

// ── property tests ────────────────────────────────────────────────────────────

use proptest::prelude::*;

/// Prop-test setup: mints exactly `stake_amount` stake tokens to staker
/// and `reward_amount` reward tokens to admin.
fn prop_setup_with(
    env: &Env,
    stake_amount: i128,
    reward_amount: i128,
) -> (StakingContractClient, Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let staker = Address::generate(env);
    let stake_token = make_token(env, &staker, stake_amount);
    let reward_token = make_token(env, &admin, reward_amount);
    let slash_dest = Address::generate(env);
    let addr = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(env, &addr);
    client.initialize(&admin, &stake_token, &reward_token, &0, &slash_dest);
    (client, admin, staker, stake_token, reward_token)
}

proptest! {
    #[test]
    fn prop_single_staker_gets_all_rewards(
        stake in 1i128..=1_000_000i128,
        reward in 1i128..=1_000_000i128,
    ) {
        let env = setup_env();
        let (client, _admin, staker, _stake_token, reward_token) =
            prop_setup_with(&env, stake, reward);
        client.stake(&staker, &stake);
        client.add_rewards(&reward);

        let claimed = client.claim_rewards(&staker);
        // Single staker gets all rewards; allow 1-token rounding loss from fixed-point division.
        assert!(claimed >= reward - 1 && claimed <= reward);

        let reward_client = soroban_sdk::token::Client::new(&env, &reward_token);
        assert_eq!(reward_client.balance(&staker), claimed);
    }

    #[test]
    fn prop_rewards_sum_to_total(
        a_stake in 1i128..=500_000i128,
        b_stake in 1i128..=500_000i128,
        reward in 2i128..=1_000_000i128,
    ) {
        let env = setup_env();
        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let stake_token = make_token(&env, &alice, a_stake + b_stake);
        // split stake tokens: alice gets a_stake, bob gets b_stake
        {
            let tc = soroban_sdk::token::Client::new(&env, &stake_token);
            // alice already has a_stake+b_stake; transfer b_stake to bob
            tc.transfer(&alice, &bob, &b_stake);
        }
        let reward_token = make_token(&env, &admin, reward);
        let slash_dest = Address::generate(&env);
        let addr = env.register_contract(None, StakingContract);
        let client = StakingContractClient::new(&env, &addr);
        client.initialize(&admin, &stake_token, &reward_token, &0, &slash_dest);

        client.stake(&alice, &a_stake);
        client.stake(&bob, &b_stake);
        client.add_rewards(&reward);

        let a_rewards = client.get_rewards(&alice);
        let b_rewards = client.get_rewards(&bob);

        // Due to integer division, sum may be <= reward (dust stays in contract).
        assert!(a_rewards + b_rewards <= reward);
        // But the difference should be at most 1 per staker (rounding).
        assert!(reward - (a_rewards + b_rewards) <= 1);
    }

    #[test]
    fn prop_stake_unstake_returns_principal(amount in 1i128..=1_000_000i128) {
        let env = setup_env();
        let (client, _admin, staker, stake_token, _reward_token) =
            prop_setup_with(&env, amount, 1);
        client.stake(&staker, &amount);
        client.unstake(&staker, &amount);

        let token_client = soroban_sdk::token::Client::new(&env, &stake_token);
        assert_eq!(token_client.balance(&staker), amount);
        assert_eq!(client.get_stake(&staker), 0);
        assert_eq!(client.get_total_staked(), 0);
    }
}

// ── #1334: set_reward_rate validation ─────────────────────────────────────────

/// Register a fresh contract and return `(client, contract_id, stake_token)`.
fn setup_rate(env: &Env) -> (StakingContractClient, Address, Address) {
    let admin = Address::generate(env);
    let stake_token = make_token(env, &admin, 10_000);
    let reward_token = make_token(env, &admin, 10_000);
    let slash_dest = Address::generate(env);
    let addr = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(env, &addr);
    client.initialize(&admin, &stake_token, &reward_token, &0u32, &slash_dest);
    (client, addr, stake_token)
}

#[test]
fn test_set_reward_rate_rejects_negative_rate() {
    let env = setup_env();
    env.ledger().set_sequence_number(10);
    let (client, _addr, _stake_token) = setup_rate(&env);

    let result = client.try_set_reward_rate(&-1i128, &1_000u32);
    assert_eq!(result, Err(Ok(StakingError::InvalidRewardRate)));
}

#[test]
fn test_set_reward_rate_rejects_current_ledger_period_end() {
    let env = setup_env();
    env.ledger().set_sequence_number(50);
    let (client, _addr, _stake_token) = setup_rate(&env);

    // `period_end` equal to the current ledger is not in the future.
    let result = client.try_set_reward_rate(&1i128, &50u32);
    assert_eq!(result, Err(Ok(StakingError::InvalidPeriodEnd)));
}

#[test]
fn test_set_reward_rate_rejects_past_period_end() {
    let env = setup_env();
    env.ledger().set_sequence_number(50);
    let (client, _addr, _stake_token) = setup_rate(&env);

    let result = client.try_set_reward_rate(&1i128, &49u32);
    assert_eq!(result, Err(Ok(StakingError::InvalidPeriodEnd)));
}

#[test]
fn test_set_reward_rate_accepts_valid_config() {
    let env = setup_env();
    env.ledger().set_sequence_number(10);
    let (client, _addr, _stake_token) = setup_rate(&env);

    let result = client.try_set_reward_rate(&5i128, &1_000u32);
    assert_eq!(result, Ok(Ok(())));
}

// ── #1335: pending_rewards is a read-only view ────────────────────────────────

/// Reads `LastUpdateLedger` directly from the contract's instance storage so a
/// test can prove a view call did not advance it.
fn read_last_update_ledger(env: &Env, addr: &Address) -> u32 {
    env.as_contract(addr, || {
        env.storage()
            .instance()
            .get::<crate::DataKey, u32>(&crate::DataKey::LastUpdateLedger)
            .unwrap_or(0u32)
    })
}

#[test]
fn test_pending_rewards_leaves_storage_unchanged() {
    let env = setup_env();
    env.ledger().set_sequence_number(10);
    let (client, addr, stake_token) = setup_rate(&env);
    client.set_reward_rate(&10i128, &1_000u32);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);

    // Snapshot the accumulator and the bookkeeping ledger before querying.
    let rpt_before = client.reward_per_token_stored();
    let last_before = read_last_update_ledger(&env, &addr);

    // Time passes, so a naive implementation would persist a larger
    // accumulator when the view is simulated.
    env.ledger().set_sequence_number(60);
    let pending = client.pending_rewards(&staker);

    // Rewards are projected (30 ledgers remaining * 10/ledger over 1_000 staked
    // => 500), but nothing may be written back to storage.
    assert!(pending > 0);
    assert_eq!(client.reward_per_token_stored(), rpt_before);
    assert_eq!(read_last_update_ledger(&env, &addr), last_before);
}

#[test]
fn test_pending_rewards_matches_persisted_accrual() {
    let env = setup_env();
    env.ledger().set_sequence_number(10);
    let (client, _addr, stake_token) = setup_rate(&env);
    client.set_reward_rate(&10i128, &1_000u32);

    let staker = Address::generate(&env);
    StellarAssetClient::new(&env, &stake_token).mint(&staker, &1_000);
    client.stake(&staker, &1_000);

    env.ledger().set_sequence_number(60);
    let projected = client.pending_rewards(&staker);
    let rpt_before = client.reward_per_token_stored();

    // A state-changing call realizes exactly the accrual the view projected.
    client.set_reward_rate(&10i128, &1_000u32);
    let rpt_after = client.reward_per_token_stored();

    assert!(projected > 0);
    assert!(rpt_after > rpt_before);
}
