//! Criterion benchmarks for the Subscription contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, subscribe, renew, cancel) using the Soroban budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, String};
use subscription_contract::{SubscriptionContract, SubscriptionContractClient};

/// Deploys the Subscription contract and returns the environment and client.
fn setup() -> (Env, SubscriptionContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(&env, &contract_id);

    (env, client)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);

            client.initialize(&admin, &token);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `subscribe` entry point.
fn bench_subscribe(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_subscribe");

    group.bench_function("subscribe", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let subscriber = Address::generate(&env);

            client.initialize(&admin, &token);
            client.subscribe(&subscriber, &1_000_000_i128, &86_400_u64);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `renew` entry point.
fn bench_renew(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_renew");

    group.bench_function("renew", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let subscriber = Address::generate(&env);

            client.initialize(&admin, &token);
            client.subscribe(&subscriber, &1_000_000_i128, &86_400_u64);
            client.renew(&subscriber);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `cancel` entry point.
fn bench_cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_cancel");

    group.bench_function("cancel", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let subscriber = Address::generate(&env);

            client.initialize(&admin, &token);
            client.subscribe(&subscriber, &1_000_000_i128, &86_400_u64);
            client.cancel(&subscriber);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks a full subscription lifecycle.
fn bench_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscription_lifecycle");

    group.bench_function("full_lifecycle", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let subscriber = Address::generate(&env);

            client.initialize(&admin, &token);
            client.subscribe(&subscriber, &1_000_000_i128, &86_400_u64);
            client.renew(&subscriber);
            client.cancel(&subscriber);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_subscribe,
    bench_renew,
    bench_cancel,
    bench_lifecycle,
);
criterion_main!(benches);