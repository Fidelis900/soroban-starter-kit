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
    token::{StellarAssetClient, TokenClient},
};

use crate::{StakingContract, StakingContractClient, StakingError};

const DAY90: u32 = 1_555_200;
const DAY365: u32 = 6_307_200;

struct Ctx<'a> {
    env: Env,
    client: StakingContractClient<'a>,
    stake_token: Address,
    reward_token: Address,
}

fn setup<'a>(unbonding: u32) -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let stake_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let reward_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let id = env.register(StakingContract, ());
    let client = StakingContractClient::new(&env, &id);
    client.initialize(&admin, &stake_token, &reward_token, &unbonding, &Address::generate(&env));
    Ctx { env, client, stake_token, reward_token }
}

fn user(c: &Ctx, amount: i128) -> Address {
    let u = Address::generate(&c.env);
    StellarAssetClient::new(&c.env, &c.stake_token).mint(&u, &amount);
    u
}

fn fund_rewards(c: &Ctx, amount: i128) {
    StellarAssetClient::new(&c.env, &c.reward_token).mint(&c.client.address, &amount);
}

fn at(c: &Ctx, ledger: u32) {
    c.env.ledger().set_sequence_number(ledger);
}

// ── #1337 reserves ───────────────────────────────────────────────────────────

#[test]
fn underfunded_rate_is_rejected() {
    let c = setup(0);
    at(&c, 100);
    fund_rewards(&c, 999);
    // needs (1100-100)*1 = 1000
    assert_eq!(
        c.client.try_set_reward_rate(&1, &1100),
        Err(Ok(StakingError::InsufficientRewardReserves))
    );
    fund_rewards(&c, 1);
    c.client.set_reward_rate(&1, &1100);
}

#[test]
fn unclaimed_rewards_count_against_reserves() {
    let c = setup(0);
    at(&c, 100);
    fund_rewards(&c, 1000);
    let u = user(&c, 100);
    c.client.stake(&u, &100);
    c.client.set_reward_rate(&1, &1100);
    at(&c, 600); // 500 emitted, unclaimed
    // balance 1000, unclaimed 500 + 500 new = 1000 ok; 501 more not ok
    assert_eq!(
        c.client.try_set_reward_rate(&2, &1100),
        Err(Ok(StakingError::InsufficientRewardReserves))
    );
    c.client.set_reward_rate(&1, &1100);
}

// ── #1336 lockup ─────────────────────────────────────────────────────────────

#[test]
fn lockup_boosts_rewards_and_blocks_early_unstake() {
    let c = setup(0);
    at(&c, 10);
    fund_rewards(&c, 100_000);
    let a = user(&c, 100);
    let b = user(&c, 100);
    c.client.stake(&a, &100);
    c.client.stake_with_lockup(&b, &100, &DAY365); // 2x
    assert_eq!(c.client.weight_of(&b), 200);
    assert_eq!(c.client.total_staked(), 200);
    c.client.set_reward_rate(&3, &1010);
    at(&c, 110); // 300 emitted split 1:2
    assert_eq!(c.client.pending_rewards(&a), 100);
    assert_eq!(c.client.pending_rewards(&b), 200);
    assert_eq!(
        c.client.try_unstake(&b, &100),
        Err(Ok(StakingError::StillLocked))
    );
    at(&c, 10 + DAY365);
    c.client.unstake(&b, &100);
    assert_eq!(TokenClient::new(&c.env, &c.stake_token).balance(&b), 100);
}

#[test]
fn lockup_tiers_validated() {
    let c = setup(0);
    let u = user(&c, 300);
    assert_eq!(
        c.client.try_stake_with_lockup(&u, &100, &(DAY90 - 1)),
        Err(Ok(StakingError::InvalidLockup))
    );
    c.client.stake_with_lockup(&u, &100, &DAY90);
    assert_eq!(c.client.weight_of(&u), 150);
}

// ── #1338 permit ─────────────────────────────────────────────────────────────

#[test]
fn stake_with_permit_single_call_and_deadline() {
    let c = setup(0);
    at(&c, 50);
    let u = user(&c, 100);
    assert_eq!(
        c.client.try_stake_with_permit(&u, &100, &49),
        Err(Ok(StakingError::PermitExpired))
    );
    c.client.stake_with_permit(&u, &100, &60);
    assert_eq!(c.client.stake_of(&u), 100);
    assert_eq!(TokenClient::new(&c.env, &c.stake_token).balance(&u), 0);
}

// ── #1339 cancel unbonding ───────────────────────────────────────────────────

#[test]
fn cancel_unbonding_restores_stake_and_accrual() {
    let c = setup(100);
    at(&c, 10);
    fund_rewards(&c, 10_000);
    let u = user(&c, 100);
    c.client.stake(&u, &100);
    c.client.set_reward_rate(&1, &1000);
    c.client.unstake(&u, &100);
    assert_eq!(c.client.total_staked(), 0);
    assert!(c.client.unbond_request(&u).is_some());
    at(&c, 20);
    c.client.cancel_unbonding(&u);
    assert_eq!(c.client.total_staked(), 100);
    assert_eq!(c.client.stake_of(&u), 100);
    assert!(c.client.unbond_request(&u).is_none());
    at(&c, 30);
    assert_eq!(c.client.pending_rewards(&u), 10);
    assert_eq!(
        c.client.try_cancel_unbonding(&u),
        Err(Ok(StakingError::NoUnbondRequest))
    );
}
