//! Criterion benchmarks for the Crowdfund contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, contribute, withdraw, refund) using the Soroban host budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

use crowdfund::{CrowdfundContract, CrowdfundContractClient};

/// Deploys the Crowdfund contract and a test token, returning the environment,
/// the crowdfund client, the token client, and the relevant addresses.
fn setup() -> (
    Env,
    CrowdfundContractClient<'static>,
    token::Client<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let contributor = Address::generate(&env);

    // Deploy a Stellar asset contract to use as the crowdfund token.
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_contract.address();
    let token_client = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    // Fund the contributor so they can pledge.
    token_admin_client.mint(&contributor, &1_000_000);

    let contract_id = env.register_contract(None, CrowdfundContract);
    let client = CrowdfundContractClient::new(&env, &contract_id);

    (env, client, token_client, admin, creator, contributor)
}

/// Benchmarks the `initialize` entry point.
fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("crowdfund_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client, _token, admin, creator, _contributor) = setup();
            let deadline = env.ledger().timestamp() + 86_400;
            client.initialize(&admin, &creator, &1_000_000, &deadline);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `contribute` entry point.
fn bench_contribute(c: &mut Criterion) {
    let mut group = c.benchmark_group("crowdfund_contribute");

    group.bench_function("contribute", |b| {
        b.iter(|| {
            let (env, client, _token, admin, creator, contributor) = setup();
            let deadline = env.ledger().timestamp() + 86_400;
            client.initialize(&admin, &creator, &1_000_000, &deadline);
            client.contribute(&contributor, &1_000);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `withdraw` entry point after a successful campaign.
fn bench_withdraw(c: &mut Criterion) {
    let mut group = c.benchmark_group("crowdfund_withdraw");

    group.bench_function("withdraw", |b| {
        b.iter(|| {
            let (env, client, _token, admin, creator, contributor) = setup();
            let deadline = env.ledger().timestamp() + 86_400;
            client.initialize(&admin, &creator, &1_000_000, &deadline);
            client.contribute(&contributor, &1_000_000);

            // Advance past the deadline so the campaign can be finalized.
            env.ledger().set_timestamp(deadline + 1);
            client.withdraw(&creator);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks the `refund` entry point after a failed campaign.
fn bench_refund(c: &mut Criterion) {
    let mut group = c.benchmark_group("crowdfund_refund");

    group.bench_function("refund", |b| {
        b.iter(|| {
            let (env, client, _token, admin, creator, contributor) = setup();
            let deadline = env.ledger().timestamp() + 86_400;
            client.initialize(&admin, &creator, &1_000_000, &deadline);
            client.contribute(&contributor, &1_000);

            // Advance past the deadline without reaching the goal.
            env.ledger().set_timestamp(deadline + 1);
            client.refund(&contributor);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_contribute,
    bench_withdraw,
    bench_refund
);
criterion_main!(benches);