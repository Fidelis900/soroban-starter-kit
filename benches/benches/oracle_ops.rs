#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

use oracle_contract::{OracleContract, OracleContractClient};

fn bench_oracle_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("oracle_ops");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register_contract(None, OracleContract);
            let client = OracleContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            client.initialize(&admin);
            env.budget().cpu_instruction_cost()
        });
    });

    group.bench_function("set_price", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register_contract(None, OracleContract);
            let client = OracleContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            client.initialize(&admin);
            let asset = Symbol::new(&env, "XLM");
            client.set_price(&admin, &asset, &1_000_000);
            env.budget().cpu_instruction_cost()
        });
    });

    group.bench_function("update_price", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register_contract(None, OracleContract);
            let client = OracleContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            client.initialize(&admin);
            let asset = Symbol::new(&env, "XLM");
            client.set_price(&admin, &asset, &1_000_000);
            client.update_price(&admin, &asset, &2_000_000);
            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_oracle_ops);
criterion_main!(benches);