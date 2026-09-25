//! Stateful fuzz / property-test harness for the DAO contract (issue #1109).
//!
//! # Invariants tested
//!
//! 1. `total_votes == yes_votes + no_votes` always holds for any proposal.
//! 2. An address can never cast a second vote on the same proposal under any
//!    vote-sequence ordering.
//! 3. A proposal cannot execute without satisfying both absolute quorum AND a
//!    yes-majority (yes > no).
//! 4. The adaptive-quorum EMA stays within `[min_quorum_bps, max_quorum_bps]`
//!    after any sequence of proposal outcomes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::as_conversions
)]
#![cfg(test)]

extern crate std;

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

use crate::{DaoContract, DaoContractClient, ProposalState};

// ---------------------------------------------------------------------------
// Shared setup helpers
// ---------------------------------------------------------------------------

/// A lightweight test environment carrying the contracts and token address.
struct TestEnv<'a> {
    env: &'a Env,
    client: DaoContractClient<'a>,
    token: Address,
    admin: Address,
}

fn build_env(env: &Env, quorum: i128, min_bps: u32, max_bps: u32) -> TestEnv<'_> {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();

    let addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(env, &addr);
    client.initialize(&admin, &token, &100, &quorum, &0, &min_bps, &max_bps);

    TestEnv { env, client, token, admin }
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn make_proposal(te: &TestEnv<'_>, proposer: &Address) -> u32 {
    te.client.create_proposal(
        proposer,
        &String::from_str(te.env, "P"),
        &String::from_str(te.env, "D"),
        &None,
        &None,
        &None,
    )
}

// ---------------------------------------------------------------------------
// Proptest: yes_votes + no_votes == total_votes
// ---------------------------------------------------------------------------

proptest! {
    /// For any split of N voters into yes/no groups the tally invariant holds.
    #[test]
    fn prop_vote_tally_invariant(
        yes_weights in proptest::collection::vec(1_i128..=1_000, 1..=8),
        no_weights  in proptest::collection::vec(1_i128..=1_000, 1..=8),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0); // quorum=1 so any single vote qualifies

        // Proposer needs tokens.
        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);
        let id = make_proposal(&te, &proposer);

        let mut expected_yes = 0_i128;
        let mut expected_no  = 0_i128;

        for w in &yes_weights {
            let voter = Address::generate(&env);
            mint(&env, &te.token, &voter, *w);
            te.client.vote(&voter, &id, &true);
            expected_yes += w;
        }
        for w in &no_weights {
            let voter = Address::generate(&env);
            mint(&env, &te.token, &voter, *w);
            te.client.vote(&voter, &id, &false);
            expected_no += w;
        }

        let p = te.client.get_proposal(&id);
        prop_assert_eq!(p.yes_votes, expected_yes, "yes_votes mismatch");
        prop_assert_eq!(p.no_votes,  expected_no,  "no_votes mismatch");
        prop_assert_eq!(
            p.yes_votes + p.no_votes,
            expected_yes + expected_no,
            "total_votes invariant violated"
        );
    }

    /// Double-vote is always rejected regardless of the vote value.
    #[test]
    fn prop_no_double_vote(support_first in any::<bool>(), support_second in any::<bool>()) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);
        let id = make_proposal(&te, &proposer);

        let voter = Address::generate(&env);
        mint(&env, &te.token, &voter, 100);

        te.client.vote(&voter, &id, &support_first);

        // Second vote must always fail with AlreadyVoted (#7).
        let result = te.client.try_vote(&voter, &id, &support_second);
        prop_assert!(result.is_err(), "second vote should be rejected");
    }

    /// A proposal can only execute when yes > no AND total >= quorum.
    #[test]
    fn prop_execution_requires_quorum_and_majority(
        yes_votes in 0_i128..=2_000,
        no_votes  in 0_i128..=2_000,
        quorum    in 1_i128..=1_000,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, quorum, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);
        let id = make_proposal(&te, &proposer);

        if yes_votes > 0 {
            let y_voter = Address::generate(&env);
            mint(&env, &te.token, &y_voter, yes_votes);
            te.client.vote(&y_voter, &id, &true);
        }
        if no_votes > 0 {
            let n_voter = Address::generate(&env);
            mint(&env, &te.token, &n_voter, no_votes);
            te.client.vote(&n_voter, &id, &false);
        }

        let deadline = te.client.get_proposal(&id).deadline;
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

        let total = yes_votes + no_votes;
        let should_pass = total >= quorum && yes_votes > no_votes;
        let result = te.client.try_execute_proposal(&id);

        if should_pass {
            prop_assert!(result.is_ok(), "proposal should have executed; yes={yes_votes} no={no_votes} quorum={quorum}");
            prop_assert_eq!(te.client.get_proposal(&id).state, ProposalState::Executed);
        } else {
            prop_assert!(result.is_err(), "proposal should NOT have executed; yes={yes_votes} no={no_votes} quorum={quorum}");
            prop_assert_ne!(te.client.get_proposal(&id).state, ProposalState::Executed);
        }
    }

    /// Adaptive-quorum EMA stays within [min_bps, max_bps] after any sequence
    /// of proposals (some passing, some failing quorum).
    #[test]
    fn prop_adaptive_quorum_stays_within_bounds(
        // Generate a sequence of per-proposal participation levels as fractions
        // of quorum (0 = no votes, 200 = 200 % of quorum).
        participations in proptest::collection::vec(0_u32..=200, 1..=12),
        min_bps in 500_u32..=3_000,
        max_bps_delta in 1_000_u32..=5_000,
    ) {
        let max_bps = (min_bps + max_bps_delta).min(10_000);
        let quorum = 500_i128;

        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, quorum, min_bps, max_bps);

        let proposer = Address::generate(&env);
        // Give proposer enough tokens for all proposals.
        mint(&env, &te.token, &proposer, 10_000);

        for participation_pct in &participations {
            let id = make_proposal(&te, &proposer);
            // total_votes = participation_pct% of quorum
            let total = (quorum * i128::from(*participation_pct)) / 100;
            if total > 0 {
                let voter = Address::generate(&env);
                mint(&env, &te.token, &voter, total);
                // Always vote yes so that if quorum is met it passes.
                te.client.vote(&voter, &id, &true);
            }

            let deadline = te.client.get_proposal(&id).deadline;
            env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
            // We don't care if it passes or fails; just drive the EMA.
            let _ = te.client.try_execute_proposal(&id);

            let ema = te.client.current_quorum_bps();
            prop_assert!(
                ema >= min_bps,
                "EMA {ema} is below min_quorum_bps {min_bps}"
            );
            prop_assert!(
                ema <= max_bps,
                "EMA {ema} exceeds max_quorum_bps {max_bps}"
            );
        }
    }

    /// An address that has already voted cannot influence the tally on a
    /// concurrent second proposal — votes are keyed per (proposal_id, voter).
    #[test]
    fn prop_vote_isolation_across_proposals(n_proposals in 2_u32..=6) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);

        let voter = Address::generate(&env);
        mint(&env, &te.token, &voter, 100);

        let mut ids = std::vec::Vec::new();
        for _ in 0..n_proposals {
            ids.push(make_proposal(&te, &proposer));
        }

        // Vote on every proposal once — all should succeed.
        for &id in &ids {
            let result = te.client.try_vote(&voter, &id, &true);
            prop_assert!(result.is_ok(), "first vote on proposal {id} should succeed");
        }

        // Re-voting on any proposal must fail.
        for &id in &ids {
            let result = te.client.try_vote(&voter, &id, &true);
            prop_assert!(result.is_err(), "re-vote on proposal {id} should be rejected");
        }
    clippy::indexing_slicing
)]
#![cfg(test)]

use crate::storage::ProposalState;
use crate::{DaoContract, DaoContractClient};
use proptest::prelude::*;
use soroban_sdk::{Address, Env, String, testutils::Address as _};
use soroban_token_template::{TokenContract, TokenContractClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_env_and_client(
    quorum: i128,
    execution_delay: u32,
) -> (Env, DaoContractClient, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_addr = env.register_contract(None, TokenContract);
    TokenContractClient::new(&env, &token_addr).initialize(
        &admin,
        &String::from_str(&env, "GOV"),
        &String::from_str(&env, "GOV"),
        &0u32,
        &None,
    );

    let dao_addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(&env, &dao_addr);
    client.initialize(&admin, &token_addr, &100u32, &quorum, &0u32, &execution_delay);

    (env, client, admin, token_addr)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    TokenContractClient::new(env, token).mint(to, &amount);
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    /// Property: yes_votes + no_votes never exceed total_supply_at_creation.
    ///
    /// Because each voter's weight is capped at the snapshot supply and the
    /// same voter cannot vote twice, the aggregate vote count can never
    /// exceed the supply captured at proposal creation.
    #[test]
    fn prop_votes_never_exceed_snapshot_supply(
        num_voters in 1usize..=5usize,
        voter_amounts in proptest::collection::vec(1i128..=200i128, 1..=5),
    ) {
        let (env, client, admin, token) = make_env_and_client(1, 0);

        // Mint to proposer so they can create a proposal.
        let proposer = Address::generate(&env);
        mint(&env, &token, &proposer, 10);

        let id = client.create_proposal(
            &proposer,
            &String::from_str(&env, "Test"),
            &String::from_str(&env, "Invariant check"),
        ).unwrap();

        let snapshot = client.get_proposal(&id).total_supply_at_creation;

        for i in 0..num_voters.min(voter_amounts.len()) {
            let voter = Address::generate(&env);
            mint(&env, &token, &voter, voter_amounts[i]);
            let _ = client.try_vote(&voter, &id, &(i % 2 == 0));
        }

        let proposal = client.get_proposal(&id);
        let total_votes = proposal.yes_votes + proposal.no_votes;

        prop_assert!(
            total_votes <= snapshot,
            "total_votes ({}) exceeded snapshot supply ({})",
            total_votes,
            snapshot
        );
        let _ = admin; // suppress unused warning
    }

    /// Property: A proposal that never met quorum can never reach Executed state.
    ///
    /// With quorum = i128::MAX - 1 no realistic vote total can satisfy it, so
    /// queue_proposal must always return QuorumNotMet and the proposal stays Active.
    #[test]
    fn prop_execution_requires_quorum(
        voter_amount in 1i128..=1000i128,
    ) {
        // Set quorum far beyond any realistic vote total.
        let quorum = 1_000_000i128;
        let (env, client, _admin, token) = make_env_and_client(quorum, 0);

        let proposer = Address::generate(&env);
        mint(&env, &token, &proposer, voter_amount);

        let id = client.create_proposal(
            &proposer,
            &String::from_str(&env, "Q"),
            &String::from_str(&env, "test"),
        ).unwrap();

        let voter = Address::generate(&env);
        mint(&env, &token, &voter, voter_amount);
        let _ = client.try_vote(&voter, &id, &true);

        // Advance past deadline.
        let deadline = client.get_proposal(&id).deadline;
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

        // Attempting to queue should fail because voter_amount < quorum (1_000_000).
        let queue_result = client.try_queue_proposal(&id);

        // Proposal must remain Active (not Queued/Executed).
        let state = client.get_proposal(&id).state;
        prop_assert!(
            state == ProposalState::Active,
            "Proposal should still be Active after quorum failure, got {:?}",
            state
        );
        prop_assert!(queue_result.is_err(), "queue_proposal should have failed");
    }
}
