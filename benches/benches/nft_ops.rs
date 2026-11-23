//! Criterion benchmarks for the NFT contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, mint, transfer, burn, approve) using the Soroban budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, String};
use nft_contract::{NftContract, NftContractClient};

/// Deploys the NFT contract and returns the environment plus a client.
fn setup() -> (Env, NftContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &contract_id);
    (env, client)
}

/// Initializes the contract with a name and symbol.
fn init_contract(env: &Env, client: &NftContractClient) {
    let admin = Address::generate(env);
    let name = String::from_str(env, "Benchmark NFT");
    let symbol = String::from_str(env, "BNFT");
    client.initialize(&admin, &name, &symbol);
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_initialize");
    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let name = String::from_str(&env, "Benchmark NFT");
            let symbol = String::from_str(&env, "BNFT");
            client.initialize(&admin, &name, &symbol);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

fn bench_mint(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_mint");
    group.bench_function("mint", |b| {
        b.iter(|| {
            let (env, client) = setup();
            init_contract(&env, &client);
            let to = Address::generate(&env);
            let token_id = 1u32;
            let uri = String::from_str(&env, "ipfs://benchmark/1");
            client.mint(&to, &token_id, &uri);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

fn bench_mint_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_mint_batch");
    group.bench_function("mint_batch", |b| {
        b.iter(|| {
            let (env, client) = setup();
            init_contract(&env, &client);
            let to = Address::generate(&env);
            let count = 10u32;
            let mut uris = soroban_sdk::Vec::new(&env);
            for i in 0..count {
                uris.push_back(String::from_str(&env, "ipfs://benchmark/batch"));
                let _ = i;
            }
            client.mint_batch(&to, &count, &uris);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

fn bench_transfer(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_transfer");
    group.bench_function("transfer", |b| {
        b.iter(|| {
            let (env, client) = setup();
            init_contract(&env, &client);
            let owner = Address::generate(&env);
            let recipient = Address::generate(&env);
            let token_id = 1u32;
            let uri = String::from_str(&env, "ipfs://benchmark/1");
            client.mint(&owner, &token_id, &uri);
            client.transfer(&owner, &recipient, &token_id);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

fn bench_burn(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_burn");
    group.bench_function("burn", |b| {
        b.iter(|| {
            let (env, client) = setup();
            init_contract(&env, &client);
            let owner = Address::generate(&env);
            let token_id = 1u32;
            let uri = String::from_str(&env, "ipfs://benchmark/1");
            client.mint(&owner, &token_id, &uri);
            client.burn(&owner, &token_id);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

fn bench_approve(c: &mut Criterion) {
    let mut group = c.benchmark_group("nft_approve");
    group.bench_function("approve", |b| {
        b.iter(|| {
            let (env, client) = setup();
            init_contract(&env, &client);
            let owner = Address::generate(&env);
            let spender = Address::generate(&env);
            let token_id = 1u32;
            let uri = String::from_str(&env, "ipfs://benchmark/1");
            client.mint(&owner, &token_id, &uri);
            client.approve(&owner, &spender, &token_id);
            env.budget().cpu_instruction_cost()
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_mint,
    bench_mint_batch,
    bench_transfer,
    bench_burn,
    bench_approve,
);
criterion_main!(benches);