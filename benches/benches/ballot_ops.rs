//! Criterion benchmarks for the Ballot contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, vote, finalize) using the Soroban budget API.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use ballot::{BallotContract, BallotContractClient};

/// Deploys the Ballot contract and returns the environment and client.
fn setup() -> (Env, BallotContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BallotContract);
    let client = BallotContractClient::new(&env, &contract_id);
    (env, client)
}

/// Builds a proposal list of the given size.
fn proposals(env: &Env, count: u32) -> Vec<String> {
    let mut list = Vec::new(env);
    for i in 0..count {
        list.push_back(String::from_str(env, &format!("proposal-{i}")));
    }
    list
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("ballot_initialize");

    for count in [1u32, 5, 10] {
        group.bench_function(format!("proposals_{count}"), |b| {
            b.iter(|| {
                let (env, client) = setup();
                let admin = Address::generate(&env);
                let list = proposals(&env, count);
                client.initialize(&admin, &list);
                env.budget().cpu_instruction_cost()
            });
        });
    }

    group.finish();
}

fn bench_vote(c: &mut Criterion) {
    let mut group = c.benchmark_group("ballot_vote");

    group.bench_function("single_vote", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let voter = Address::generate(&env);
            let list = proposals(&env, 5);
            client.initialize(&admin, &list);
            client.vote(&voter, &0u32);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_finalize(c: &mut Criterion) {
    let mut group = c.benchmark_group("ballot_finalize");

    group.bench_function("finalize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let voter = Address::generate(&env);
            let list = proposals(&env, 5);
            client.initialize(&admin, &list);
            client.vote(&voter, &0u32);
            client.finalize();
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_initialize, bench_vote, bench_finalize);
criterion_main!(benches);