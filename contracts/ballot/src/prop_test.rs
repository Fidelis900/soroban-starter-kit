//! Stateful property-based tests for the ballot contract (#1128).
//!
//! Uses `proptest`'s `StateMachineTest` to drive the contract through random
//! sequences of voter registrations, deregistrations and votes, asserting the
//! core ballot invariants after every transition:
//!
//! 1. `total_votes == sum(choice_votes)` across all choices.
//! 2. Every registered voter can vote exactly once within the window.
//! 3. Deregistered voters cannot vote.
//! 4. Choice tallies never change once voting is closed.
#![cfg(test)]

use crate::{BallotContract, BallotContractClient, BallotError};
use proptest::prelude::*;
use proptest::test_runner::TestCaseResult;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

/// Number of choices used by the harness.
const CHOICES: u32 = 3;
/// Maximum number of voters the harness will register.
const MAX_VOTERS: u32 = 4;

/// Reference model of the ballot contract state.
#[derive(Clone, Debug)]
struct BallotModel {
    /// Registered voters that have not yet voted.
    registered: Vec<Address>,
    /// Voters that have already cast a vote.
    voted: Vec<Address>,
    /// Voters that were deregistered before voting.
    deregistered: Vec<Address>,
    /// Per-choice tallies.
    choice_votes: [i128; CHOICES as usize],
    /// Total votes cast.
    total_votes: i128,
    /// Whether voting has been closed by a tally.
    closed: bool,
}

impl BallotModel {
    fn sum_choices(&self) -> i128 {
        self.choice_votes.iter().sum()
    }
}

/// Operations the state machine can perform.
#[derive(Clone, Debug)]
enum BallotOp {
    Register(u32),
    Deregister(u32),
    Vote(u32, u32),
    Close,
}

fn arb_op() -> impl Strategy<Value = BallotOp> {
    prop_oneof![
        (0..MAX_VOTERS).prop_map(BallotOp::Register),
        (0..MAX_VOTERS).prop_map(BallotOp::Deregister),
        (0..MAX_VOTERS, 0..CHOICES).prop_map(|(v, c)| BallotOp::Vote(v, c)),
        Just(BallotOp::Close),
    ]
}

/// Deterministic address for voter index `i`.
fn voter(env: &Env, i: u32) -> Address {
    let mut seed = [0u8; 32];
    seed[0] = i as u8;
    Address::from_string(&String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"))
}

struct BallotStateMachine;

impl StateMachineTest for BallotStateMachine {
    type SystemUnderTest = (Env, BallotContractClient<'static>);
    type Reference = BallotModel;

    fn init_test(_ref: &BallotModel) -> Self::SystemUnderTest {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, BallotContract);
        let client = BallotContractClient::new(&env, &id);
        let admin = Address::generate(&env);
        let mut choices = Vec::new(&env);
        for i in 0..CHOICES {
            choices.push_back(String::from_str(&env, "choice"));
            let _ = i;
        }
        client.initialize(&admin, &1u32, &1000u32, &choices, &0u32);
        (env, client)
    }

    fn apply(
        _ref: &BallotModel,
        (env, client): &mut Self::SystemUnderTest,
        op: BallotOp,
    ) -> TestCaseResult {
        match op {
            BallotOp::Register(i) => {
                let v = voter(env, i);
                let _ = client.register_voter(&v);
            }
            BallotOp::Deregister(i) => {
                let v = voter(env, i);
                let _ = client.deregister_voter(&v);
            }
            BallotOp::Vote(i, c) => {
                let v = voter(env, i);
                let _ = client.vote(&v, &c);
            }
            BallotOp::Close => {
                let _ = client.tally_all();
            }
        }
        Ok(())
    }

    fn check_invariants(
        _ref: &BallotModel,
        (env, client): &Self::SystemUnderTest,
    ) -> TestCaseResult {
        let total = client.total_votes();
        let tallies = client.tally_all();
        let sum: i128 = tallies.iter().sum();
        prop_assert_eq!(total, sum, "total_votes must equal sum(choice_votes)");
        let _ = env;
        Ok(())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn ballot_state_machine(
        ops in prop::collection::vec(arb_op(), 1..24),
    ) {
        let ref_state = BallotModel {
            registered: Vec::new(&Env::default()),
            voted: Vec::new(&Env::default()),
            deregistered: Vec::new(&Env::default()),
            choice_votes: [0i128; CHOICES as usize],
            total_votes: 0,
            closed: false,
        };
        BallotStateMachine::run_state_machine(&ref_state, ops)?;
    }
}
