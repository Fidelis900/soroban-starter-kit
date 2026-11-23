//! Criterion benchmarks for the Lottery contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, buy_ticket, draw_winner, claim_prize) using the Soroban
//! host budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use lottery_contract::{LotteryContract, LotteryContractClient};

/// Deploys the Lottery contract and returns the environment plus a client.
fn setup() -> (Env, LotteryContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(&env, &contract_id);
    (env, client)
}

/// Initializes the lottery with a fixed ticket price and a set of participants.
fn init_lottery(env: &Env, client: &LotteryContractClient) -> (Address, Vec<Address>) {
    let admin = Address::generate(env);
    let token = Address::generate(env);
    let ticket_price: i128 = 100;
    let max_tickets: u32 = 10;

    client.initialize(&admin, &token, &ticket_price, &max_tickets);

    let mut buyers: Vec<Address> = Vec::new(env);
    for _ in 0..max_tickets {
        buyers.push_back(Address::generate(env));
    }

    (admin, buyers)
}

fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("lottery_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let (env, client) = setup();
            let admin = Address::generate(&env);
            let token = Address::generate(&env);
            let ticket_price: i128 = 100;
            let max_tickets: u32 = 10;

            client.initialize(&admin, &token, &ticket_price, &max_tickets);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

fn bench_buy_ticket(c: &mut Criterion) {
    let mut group = c.benchmark_group("lottery_buy_ticket");

    group.bench_function("buy_ticket", |b| {
        b.iter_batched(
            || {
                let (env, client) = setup();
                let (_admin, buyers) = init_lottery(&env, &client);
                (env, client, buyers)
            },
            |(env, client, buyers)| {
                let buyer = buyers.get(0).unwrap();
                client.buy_ticket(&buyer);
                env.budget().cpu_instruction_cost()
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_draw_winner(c: &mut Criterion) {
    let mut group = c.benchmark_group("lottery_draw_winner");

    group.bench_function("draw_winner", |b| {
        b.iter_batched(
            || {
                let (env, client) = setup();
                let (admin, buyers) = init_lottery(&env, &client);
                for buyer in buyers.iter() {
                    client.buy_ticket(&buyer);
                }
                (env, client, admin)
            },
            |(env, client, admin)| {
                client.draw_winner(&admin);
                env.budget().cpu_instruction_cost()
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_claim_prize(c: &mut Criterion) {
    let mut group = c.benchmark_group("lottery_claim_prize");

    group.bench_function("claim_prize", |b| {
        b.iter_batched(
            || {
                let (env, client) = setup();
                let (admin, buyers) = init_lottery(&env, &client);
                for buyer in buyers.iter() {
                    client.buy_ticket(&buyer);
                }
                client.draw_winner(&admin);
                let winner = client.get_winner();
                (env, client, winner)
            },
            |(env, client, winner)| {
                client.claim_prize(&winner);
                env.budget().cpu_instruction_cost()
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_buy_ticket,
    bench_draw_winner,
    bench_claim_prize
);
criterion_main!(benches);