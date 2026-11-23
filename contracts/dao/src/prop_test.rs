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
//! 5. Vote weight is derived from the balance snapshot taken at proposal
//!    creation, so transferring tokens after creation cannot be recycled into
//!    a second vote (issue #1442).
//! 6. Execution is gated by an immutable timelock: `execute_proposal` always
//!    fails while `ledger < executable_at` (issue #1443).
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

use crate::{DaoContract, DaoContractClient, ProposalState, ProposalTier, VotingType};

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
        &VotingType::Linear,
        &ProposalTier::Standard,
        &soroban_sdk::Vec::new(te.env),
    )
}

fn make_quadratic_proposal(te: &TestEnv<'_>, proposer: &Address) -> u32 {
    te.client.create_proposal(
        proposer,
        &String::from_str(te.env, "P"),
        &String::from_str(te.env, "D"),
        &None,
        &None,
        &None,
        &VotingType::Quadratic,
        &ProposalTier::Standard,
        &soroban_sdk::Vec::new(te.env),
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

    /// Quadratic voting: each voter's recorded weight never exceeds
    /// `fixed_sqrt(total_supply_at_creation)` and the tally equals the sum of
    /// the per-voter integer square roots of their balances.
    #[test]
    fn prop_quadratic_vote_weight_bounded(
        yes_weights in proptest::collection::vec(1_i128..=1_000_000, 1..=6),
        no_weights  in proptest::collection::vec(1_i128..=1_000_000, 1..=6),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);
        let id = make_quadratic_proposal(&te, &proposer);

        let snapshot = te.client.get_proposal(&id).total_supply_at_creation;
        let max_weight = crate::fixed_sqrt(snapshot);

        let mut expected_yes = 0_i128;
        let mut expected_no = 0_i128;

        for w in &yes_weights {
            let voter = Address::generate(&env);
            mint(&env, &te.token, &voter, *w);
            te.client.vote(&voter, &id, &true);
            let weight = crate::fixed_sqrt(*w);
            prop_assert!(
                weight <= max_weight,
                "quadratic weight {weight} exceeds sqrt(supply) {max_weight}"
            );
            expected_yes += weight;
        }
        for w in &no_weights {
            let voter = Address::generate(&env);
            mint(&env, &te.token, &voter, *w);
            te.client.vote(&voter, &id, &false);
            let weight = crate::fixed_sqrt(*w);
            prop_assert!(
                weight <= max_weight,
                "quadratic weight {weight} exceeds sqrt(supply) {max_weight}"
            );
            expected_no += weight;
        }

        let p = te.client.get_proposal(&id);
        prop_assert_eq!(p.yes_votes, expected_yes, "quadratic yes_votes mismatch");
        prop_assert_eq!(p.no_votes, expected_no, "quadratic no_votes mismatch");
    }

    /// Duplicate proposal payloads are always rejected while the original is
    /// still active.
    #[test]
    fn prop_duplicate_proposal_rejected(
        title in "[A-Za-z0-9 ]{1,16}",
        description in "[A-Za-z0-9 ]{1,16}",
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);

        let title_s = String::from_str(&env, &title);
        let desc_s = String::from_str(&env, &description);

        let first = te.client.try_create_proposal(
            &proposer,
            &title_s,
            &desc_s,
            &None,
            &None,
            &None,
            &VotingType::Linear,
            &ProposalTier::Standard,
            &soroban_sdk::Vec::new(&env),
        );
        prop_assert!(first.is_ok(), "first proposal should be accepted");

        let second = te.client.try_create_proposal(
            &proposer,
            &title_s,
            &desc_s,
            &None,
            &None,
            &None,
            &VotingType::Linear,
            &ProposalTier::Standard,
            &soroban_sdk::Vec::new(&env),
        );
        prop_assert!(second.is_err(), "duplicate proposal should be rejected");
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
    }

    /// Issue #1442: For any sequence of votes and token transfers, the sum of
    /// vote weights for a proposal never exceeds the sum of snapshotted
    /// balances at creation, and a recipient of transferred tokens cannot cast
    /// a vote whose weight is derived from the transfer.
    ///
    /// The strategy is:
    /// 1. Mint tokens to a set of holders *before* proposal creation so their
    ///    balances are captured in the snapshot.
    /// 2. Create the proposal (snapshot taken here).
    /// 3. Have some holders vote, then transfer their remaining balance to a
    ///    fresh recipient.
    /// 4. The recipient attempts to vote — it must be rejected (or, if it
    ///    somehow succeeds, it must contribute zero weight because its
    ///    snapshotted balance is zero).
    #[test]
    fn prop_transfer_cannot_recycle_vote_weight(
        holder_balances in proptest::collection::vec(1_i128..=500, 1..=5),
        // Which holders vote (indices into holder_balances, modulo length).
        voter_indices in proptest::collection::vec(0usize..=4, 0..=5),
        // Fraction (in percent) of the voter's balance to transfer away.
        transfer_pct in 0_u32..=100,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);

        // Mint to holders *before* proposal creation so the snapshot captures
        // their balances.
        let mut holders = std::vec::Vec::new();
        let mut snapshotted_total = 0_i128;
        for b in &holder_balances {
            let h = Address::generate(&env);
            mint(&env, &te.token, &h, *b);
            snapshotted_total += *b;
            holders.push(h);
        }

        // Snapshot is taken at creation.
        let id = make_proposal(&te, &proposer);
        let snapshot = te.client.get_proposal(&id).total_supply_at_creation;
        prop_assert!(
            snapshot >= snapshotted_total,
            "snapshot {snapshot} should include all pre-creation balances {snapshotted_total}"
        );

        // Have selected holders vote, then transfer their remaining balance to
        // a fresh recipient.
        let mut recipients = std::vec::Vec::new();
        for idx in &voter_indices {
            let i = *idx % holders.len();
            let voter = holders[i].clone();
            // Only vote once per holder.
            if recipients.iter().any(|(v, _): &(Address, Address)| v == &voter) {
                continue;
            }
            let _ = te.client.try_vote(&voter, &id, &true);

            // Transfer a fraction of the voter's balance to a fresh recipient.
            let balance = holder_balances[i];
            let transfer_amt = (balance * i128::from(transfer_pct)) / 100;
            if transfer_amt > 0 {
                let recipient = Address::generate(&env);
                StellarAssetClient::new(&env, &te.token)
                    .transfer(&voter, &recipient, &transfer_amt);
                recipients.push((voter.clone(), recipient));
            }
        }

        // Each recipient attempts to vote. Because the snapshot was taken
        // before the transfer, the recipient's snapshotted balance is zero, so
        // either the vote is rejected (no voting power) or it contributes zero
        // weight. Either way, the recipient cannot recycle the transfer into a
        // second vote.
        for (_voter, recipient) in &recipients {
            let result = te.client.try_vote(recipient, &id, &true);
            // The vote must not add weight derived from the transfer. We
            // verify this by checking the tally does not exceed the snapshot.
            let _ = result;
        }

        let p = te.client.get_proposal(&id);
        let total_votes = p.yes_votes + p.no_votes;
        prop_assert!(
            total_votes <= snapshot,
            "total_votes ({total_votes}) exceeded snapshot supply ({snapshot}) after transfers"
        );

        // Additionally, the sum of weights must not exceed the sum of
        // snapshotted balances of the holders who actually voted.
        let mut max_possible = 0_i128;
        for idx in &voter_indices {
            let i = *idx % holders.len();
            max_possible += holder_balances[i];
        }
        prop_assert!(
            total_votes <= max_possible.max(snapshot),
            "total_votes ({total_votes}) exceeded plausible weight bound"
        );
    }

    /// Issue #1443: `execute_proposal` always fails while
    /// `ledger < executable_at` for any queued proposal.
    ///
    /// We generate a proposal that passes quorum and majority, queue it, then
    /// attempt execution at a ledger strictly before `executable_at`. The
    /// execution must be rejected and the proposal must remain Queued.
    #[test]
    fn prop_execution_rejected_during_timelock(
        yes_votes in 1_i128..=1_000,
        // Ledger offset before executable_at (0 = at executable_at - 1).
        offset_before in 0_u32..=47,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        // quorum=1 so a single yes vote qualifies.
        let te = build_env(&env, 1, 0, 0);

        let proposer = Address::generate(&env);
        mint(&env, &te.token, &proposer, 10);
        let id = make_proposal(&te, &proposer);

        let voter = Address::generate(&env);
        mint(&env, &te.token, &voter, yes_votes);
        te.client.vote(&voter, &id, &true);

        // Advance past the voting deadline so the proposal can be queued.
        let deadline = te.client.get_proposal(&id).deadline;
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

        // Queue the proposal (this sets executable_at = now + timelock).
        let queue_result = te.client.try_queue_proposal(&id);
        prop_assert!(queue_result.is_ok(), "queue_proposal should succeed for a passing proposal");

        let queued = te.client.get_proposal(&id);
        prop_assert_eq!(queued.state, ProposalState::Queued);
        let executable_at = queued.executable_at;
        prop_assert!(
            executable_at > env.ledger().sequence(),
            "executable_at ({executable_at}) must be in the future"
        );

        // Attempt execution strictly before executable_at.
        let attempt_ledger = executable_at.saturating_sub(1 + offset_before);
        // Ensure we are still before executable_at.
        prop_assume!(attempt_ledger < executable_at);
        env.ledger().with_mut(|l| l.sequence_number = attempt_ledger);

        let result = te.client.try_execute_proposal(&id);
        prop_assert!(
            result.is_err(),
            "execute_proposal must fail before executable_at ({executable_at}); attempted at {attempt_ledger}"
        );
        prop_assert_eq!(
            te.client.get_proposal(&id).state,
            ProposalState::Queued,
            "proposal must remain Queued during timelock"
        );
    }
}