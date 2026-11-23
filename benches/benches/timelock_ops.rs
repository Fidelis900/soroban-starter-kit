//! Criterion benchmarks for the Timelock contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, schedule, execute, cancel) using the Soroban host budget.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use timelock_contract::{TimelockContract, TimelockContractClient};

/// Deploys the Timelock contract and returns the environment and client.
fn setup() -> (Env, TimelockContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &contract_id);

    (env, client)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    c.bench_function("timelock_initialize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let delay: u64 = 100;

            client.initialize(&admin, &delay);

            env.budget().cpu_instruction_cost()
        });
    });
}

/// Benchmarks the `schedule` entry point.
fn bench_schedule(c: &mut Criterion) {
    c.bench_function("timelock_schedule", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let delay: u64 = 100;
            client.initialize(&admin, &delay);

            let target = Address::generate(&env);
            let data = BytesN::from_array(&env, &[0u8; 32]);
            let eta: u64 = env.ledger().timestamp() + delay;

            client.schedule(&admin, &target, &data, &eta);

            env.budget().cpu_instruction_cost()
        });
    });
}

/// Benchmarks the `execute` entry point.
fn bench_execute(c: &mut Criterion) {
    c.bench_function("timelock_execute", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let delay: u64 = 100;
            client.initialize(&admin, &delay);

            let target = Address::generate(&env);
            let data = BytesN::from_array(&env, &[0u8; 32]);
            let eta: u64 = env.ledger().timestamp() + delay;

            client.schedule(&admin, &target, &data, &eta);

            // Advance the ledger past the scheduled execution time.
            env.ledger().set_timestamp(eta + 1);

            client.execute(&admin, &target, &data, &eta);

            env.budget().cpu_instruction_cost()
        });
    });
}

/// Benchmarks the `cancel` entry point.
fn bench_cancel(c: &mut Criterion) {
    c.bench_function("timelock_cancel", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let delay: u64 = 100;
            client.initialize(&admin, &delay);

            let target = Address::generate(&env);
            let data = BytesN::from_array(&env, &[0u8; 32]);
            let eta: u64 = env.ledger().timestamp() + delay;

            client.schedule(&admin, &target, &data, &eta);
            client.cancel(&admin, &target, &data, &eta);

            env.budget().cpu_instruction_cost()
        });
    });
}

criterion_group!(
    timelock_benches,
    bench_initialize,
    bench_schedule,
    bench_execute,
    bench_cancel
);
criterion_main!(timelock_benches);