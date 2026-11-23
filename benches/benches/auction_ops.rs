//! Criterion benchmarks for the Auction contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, create_auction, bid, settle, cancel) using the Soroban budget
//! exposed through `env.budget().cpu_instruction_cost()`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String,
};

use auction_contract::{AuctionContract, AuctionContractClient};

/// Deploys the Auction contract and returns the environment, client and admin.
fn setup() -> (Env, AuctionContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AuctionContract);
    let client = AuctionContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    (env, client, admin)
}

/// Creates a funded token contract and returns its address plus a client.
fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, token::Client<'a>) {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = token_id.address();
    let token_client = token::Client::new(env, &token_address);
    (token_address, token_client)
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("auction_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_create_auction(c: &mut Criterion) {
    let mut group = c.benchmark_group("auction_create");

    group.bench_function("create_auction", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            let seller = Address::generate(&env);
            let title = String::from_str(&env, "Rare collectible");
            let start_price: i128 = 1_000;
            let duration: u64 = 3_600;

            client.create_auction(&seller, &title, &start_price, &duration);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_bid(c: &mut Criterion) {
    let mut group = c.benchmark_group("auction_bid");

    group.bench_function("bid", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            let seller = Address::generate(&env);
            let bidder = Address::generate(&env);
            let title = String::from_str(&env, "Rare collectible");
            let start_price: i128 = 1_000;
            let duration: u64 = 3_600;

            let auction_id = client.create_auction(&seller, &title, &start_price, &duration);

            // Fund the bidder so the bid transfer succeeds.
            token_client.mint(&bidder, &1_000_000);

            let bid_amount: i128 = 5_000;
            client.bid(&bidder, &auction_id, &bid_amount);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_settle(c: &mut Criterion) {
    let mut group = c.benchmark_group("auction_settle");

    group.bench_function("settle", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, token_client) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            let seller = Address::generate(&env);
            let bidder = Address::generate(&env);
            let title = String::from_str(&env, "Rare collectible");
            let start_price: i128 = 1_000;
            let duration: u64 = 3_600;

            let auction_id = client.create_auction(&seller, &title, &start_price, &duration);

            token_client.mint(&bidder, &1_000_000);
            client.bid(&bidder, &auction_id, &5_000);

            // Advance the ledger past the auction end so it can be settled.
            env.ledger().with_mut(|l| {
                l.timestamp += duration + 1;
            });

            client.settle(&auction_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("auction_cancel");

    group.bench_function("cancel", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            let (token_address, _) = create_token(&env, &admin);

            client.initialize(&admin, &token_address);

            let seller = Address::generate(&env);
            let title = String::from_str(&env, "Rare collectible");
            let start_price: i128 = 1_000;
            let duration: u64 = 3_600;

            let auction_id = client.create_auction(&seller, &title, &start_price, &duration);

            client.cancel(&seller, &auction_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_create_auction,
    bench_bid,
    bench_settle,
    bench_cancel
);
criterion_main!(benches);