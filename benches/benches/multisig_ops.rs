//! Criterion benchmarks for the Multisig contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points:
//! `initialize`, `submit_transaction`, `confirm_transaction`,
//! `execute_transaction`, and `execute_batch` (compared against executing the
//! same number of proposals individually).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

use multisig_contract::{MultisigContract, MultisigContractClient};

/// Number of owners used across the benchmark scenarios.
const OWNER_COUNT: u32 = 3;

/// Threshold required to execute a transaction.
const THRESHOLD: u32 = 2;

/// Builds a fresh environment with the Multisig contract deployed and
/// initialized, returning the environment, the client, and the owner list.
fn setup() -> (Env, MultisigContractClient<'static>, Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(&env, &contract_id);

    let mut owners = Vec::new(&env);
    for _ in 0..OWNER_COUNT {
        owners.push_back(Address::generate(&env));
    }

    client.initialize(&owners, &THRESHOLD);

    (env, client, owners)
}

/// Benchmarks `initialize` on a freshly deployed contract.
fn bench_initialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("multisig_initialize");

    group.bench_function("initialize", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();

            let contract_id = env.register_contract(None, MultisigContract);
            let client = MultisigContractClient::new(&env, &contract_id);

            let mut owners = Vec::new(&env);
            for _ in 0..OWNER_COUNT {
                owners.push_back(Address::generate(&env));
            }

            client.initialize(&owners, &THRESHOLD);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks `submit_transaction`.
fn bench_submit_transaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("multisig_submit_transaction");

    group.bench_function("submit_transaction", |b| {
        b.iter(|| {
            let (env, client, owners) = setup();

            let target = Address::generate(&env);
            let payload = BytesN::from_array(&env, &[0u8; 32]);

            client.submit_transaction(&owners.get(0).unwrap(), &target, &payload);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks `confirm_transaction`.
fn bench_confirm_transaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("multisig_confirm_transaction");

    group.bench_function("confirm_transaction", |b| {
        b.iter(|| {
            let (env, client, owners) = setup();

            let target = Address::generate(&env);
            let payload = BytesN::from_array(&env, &[0u8; 32]);

            let tx_id = client.submit_transaction(&owners.get(0).unwrap(), &target, &payload);

            client.confirm_transaction(&owners.get(1).unwrap(), &tx_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Number of proposals executed in the batch benchmark scenarios.
const BATCH_SIZE: u32 = 3;

/// Submits and confirms `count` transactions, returning their ids.
fn prepare_approved_transactions(
    env: &Env,
    client: &MultisigContractClient,
    owners: &Vec<Address>,
    count: u32,
) -> Vec<u64> {
    let mut tx_ids = Vec::new(env);
    for _ in 0..count {
        let target = Address::generate(env);
        let payload = BytesN::from_array(env, &[0u8; 32]);
        let tx_id = client.submit_transaction(&owners.get(0).unwrap(), &target, &payload);
        client.confirm_transaction(&owners.get(1).unwrap(), &tx_id);
        tx_ids.push_back(tx_id);
    }
    tx_ids
}

/// Benchmarks `execute_batch` against executing the same number of proposals
/// individually, to quantify the gas savings from batched execution.
fn bench_execute_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("multisig_execute_batch");

    group.bench_function("execute_batch", |b| {
        b.iter(|| {
            let (env, client, owners) = setup();
            let tx_ids = prepare_approved_transactions(&env, &client, &owners, BATCH_SIZE);

            client.execute_batch(&owners.get(0).unwrap(), &tx_ids);

            env.budget().cpu_instruction_cost()
        });
    });

    group.bench_function("execute_individually", |b| {
        b.iter(|| {
            let (env, client, owners) = setup();
            let tx_ids = prepare_approved_transactions(&env, &client, &owners, BATCH_SIZE);

            for tx_id in tx_ids.iter() {
                client.execute_transaction(&owners.get(0).unwrap(), &tx_id);
            }

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

/// Benchmarks `execute_transaction` once the threshold has been reached.
fn bench_execute_transaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("multisig_execute_transaction");

    group.bench_function("execute_transaction", |b| {
        b.iter(|| {
            let (env, client, owners) = setup();

            let target = Address::generate(&env);
            let payload = BytesN::from_array(&env, &[0u8; 32]);

            let tx_id = client.submit_transaction(&owners.get(0).unwrap(), &target, &payload);
            client.confirm_transaction(&owners.get(1).unwrap(), &tx_id);

            client.execute_transaction(&owners.get(0).unwrap(), &tx_id);

            env.budget().cpu_instruction_cost()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_initialize,
    bench_submit_transaction,
    bench_confirm_transaction,
    bench_execute_transaction,
    bench_execute_batch,
);
criterion_main!(benches);