//! Criterion benchmarks for the Vesting contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, claim, revoke) using the Soroban host budget.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env};

use vesting::{VestingContract, VestingContractClient};

/// Deploys the Vesting contract and returns the environment and client.
fn setup() -> (Env, VestingContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VestingContract);
    let client = VestingContractClient::new(&env, &contract_id);
    (env, client)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    c.bench_function("vesting_initialize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let beneficiary = Address::generate(&env);
            let token = Address::generate(&env);
            client.initialize(&admin, &beneficiary, &token, &0u64, &1_000u64, &100u64);
            env.budget().cpu_instruction_cost()
        });
    });
}

/// Benchmarks the `claim` entry point.
fn bench_claim(c: &mut Criterion) {
    c.bench_function("vesting_claim", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let beneficiary = Address::generate(&env);
            let token = Address::generate(&env);
            client.initialize(&admin, &beneficiary, &token, &0u64, &1_000u64, &100u64);
            client.claim(&beneficiary);
            env.budget().cpu_instruction_cost()
        });
    });
}

/// Benchmarks the `revoke` entry point.
fn bench_revoke(c: &mut Criterion) {
    c.bench_function("vesting_revoke", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let beneficiary = Address::generate(&env);
            let token = Address::generate(&env);
            client.initialize(&admin, &beneficiary, &token, &0u64, &1_000u64, &100u64);
            client.revoke(&admin);
            env.budget().cpu_instruction_cost()
        });
    });
}

criterion_group!(benches, bench_initialize, bench_claim, bench_revoke);
criterion_main!(benches);