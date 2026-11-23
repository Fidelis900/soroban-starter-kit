//! Criterion benchmarks for the BondingCurve contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, buy, sell) using the Soroban host budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env};

use bonding_curve::{BondingCurveContract, BondingCurveContractClient};

/// Deploys a fresh BondingCurve contract and returns the environment and client.
fn setup() -> (Env, BondingCurveContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BondingCurveContract);
    let client = BondingCurveContractClient::new(&env, &contract_id);

    (env, client)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("bonding_curve_initialize");

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

/// Benchmarks the `buy` entry point.
fn bench_buy(c: &mut Criterion) {
    let mut group = c.benchmark_group("bonding_curve_buy");

    group.bench_function("buy", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let buyer = Address::generate(&env);

            client.initialize(&admin, &token);
            client.buy(&buyer, &1_000_i128);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `sell` entry point.
fn bench_sell(c: &mut Criterion) {
    let mut group = c.benchmark_group("bonding_curve_sell");

    group.bench_function("sell", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let trader = Address::generate(&env);

            client.initialize(&admin, &token);
            client.buy(&trader, &1_000_i128);
            client.sell(&trader, &500_i128);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_initialize, bench_buy, bench_sell);
criterion_main!(benches);