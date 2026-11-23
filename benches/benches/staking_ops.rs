//! Criterion benchmarks for the Staking contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, stake, unstake, claim_rewards, add_rewards) using the Soroban
//! host budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

use staking_contract::{StakingContract, StakingContractClient};

/// Deploy the Staking contract and return the environment, client, and admin.
fn setup() -> (Env, StakingContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StakingContract);
    let client = StakingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    (env, client, admin)
}

/// Deploy a Stellar asset contract used as the staking reward token.
fn deploy_token(env: &Env, admin: &Address) -> Address {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    token_id.address()
}

/// Mint `amount` of the given token to `to`.
fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    let admin_client = token::StellarAssetClient::new(env, token);
    admin_client.mint(to, &amount);
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_stake(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_stake");

    group.bench_function("stake", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);

            client.stake(&staker, &100_000);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_unstake(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_unstake");

    group.bench_function("unstake", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);

            client.stake(&staker, &100_000);

            // Advance the ledger so any lock/cooldown period has elapsed.
            env.ledger().with_mut(|li| {
                li.sequence_number += 1_000;
                li.timestamp += 1_000;
            });

            client.unstake(&staker, &100_000);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_claim_rewards(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_claim_rewards");

    group.bench_function("claim_rewards", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);
            mint(&env, &reward_token, &admin, 1_000_000);

            client.stake(&staker, &100_000);
            client.add_rewards(&admin, &50_000);

            // Advance the ledger so rewards accrue.
            env.ledger().with_mut(|li| {
                li.sequence_number += 1_000;
                li.timestamp += 1_000;
            });

            client.claim_rewards(&staker);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_add_rewards(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_add_rewards");

    group.bench_function("add_rewards", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);
            mint(&env, &reward_token, &admin, 1_000_000);

            client.stake(&staker, &100_000);

            client.add_rewards(&admin, &50_000);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_slash(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_slash");

    group.bench_function("slash", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);

            client.stake(&staker, &100_000);

            // Slash 10% of the staker's principal.
            client.slash(&admin, &staker, &1_000);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_request_unbond(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_request_unbond");

    group.bench_function("request_unbond", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);

            client.stake(&staker, &100_000);

            client.request_unbond(&staker, &100_000);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_claim_unbonded(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_claim_unbonded");

    group.bench_function("claim_unbonded", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);

            client.stake(&staker, &100_000);
            client.request_unbond(&staker, &100_000);

            // Advance the ledger past the unbonding maturation delay.
            env.ledger().with_mut(|li| {
                li.sequence_number += 200_000;
                li.timestamp += 200_000;
            });

            client.claim_unbonded(&staker);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_compound(c: &mut Criterion) {
    let mut group = c.benchmark_group("staking_compound");

    group.bench_function("compound", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let reward_token = deploy_token(&env, &admin);
            let staking_token = deploy_token(&env, &admin);

            client.initialize(&admin, &staking_token, &reward_token);

            let staker = Address::generate(&env);
            mint(&env, &staking_token, &staker, 1_000_000);
            mint(&env, &reward_token, &admin, 1_000_000);

            client.stake(&staker, &100_000);
            client.add_rewards(&admin, &50_000);

            // Advance the ledger so rewards accrue before compounding.
            env.ledger().with_mut(|li| {
                li.sequence_number += 1_000;
                li.timestamp += 1_000;
            });

            client.compound(&staker);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_stake,
    bench_unstake,
    bench_claim_rewards,
    bench_add_rewards,
    bench_slash,
    bench_request_unbond,
    bench_claim_unbonded,
    bench_compound,
);
criterion_main!(benches);