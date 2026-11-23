//! Criterion benchmarks for the DAO contract.
//!
//! Measures CPU instruction cost for the primary state-changing entry points
//! (initialize, create_proposal, vote, execute) using the Soroban budget API.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::too_many_arguments)]

use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Symbol, Vec};

use dao_contract::{DaoContract, DaoContractClient, VotingType};

/// Deploys the DAO contract and returns the environment, client, and admin.
fn setup() -> (Env, DaoContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    (env, client, admin)
}

/// Initializes the DAO with a default voting period and quorum.
fn setup_initialized() -> (Env, DaoContractClient<'static>, Address) {
    let (env, client, admin) = setup();
    client.initialize(&admin, &100u32, &1u32);
    (env, client, admin)
}

fn bench_initialize(c: &mut Criterion) {
    c.bench_function("dao_initialize", |b| {
        b.iter(|| {
            let (env, client, admin) = setup();
            client.initialize(&admin, &100u32, &1u32);
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_create_proposal(c: &mut Criterion) {
    c.bench_function("dao_create_proposal", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let choices: Vec<Symbol> = Vec::new(&env);
            client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_vote(c: &mut Criterion) {
    c.bench_function("dao_vote", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let choices: Vec<Symbol> = Vec::new(&env);
            let proposal_id = client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );

            let voter = Address::generate(&env);
            client.vote(&voter, &proposal_id, &true);
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_vote_quadratic(c: &mut Criterion) {
    c.bench_function("dao_vote_quadratic", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let choices: Vec<Symbol> = Vec::new(&env);
            let proposal_id = client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Quadratic,
                &choices,
            );

            let voter = Address::generate(&env);
            client.vote(&voter, &proposal_id, &true);
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_vote_choice(c: &mut Criterion) {
    c.bench_function("dao_vote_choice", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let mut choices: Vec<Symbol> = Vec::new(&env);
            choices.push_back(Symbol::new(&env, "OptionA"));
            choices.push_back(Symbol::new(&env, "OptionB"));
            choices.push_back(Symbol::new(&env, "OptionC"));
            let proposal_id = client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );

            let voter = Address::generate(&env);
            client.vote_choice(&voter, &proposal_id, &Symbol::new(&env, "OptionB"));
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_execute(c: &mut Criterion) {
    c.bench_function("dao_execute", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let choices: Vec<Symbol> = Vec::new(&env);
            let proposal_id = client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );

            let voter = Address::generate(&env);
            client.vote(&voter, &proposal_id, &true);

            client.execute(&proposal_id);
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_create_proposal_batch(c: &mut Criterion) {
    c.bench_function("dao_create_proposal_batch_10", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let mut ids: Vec<u32> = Vec::new(&env);
            for i in 0..10u32 {
                let title = String::from_str(&env, "Proposal Title");
                let description = String::from_str(&env, "Proposal description text");
                let choices: Vec<Symbol> = Vec::new(&env);
                let id = client.create_proposal(
                    &proposer,
                    &title,
                    &description,
                    &VotingType::Linear,
                    &choices,
                );
                ids.push_back(id);
            }
            env.budget().cpu_instruction_cost()
        });
    });
}

fn bench_create_proposal_duplicate(c: &mut Criterion) {
    c.bench_function("dao_create_proposal_duplicate", |b| {
        b.iter(|| {
            let (env, client, _admin) = setup_initialized();
            let proposer = Address::generate(&env);
            let title = String::from_str(&env, "Proposal Title");
            let description = String::from_str(&env, "Proposal description text");
            let choices: Vec<Symbol> = Vec::new(&env);
            client.create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );
            // Duplicate payload: expected to be rejected with DuplicateProposal.
            let _ = client.try_create_proposal(
                &proposer,
                &title,
                &description,
                &VotingType::Linear,
                &choices,
            );
            env.budget().cpu_instruction_cost()
        });
    });
}

criterion_group!(
    benches,
    bench_initialize,
    bench_create_proposal,
    bench_vote,
    bench_vote_quadratic,
    bench_vote_choice,
    bench_execute,
    bench_create_proposal_batch,
    bench_create_proposal_duplicate,
);
criterion_main!(benches);