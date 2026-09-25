#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, Vec,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

use crate::{VestingContract, VestingContractClient, VestingError};

// ── helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 100);
    env
}

pub(crate) fn make_token(env: &Env, mint_to: &Address, amount: i128) -> Address {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let addr = sac.address();
    StellarAssetClient::new(env, &addr).mint(mint_to, &amount);
    addr
}

pub(crate) fn setup(
    env: &Env,
) -> (
    VestingContractClient,
    Address,
    Address,
    Address,
    u32,
    u32,
    i128,
) {
    let admin = Address::generate(env);
    let beneficiary = Address::generate(env);
    let amount = 1_000i128;
    let token = make_token(env, &admin, amount * 2); // Mint extra to allow for potential multiple schedules
    let cliff = env.ledger().sequence() + 10;
    let end = cliff + 100;
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(env, &addr);
    client.initialize(&admin, &token);
    client.create_schedule(&beneficiary, &token, &cliff, &end, &amount);
    client.create_schedule(&beneficiary, &cliff, &end, &amount, &true);
    (client, admin, beneficiary, token, cliff, end, amount)
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_info() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, cliff, end, amount) = setup(&env);
    let info = client.get_info(&beneficiary, &token).unwrap();
    assert_eq!(info.amount, amount);
    assert_eq!(info.cliff_ledger, cliff);
    assert_eq!(info.end_ledger, end);
    assert_eq!(info.claimed, 0);
    assert!(!info.revoked);
}

#[test]
fn test_initialize_twice_fails() {
    let env = setup_env();
    let (client, admin, _beneficiary, token, ..) = setup(&env);
    let result = client.try_initialize(&admin, &token);
    assert_eq!(result, Err(Ok(VestingError::AlreadyInitialized)));
}

#[test]
fn test_create_schedule_zero_amount_fails() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = make_token(&env, &admin, 0);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &token);
    let result = client.try_create_schedule(&beneficiary, &token, &110u32, &200u32, &0i128);
    let result = client.try_create_schedule(&beneficiary, &110u32, &200u32, &0i128, &true);
    assert_eq!(result, Err(Ok(VestingError::InvalidAmount)));
}

#[test]
fn test_create_schedule_invalid_schedule_fails() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = make_token(&env, &admin, 1000);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &token);
    // cliff >= end
    let result = client.try_create_schedule(&beneficiary, &token, &200u32, &150u32, &1000i128);
    let result = client.try_create_schedule(&beneficiary, &200u32, &150u32, &1000i128, &true);
    assert_eq!(result, Err(Ok(VestingError::InvalidSchedule)));
}

#[test]
fn test_claim_before_cliff_fails() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, ..) = setup(&env);
    let result = client.try_claim(&beneficiary, &token);
    assert_eq!(result, Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn test_claim_at_cliff_returns_zero() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, cliff, _end, _amount) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = cliff);
    let result = client.try_claim(&beneficiary, &token);
    assert_eq!(result, Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn test_claim_halfway_through_vesting() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, cliff, end, amount) = setup(&env);
    let mid = cliff + (end - cliff) / 2;
    env.ledger().with_mut(|l| l.sequence_number = mid);
    let claimed = client.claim(&beneficiary, &token);
    assert!(claimed > 0 && claimed <= amount / 2 + 1);
}

#[test]
fn test_claim_after_end_returns_full_amount() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, _cliff, end, amount) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = end + 1);
    let claimed = client.claim(&beneficiary, &token);
    assert_eq!(claimed, amount);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&beneficiary), amount);
}

#[test]
fn test_double_claim_second_returns_nothing() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, _cliff, end, _amount) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = end + 1);
    client.claim(&beneficiary, &token);
    let result = client.try_claim(&beneficiary, &token);
    assert_eq!(result, Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn test_revoke_before_cliff_returns_all() {
    let env = setup_env();
    let (client, admin, beneficiary, token, _cliff, _end, amount) = setup(&env);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let admin_balance_before = token_client.balance(&admin);
    let returned = client.revoke(&beneficiary, &token);
    assert_eq!(returned, amount);
    // Before the cliff, nothing is vested — the schedule's full `amount` comes back.
    assert_eq!(token_client.balance(&admin), admin_balance_before + amount);
}

#[test]
fn test_revoke_after_end_returns_nothing() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, _cliff, end, _amount) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = end + 1);
    let returned = client.revoke(&beneficiary, &token);
    assert_eq!(returned, 0);
}

#[test]
fn test_revoke_midway_returns_unvested_portion() {
    let env = setup_env();
    let (client, admin, beneficiary, token, cliff, end, amount) = setup(&env);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let admin_balance_before = token_client.balance(&admin);
    let mid = cliff + (end - cliff) / 2;
    env.ledger().with_mut(|l| l.sequence_number = mid);
    let returned = client.revoke(&beneficiary, &token);
    assert!(returned > 0 && returned < amount);
    assert_eq!(token_client.balance(&admin), admin_balance_before + returned);
}

#[test]
fn test_claim_after_revoke_gets_vested_portion() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, cliff, end, amount) = setup(&env);
    let mid = cliff + (end - cliff) / 2;
    env.ledger().with_mut(|l| l.sequence_number = mid);
    let returned = client.revoke(&beneficiary, &token);
    let claimed = client.claim(&beneficiary, &token);
    assert_eq!(claimed + returned, amount);
}

#[test]
fn test_revoke_twice_fails() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, ..) = setup(&env);
    client.revoke(&beneficiary, &token);
    let result = client.try_revoke(&beneficiary, &token);
    assert_eq!(result, Err(Ok(VestingError::AlreadyRevoked)));
}

#[test]
fn test_claim_after_full_revoke_fails() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, ..) = setup(&env);
    // revoke before cliff — nothing vested, amount capped to 0
    client.revoke(&beneficiary, &token);
    let result = client.try_claim(&beneficiary, &token);
    assert_eq!(result, Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn test_get_info_uninitialized_returns_none() {
    let env = setup_env();
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    let beneficiary = Address::generate(&env);
    let token = Address::generate(&env);
    assert_eq!(client.get_info(&beneficiary, &token), None);
}

#[test]
fn test_change_beneficiary_moves_schedule() {
    let env = setup_env();
    let (client, _admin, beneficiary, token, ..) = setup(&env);
    assert_eq!(client.claimable(&beneficiary, &token), 0);
    let (client, _admin, old_beneficiary, _token, _cliff, _end, amount) = setup(&env);
    let new_beneficiary = Address::generate(&env);
    client.change_beneficiary(&old_beneficiary, &new_beneficiary);
    assert_eq!(client.get_info(&old_beneficiary), None);
    let info = client.get_info(&new_beneficiary).unwrap();
    assert_eq!(info.amount, amount);
}

// ── tranche / milestone tests (#1141) ─────────────────────────────────────────

/// Build a schedule with quarterly (25% BPS = 2500) tranche unlocks.
fn setup_tranche_schedule(env: &Env) -> (VestingContractClient, Address, Address, u32, i128) {
    let admin = Address::generate(env);
    let beneficiary = Address::generate(env);
    let amount = 1_000i128;
    let token = make_token(env, &admin, amount);
    let start = env.ledger().sequence();
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(env, &addr);
    client.initialize(&admin, &token);

    // Four quarterly tranches, each releasing 25% (2500 BPS).
    let mut tranches: Vec<(u32, u32)> = Vec::new(env);
    tranches.push_back((start + 25, 2_500));
    tranches.push_back((start + 50, 2_500));
    tranches.push_back((start + 75, 2_500));
    tranches.push_back((start + 100, 2_500));

    client.create_tranche_schedule(&beneficiary, &tranches, &amount);
    (client, admin, beneficiary, start, amount)
}

// ── multi-token portfolio tests ───────────────────────────────────────────────

#[test]
fn test_multi_token_schedules_are_independent() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let gov = make_token(&env, &admin, 10_000);
    let reward = make_token(&env, &admin, 10_000);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &gov);

    let cliff = env.ledger().sequence() + 10;
    let end = cliff + 100;
    client.create_schedule(&beneficiary, &gov, &cliff, &end, &1_000i128);
    client.create_schedule(&beneficiary, &reward, &cliff, &end, &2_000i128);

    let gov_info = client.get_info(&beneficiary, &gov).unwrap();
    let reward_info = client.get_info(&beneficiary, &reward).unwrap();
    assert_eq!(gov_info.amount, 1_000);
    assert_eq!(reward_info.amount, 2_000);

    env.ledger().with_mut(|l| l.sequence_number = end + 1);
    let gov_claimed = client.claim(&beneficiary, &gov);
    let reward_claimed = client.claim(&beneficiary, &reward);
    assert_eq!(gov_claimed, 1_000);
    assert_eq!(reward_claimed, 2_000);

    let gov_client = soroban_sdk::token::Client::new(&env, &gov);
    let reward_client = soroban_sdk::token::Client::new(&env, &reward);
    assert_eq!(gov_client.balance(&beneficiary), 1_000);
    assert_eq!(reward_client.balance(&beneficiary), 2_000);
}

#[test]
fn test_multi_token_claim_one_does_not_affect_other() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let gov = make_token(&env, &admin, 10_000);
    let reward = make_token(&env, &admin, 10_000);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &gov);

    let cliff = env.ledger().sequence() + 10;
    let end = cliff + 100;
    client.create_schedule(&beneficiary, &gov, &cliff, &end, &1_000i128);
    client.create_schedule(&beneficiary, &reward, &cliff, &end, &2_000i128);

    env.ledger().with_mut(|l| l.sequence_number = end + 1);
    client.claim(&beneficiary, &gov);

    // The reward schedule is untouched by the governance claim.
    let reward_info = client.get_info(&beneficiary, &reward).unwrap();
    assert_eq!(reward_info.claimed, 0);
    assert_eq!(client.claimable(&beneficiary, &reward), 2_000);
}

#[test]
fn test_multi_token_revoke_only_targets_one_token() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let gov = make_token(&env, &admin, 10_000);
    let reward = make_token(&env, &admin, 10_000);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &gov);

    let cliff = env.ledger().sequence() + 10;
    let end = cliff + 100;
    client.create_schedule(&beneficiary, &gov, &cliff, &end, &1_000i128);
    client.create_schedule(&beneficiary, &reward, &cliff, &end, &2_000i128);

    let returned = client.revoke(&beneficiary, &gov);
    assert_eq!(returned, 1_000);

    let gov_info = client.get_info(&beneficiary, &gov).unwrap();
    let reward_info = client.get_info(&beneficiary, &reward).unwrap();
    assert!(gov_info.revoked);
    assert!(!reward_info.revoked);
}

#[test]
fn test_multi_token_duplicate_schedule_fails() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let gov = make_token(&env, &admin, 10_000);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &gov);

    let cliff = env.ledger().sequence() + 10;
    let end = cliff + 100;
    client.create_schedule(&beneficiary, &gov, &cliff, &end, &1_000i128);
    let result = client.try_create_schedule(&beneficiary, &gov, &cliff, &end, &1_000i128);
    assert_eq!(result, Err(Ok(VestingError::ScheduleAlreadyExists)));
fn test_tranche_schedule_stores_tranches() {
    let env = setup_env();
    let (client, _admin, beneficiary, _start, amount) = setup_tranche_schedule(&env);
    let info = client.get_info(&beneficiary).unwrap();
    assert_eq!(info.amount, amount);
    assert_eq!(info.tranches.len(), 4);
}

#[test]
fn test_tranche_nothing_vested_before_first_boundary() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, _amount) = setup_tranche_schedule(&env);
    env.ledger().with_mut(|l| l.sequence_number = start + 24);
    let result = client.try_claim(&beneficiary);
    assert_eq!(result, Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn test_tranche_first_quarter_unlock() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, amount) = setup_tranche_schedule(&env);
    env.ledger().with_mut(|l| l.sequence_number = start + 25);
    let claimed = client.claim(&beneficiary);
    assert_eq!(claimed, amount * 2_500 / 10_000);
}

#[test]
fn test_tranche_second_quarter_unlock() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, amount) = setup_tranche_schedule(&env);
    env.ledger().with_mut(|l| l.sequence_number = start + 50);
    let claimed = client.claim(&beneficiary);
    assert_eq!(claimed, amount * 5_000 / 10_000);
}

#[test]
fn test_tranche_third_quarter_unlock() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, amount) = setup_tranche_schedule(&env);
    env.ledger().with_mut(|l| l.sequence_number = start + 75);
    let claimed = client.claim(&beneficiary);
    assert_eq!(claimed, amount * 7_500 / 10_000);
}

#[test]
fn test_tranche_final_quarter_unlock_full_amount() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, amount) = setup_tranche_schedule(&env);
    env.ledger().with_mut(|l| l.sequence_number = start + 100);
    let claimed = client.claim(&beneficiary);
    assert_eq!(claimed, amount);
}

#[test]
fn test_tranche_incremental_claims() {
    let env = setup_env();
    let (client, _admin, beneficiary, start, amount) = setup_tranche_schedule(&env);
    let quarter = amount * 2_500 / 10_000;

    env.ledger().with_mut(|l| l.sequence_number = start + 25);
    assert_eq!(client.claim(&beneficiary), quarter);

    env.ledger().with_mut(|l| l.sequence_number = start + 50);
    assert_eq!(client.claim(&beneficiary), quarter);

    env.ledger().with_mut(|l| l.sequence_number = start + 75);
    assert_eq!(client.claim(&beneficiary), quarter);

    env.ledger().with_mut(|l| l.sequence_number = start + 100);
    assert_eq!(client.claim(&beneficiary), quarter);
}

#[test]
fn test_tranche_invalid_percentages_fail() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = make_token(&env, &admin, 1_000);
    let start = env.ledger().sequence();
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &token);

    // Percentages sum to 9000 BPS, not 10000.
    let mut tranches: Vec<(u32, u32)> = Vec::new(&env);
    tranches.push_back((start + 25, 4_500));
    tranches.push_back((start + 50, 4_500));
    let result = client.try_create_tranche_schedule(&beneficiary, &tranches, &1_000i128);
    assert_eq!(result, Err(Ok(VestingError::InvalidSchedule)));
}

#[test]
fn test_milestone_release_by_oracle() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let oracle = Address::generate(&env);
    let amount = 1_000i128;
    let token = make_token(&env, &admin, amount);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &token);
    client.set_oracle(&oracle);

    let mut milestones: Vec<(u32, u32)> = Vec::new(&env);
    milestones.push_back((1, 5_000));
    milestones.push_back((2, 5_000));
    client.create_milestone_schedule(&beneficiary, &milestones, &amount);

    // Nothing released until the oracle verifies a milestone.
    assert_eq!(client.try_claim(&beneficiary), Err(Ok(VestingError::NothingToClaim)));

    client.release_milestone(&beneficiary, &1);
    assert_eq!(client.claim(&beneficiary), amount * 5_000 / 10_000);

    client.release_milestone(&beneficiary, &2);
    assert_eq!(client.claim(&beneficiary), amount * 5_000 / 10_000);
}

#[test]
fn test_milestone_release_unauthorized_fails() {
    let env = setup_env();
    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let oracle = Address::generate(&env);
    let amount = 1_000i128;
    let token = make_token(&env, &admin, amount);
    let addr = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &addr);
    client.initialize(&admin, &token);
    client.set_oracle(&oracle);

    let mut milestones: Vec<(u32, u32)> = Vec::new(&env);
    milestones.push_back((1, 10_000));
    client.create_milestone_schedule(&beneficiary, &milestones, &amount);

    // A non-oracle caller cannot release a milestone.
    env.set_auths(&[]);
    let result = client.try_release_milestone(&beneficiary, &1);
    assert!(result.is_err());
}
