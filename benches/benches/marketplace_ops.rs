//! Criterion benchmarks for the Marketplace contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, list, buy, cancel_listing) using the Soroban budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_range_loop)]
#![allow(dead_code)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String,
};

use marketplace::{MarketplaceContract, MarketplaceContractClient};

/// Deploys the Marketplace contract and returns the environment and client.
fn setup() -> (Env, MarketplaceContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MarketplaceContract);
    let client = MarketplaceContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client, admin)
}

/// Creates a funded token contract and returns its address and client.
fn setup_token(env: &Env, admin: &Address) -> (Address, token::Client<'static>) {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = token_id.address();
    let token_client = token::Client::new(env, &token_address);
    (token_address, token_client)
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();

            let contract_id = env.register_contract(None, MarketplaceContract);
            let client = MarketplaceContractClient::new(&env, &contract_id);

            let admin = Address::generate(&env);
            client.initialize(&admin);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_list");

    group.bench_function("list", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = setup_token(&env, &admin);

            let seller = Address::generate(&env);
            token_client.mint(&seller, &1_000_000);

            let listing_id = client.list(
                &seller,
                &token_address,
                &100_000,
                &String::from_str(&env, "Test Listing"),
            );

            let _ = listing_id;
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_buy(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_buy");

    group.bench_function("buy", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = setup_token(&env, &admin);

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            token_client.mint(&seller, &1_000_000);
            token_client.mint(&buyer, &1_000_000);

            let listing_id = client.list(
                &seller,
                &token_address,
                &100_000,
                &String::from_str(&env, "Test Listing"),
            );

            client.buy(&buyer, &listing_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_make_offers_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_make_offers_batch");

    group.bench_function("make_offers_batch", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = setup_token(&env, &admin);

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            token_client.mint(&seller, &1_000_000);
            token_client.mint(&buyer, &1_000_000);

            let mut token_ids = soroban_sdk::Vec::new(&env);
            let mut prices = soroban_sdk::Vec::new(&env);
            for i in 0..10u32 {
                let listing_id = client.list(
                    &seller,
                    &token_address,
                    &100_000,
                    &String::from_str(&env, "Test Listing"),
                );
                token_ids.push_back(listing_id);
                prices.push_back(50_000i128 + i as i128);
            }

            let expiration = env.ledger().sequence() + 1_000;
            client.make_offers_batch(&buyer, &token_ids, &prices, &expiration);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_expire_offer(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_expire_offer");

    group.bench_function("expire_offer", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = setup_token(&env, &admin);

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            token_client.mint(&seller, &1_000_000);
            token_client.mint(&buyer, &1_000_000);

            let listing_id = client.list(
                &seller,
                &token_address,
                &100_000,
                &String::from_str(&env, "Test Listing"),
            );

            let expiration = env.ledger().sequence() + 10;
            let offer_id = client.make_offer(&buyer, &listing_id, &50_000, &expiration);

            env.ledger().set_sequence_number(expiration + 1);
            client.expire_offer(&offer_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_cancel_listing(c: &mut Criterion) {
    let mut group = c.benchmark_group("marketplace_cancel_listing");

    group.bench_function("cancel_listing", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = setup_token(&env, &admin);

            let seller = Address::generate(&env);
            token_client.mint(&seller, &1_000_000);

            let listing_id = client.list(
                &seller,
                &token_address,
                &100_000,
                &String::from_str(&env, "Test Listing"),
            );

            client.cancel_listing(&seller, &listing_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_list,
    bench_buy,
    bench_make_offers_batch,
    bench_expire_offer,
    bench_cancel_listing
);
criterion_main!(benches);
</final>