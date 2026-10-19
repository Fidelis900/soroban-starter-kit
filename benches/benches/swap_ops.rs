//! Criterion benchmarks for the Swap contract.
//!
//! Measures CPU instruction cost and execution time for the primary
//! state-changing entry points of the Swap contract.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_pass_by_value)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String,
};

use soroban_swap_template::SwapContractClient;

/// Deploys the Swap contract and returns the environment, client, and admin.
fn setup_swap() -> (Env, SwapContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, soroban_swap_template::SwapContract);
    let client = SwapContractClient::new(&env, &contract_id);

    (env, client, admin)
}

/// Creates a token contract and mints `amount` to `to`.
fn create_token<'a>(
    env: &'a Env,
    admin: &Address,
    to: &Address,
    amount: i128,
) -> (Address, token::Client<'a>) {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token::Client::new(env, &token_id.address());
    token.mint(to, &amount);
    (token_id.address(), token)
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let token_a = Address::generate(&env);
            let token_b = Address::generate(&env);

            client.initialize(black_box(&admin), black_box(&token_a), black_box(&token_b));

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_add_liquidity(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_add_liquidity");

    group.bench_function("add_liquidity", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let provider = Address::generate(&env);

            let (token_a, _) = create_token(&env, &admin, &provider, 1_000_000_000);
            let (token_b, _) = create_token(&env, &admin, &provider, 1_000_000_000);

            client.initialize(&admin, &token_a, &token_b);

            client.add_liquidity(
                black_box(&provider),
                black_box(&1_000_000_i128),
                black_box(&1_000_000_i128),
            );

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_swap(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_swap");

    group.bench_function("swap", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let provider = Address::generate(&env);
            let trader = Address::generate(&env);

            let (token_a, _) = create_token(&env, &admin, &provider, 1_000_000_000);
            let (token_b, _) = create_token(&env, &admin, &provider, 1_000_000_000);

            client.initialize(&admin, &token_a, &token_b);
            client.add_liquidity(&provider, &1_000_000_i128, &1_000_000_i128);

            create_token(&env, &admin, &trader, 1_000_000);

            client.swap(
                black_box(&trader),
                black_box(&token_a),
                black_box(&1_000_i128),
                black_box(&0_i128),
            );

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_remove_liquidity(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_remove_liquidity");

    group.bench_function("remove_liquidity", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let provider = Address::generate(&env);

            let (token_a, _) = create_token(&env, &admin, &provider, 1_000_000_000);
            let (token_b, _) = create_token(&env, &admin, &provider, 1_000_000_000);

            client.initialize(&admin, &token_a, &token_b);
            client.add_liquidity(&provider, &1_000_000_i128, &1_000_000_i128);

            client.remove_liquidity(
                black_box(&provider),
                black_box(&500_000_i128),
                black_box(&0_i128),
                black_box(&0_i128),
            );

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_get_reserves(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_get_reserves");

    group.bench_function("get_reserves", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let provider = Address::generate(&env);

            let (token_a, _) = create_token(&env, &admin, &provider, 1_000_000_000);
            let (token_b, _) = create_token(&env, &admin, &provider, 1_000_000_000);

            client.initialize(&admin, &token_a, &token_b);
            client.add_liquidity(&provider, &1_000_000_i128, &1_000_000_i128);

            black_box(client.get_reserves());
            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_get_price(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_get_price");

    group.bench_function("get_price", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let provider = Address::generate(&env);

            let (token_a, _) = create_token(&env, &admin, &provider, 1_000_000_000);
            let (token_b, _) = create_token(&env, &admin, &provider, 1_000_000_000);

            client.initialize(&admin, &token_a, &token_b);
            client.add_liquidity(&provider, &1_000_000_i128, &1_000_000_i128);

            black_box(client.get_price(black_box(&token_a)));
            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

/// Proposes an open swap with partial fills enabled and returns its id.
fn propose_partial_swap(
    env: &Env,
    client: &SwapContractClient<'_>,
    creator: &Address,
    token_offered: &Address,
    token_requested: &Address,
    offer_amount: i128,
    request_amount: i128,
) -> u32 {
    client.propose_swap(
        creator,
        token_offered,
        token_requested,
        &offer_amount,
        &request_amount,
        &true,  // allow_partial_fills
        &None,  // hash_lock
        &0_u64, // deadline
    )
}

fn bench_fill_swap_partial(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_fill_swap_partial");

    group.bench_function("fill_swap_partial", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let creator = Address::generate(&env);
            let taker = Address::generate(&env);

            let (token_offered, _) = create_token(&env, &admin, &creator, 1_000_000_000);
            let (token_requested, _) = create_token(&env, &admin, &taker, 1_000_000_000);

            client.initialize(&admin, &token_offered, &token_requested);

            let swap_id = propose_partial_swap(
                &env,
                &client,
                &creator,
                &token_offered,
                &token_requested,
                1_000_000_i128,
                1_000_000_i128,
            );

            client.fill_swap_partial(
                black_box(&taker),
                black_box(&swap_id),
                black_box(&250_000_i128),
            );

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_cancel_swaps_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_cancel_swaps_batch");

    group.bench_function("cancel_swaps_batch", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let creator = Address::generate(&env);

            let (token_offered, _) = create_token(&env, &admin, &creator, 1_000_000_000);
            let (token_requested, _) = create_token(&env, &admin, &creator, 1_000_000_000);

            client.initialize(&admin, &token_offered, &token_requested);

            let mut swap_ids = soroban_sdk::Vec::new(&env);
            for _ in 0..5 {
                let swap_id = propose_partial_swap(
                    &env,
                    &client,
                    &creator,
                    &token_offered,
                    &token_requested,
                    100_000_i128,
                    100_000_i128,
                );
                swap_ids.push_back(swap_id);
            }

            client.cancel_swaps_batch(black_box(&creator), black_box(&swap_ids));

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

fn bench_execute_swap_with_referrer(c: &mut Criterion) {
    let mut group = c.benchmark_group("swap_execute_swap_with_referrer");

    group.bench_function("execute_swap_with_referrer", |b| {
        b.iter(|| {
            let (env, client, admin) = setup_swap();
            let creator = Address::generate(&env);
            let taker = Address::generate(&env);
            let referrer = Address::generate(&env);

            let (token_offered, _) = create_token(&env, &admin, &creator, 1_000_000_000);
            let (token_requested, _) = create_token(&env, &admin, &taker, 1_000_000_000);

            client.initialize(&admin, &token_offered, &token_requested);

            let swap_id = propose_partial_swap(
                &env,
                &client,
                &creator,
                &token_offered,
                &token_requested,
                1_000_000_i128,
                1_000_000_i128,
            );

            client.execute_swap(
                black_box(&taker),
                black_box(&swap_id),
                black_box(&Some(referrer.clone())),
            );

            black_box(env.budget().cpu_instruction_cost());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_add_liquidity,
    bench_swap,
    bench_remove_liquidity,
    bench_get_reserves,
    bench_get_price,
    bench_fill_swap_partial,
    bench_cancel_swaps_batch,
    bench_execute_swap_with_referrer,
);
criterion_main!(benches);