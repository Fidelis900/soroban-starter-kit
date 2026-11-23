//! Criterion benchmarks for the Airdrop contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, fund, claim) using the Soroban host budget.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Vec,
};

use airdrop_contract::{AirdropContract, AirdropContractClient};

/// Deploys the Airdrop contract and returns the environment, client, and admin.
fn setup() -> (Env, AirdropContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    (env, client, admin)
}

/// Creates a funded SAC token and returns its address plus a client.
fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, token::Client<'a>) {
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_id.address();
    let token_client = token::Client::new(env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(env, &token_address);
    token_admin_client.mint(admin, &1_000_000_000);
    (token_address, token_client)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("airdrop_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            let start = env.budget().cpu_instruction_cost();
            client.initialize(&admin, &token_address);
            let end = env.budget().cpu_instruction_cost();

            let _ = end - start;
        });
    });

    group.finish();
}

/// Benchmarks the `fund` entry point.
fn bench_fund(c: &mut Criterion) {
    let mut group = c.benchmark_group("airdrop_fund");

    group.bench_function("fund", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            let start = env.budget().cpu_instruction_cost();
            client.fund(&admin, &1_000_000);
            let end = env.budget().cpu_instruction_cost();

            let _ = end - start;
        });
    });

    group.finish();
}

/// Benchmarks the `claim` entry point.
fn bench_claim(c: &mut Criterion) {
    let mut group = c.benchmark_group("airdrop_claim");

    group.bench_function("claim", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);
            client.fund(&admin, &1_000_000);

            let recipient = Address::generate(&env);

            let start = env.budget().cpu_instruction_cost();
            client.claim(&recipient);
            let end = env.budget().cpu_instruction_cost();

            let _ = end - start;
        });
    });

    group.finish();
}

/// Benchmarks a batch of `claim` calls to measure amortized cost.
fn bench_claim_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("airdrop_claim_batch");

    group.bench_function("claim_batch_10", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);
            client.fund(&admin, &1_000_000);

            let mut recipients = Vec::new(&env);
            for _ in 0..10 {
                recipients.push_back(Address::generate(&env));
            }

            let start = env.budget().cpu_instruction_cost();
            for recipient in recipients.iter() {
                client.claim(&recipient);
            }
            let end = env.budget().cpu_instruction_cost();

            let _ = end - start;
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_fund,
    bench_claim,
    bench_claim_batch
);
criterion_main!(benches);