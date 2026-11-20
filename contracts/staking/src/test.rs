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

// ── #1450 slashing ───────────────────────────────────────────────────────────

#[test]
fn slash_deducts_percentage_and_routes_to_insurance() {
    let c = setup(0);
    let u = user(&c, 1000);
    c.client.stake(&u, &1000);
    assert_eq!(c.client.total_staked(), 1000);
    let insurance = c.client.insurance_pool();
    let before = TokenClient::new(&c.env, &c.stake_token).balance(&insurance);

    c.client.slash(&u, &2500); // 25%

    assert_eq!(c.client.stake_of(&u), 750);
    assert_eq!(c.client.total_staked(), 750);
    assert_eq!(c.client.insurance_pool_balance(), 250);
    let after = TokenClient::new(&c.env, &c.stake_token).balance(&insurance);
    assert_eq!(after - before, 250);
    assert_eq!(TokenClient::new(&c.env, &c.stake_token).balance(&u), 0);
}

#[test]
fn slash_rejects_unauthorized_and_out_of_bounds() {
    let c = setup(0);
    let u = user(&c, 1000);
    c.client.stake(&u, &1000);
    let stranger = Address::generate(&c.env);
    assert_eq!(
        c.client.try_slash(&stranger, &u, &1000),
        Err(Ok(StakingError::Unauthorized))
    );
    assert_eq!(
        c.client.try_slash(&u, &1000),
        Err(Ok(StakingError::InvalidSlashPercentage))
    );
    assert_eq!(
        c.client.try_slash(&u, &10_001),
        Err(Ok(StakingError::InvalidSlashPercentage))
    );
    assert_eq!(c.client.stake_of(&u), 1000);
}

// ── #1451 unbonding queue ────────────────────────────────────────────────────

#[test]
fn unbonding_maturation_timeline() {
    let c = setup(200_000);
    at(&c, 1_000);
    let u = user(&c, 500);
    c.client.stake(&u, &500);

    c.client.request_unbond(&u, &500);
    let req = c.client.unbond_request(&u).unwrap();
    assert_eq!(req.amount, 500);
    assert_eq!(req.available_at, 1_000 + 200_000);
    assert_eq!(c.client.total_staked(), 0);

    at(&c, 1_000 + 200_000 - 1);
    assert_eq!(
        c.client.try_claim_unbonded(&u),
        Err(Ok(StakingError::UnbondingNotComplete))
    );

    at(&c, 1_000 + 200_000);
    c.client.claim_unbonded(&u);
    assert_eq!(TokenClient::new(&c.env, &c.stake_token).balance(&u), 500);
    assert!(c.client.unbond_request(&u).is_none());
}

// ── #1452 dynamic APY decay ──────────────────────────────────────────────────

#[test]
fn reward_rate_decays_as_tvl_grows() {
    let c = setup(0);
    at(&c, 10);
    fund_rewards(&c, 10_000_000);
    c.client.set_reward_params(&1_000, &1_000_000, &500);

    let a = user(&c, 100_000);
    c.client.stake(&a, &100_000);
    let low_tvl_rate = c.client.current_reward_rate();

    let b = user(&c, 900_000);
    c.client.stake(&b, &900_000);
    let high_tvl_rate = c.client.current_reward_rate();

    assert!(high_tvl_rate < low_tvl_rate);
    assert!(high_tvl_rate > 0);
}

#[test]
fn compound_matches_manual_claim_and_stake() {
    let c = setup(0);
    at(&c, 10);
    fund_rewards(&c, 100_000);
    let a = user(&c, 100);
    let b = user(&c, 100);
    c.client.stake(&a, &100);
    c.client.stake(&b, &100);
    c.client.set_reward_rate(&1, &1010);
    at(&c, 110);

    let pending = c.client.pending_rewards(&a);
    assert!(pending > 0);
    c.client.compound(&a);
    assert_eq!(c.client.stake_of(&a), 100 + pending);
    assert_eq!(c.client.pending_rewards(&a), 0);

    // manual path for b
    let pending_b = c.client.pending_rewards(&b);
    c.client.claim(&b);
    c.client.stake(&b, &pending_b);
    assert_eq!(c.client.stake_of(&b), 100 + pending_b);
    assert_eq!(c.client.stake_of(&a), c.client.stake_of(&b));
}

// ── #1452 property: reward pool solvency ─────────────────────────────────────

#[test]
fn prop_reward_pool_never_insolvent() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = |m: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % m
    };
    for _ in 0..32 {
        let c = setup(0);
        at(&c, 10);
        fund_rewards(&c, 1_000_000);
        c.client.set_reward_params(&1_000, &1_000_000, &500);
        let mut stakers: [Option<Address>; 4] = [None, None, None, None];
        for i in 0..4 {
            let amt = 1 + (next(50_000) as i128);
            let u = user(&c, amt);
            c.client.stake(&u, &amt);
            stakers[i] = Some(u);
        }
        let mut ledger = 10u32;
        for _ in 0..20 {
            ledger += 1 + next(5_000) as u32;
            at(&c, ledger);
            let idx = next(4) as usize;
            if let Some(u) = stakers[idx].clone() {
                let bal = c.client.stake_of(&u);
                if bal > 0 && next(2) == 0 {
                    let amt = 1 + (next(bal as u64) as i128);
                    c.client.unstake(&u, &amt);
                } else {
                    let amt = 1 + (next(10_000) as i128);
                    let extra = user(&c, amt);
                    c.client.stake(&extra, &amt);
                    stakers[idx] = Some(extra);
                }
            }
            let pool = TokenClient::new(&c.env, &c.reward_token).balance(&c.client.address);
            let owed = c.client.total_pending_rewards();
            assert!(pool >= owed, "pool {pool} < owed {owed}");
        }
    }
}

// ── #1454 emergency unstake ──────────────────────────────────────────────────

#[test]
fn emergency_unstake_during_pause_with_penalty() {
    let c = setup(0);
    at(&c, 10);
    fund_rewards(&c, 10_000);
    let u = user(&c, 1000);
    c.client.stake(&u, &1000);
    c.client.set_reward_rate(&1, &1000);
    
    at(&c, 110); // accrue 100 ledgers of rewards
    assert!(c.client.pending_rewards(&u) > 0);
    
    // Cannot emergency unstake when not paused
    assert_eq!(
        c.client.try_emergency_unstake(&u),
        Err(Ok(StakingError::ContractNotPaused))
    );
    
    // Pause contract and set 10% penalty
    c.client.pause();
    c.client.set_emergency_penalty(&1000); // 10% = 1000 bps
    assert!(c.client.is_paused());
    
    // Emergency unstake forfeits rewards and applies penalty
    let returned = c.client.emergency_unstake(&u);
    
    // Should return 900 (1000 - 10% penalty), 100 burned
    assert_eq!(returned, 900);
    assert_eq!(c.client.stake_of(&u), 0);
    assert_eq!(c.client.pending_rewards(&u), 0); // rewards forfeited
    assert_eq!(TokenClient::new(&c.env, &c.stake_token).balance(&u), 900);
}

#[test]
fn emergency_unstake_requires_pause() {
    let c = setup(0);
    let u = user(&c, 1000);
    c.client.stake(&u, &1000);
    
    assert_eq!(
        c.client.try_emergency_unstake(&u),
        Err(Ok(StakingError::ContractNotPaused))
    );
}

