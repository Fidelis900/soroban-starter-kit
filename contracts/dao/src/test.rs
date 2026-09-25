#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, String, Vec,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    vec,
};
use soroban_token_template::{TokenContract, TokenContractClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Standard test setup: voting_period=100, quorum=500, no bond, adaptive quorum disabled.
/// Default setup: voting_period=100, quorum=500 (absolute), quorum_bps=0, execution_delay=0.
fn setup(env: &Env) -> (DaoContractClient, Address, Address, Address) {
    setup_with_params(env, 500, 0, 0)
}

/// Setup with explicit quorum (absolute), quorum_bps (percentage), and execution_delay.
///
/// The governance token is `soroban-token-template`, not a bare Stellar
/// Asset Contract: the quorum_bps check needs `total_supply`, which is not
/// part of the SEP-41 `TokenInterface` a SAC exposes.
fn setup_with_params(
    env: &Env,
    quorum: i128,
    quorum_bps: u32,
    execution_delay: u32,
) -> (DaoContractClient, Address, Address, Address) {
    let admin = Address::generate(env);
    let token_addr = env.register_contract(None, TokenContract);
    TokenContractClient::new(env, &token_addr).initialize(
        &admin,
        &String::from_str(env, "Governance Token"),
        &String::from_str(env, "GOV"),
        &7u32,
        &None,
    );

    let addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(env, &addr);
    // proposal_bond=0, min/max_quorum_bps=0 → adaptive quorum disabled
    client.initialize(&admin, &token, &100, &500, &0, &0, &0);

    (client, admin, token, addr)
}

/// Setup with a non-zero proposal bond.
fn setup_with_bond(env: &Env, bond: i128) -> (DaoContractClient, Address, Address, Address) {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();

    let addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(env, &addr);
    client.initialize(&admin, &token, &100, &500, &bond, &0, &0);

    (client, admin, token, addr)
}

/// Setup with adaptive quorum enabled.
fn setup_adaptive(env: &Env, min_bps: u32, max_bps: u32) -> (DaoContractClient, Address, Address, Address) {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();

    let addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(env, &addr);
    client.initialize(&admin, &token, &100, &500, &0, &min_bps, &max_bps);
    client.initialize(&admin, &token_addr, &100, &quorum, &quorum_bps, &execution_delay);

    (client, admin, token_addr, addr)
}

/// Backwards-compatible helper used by legacy tests that do not care about execution_delay.
fn setup_with_quorum_bps(
    env: &Env,
    quorum: i128,
    quorum_bps: u32,
) -> (DaoContractClient, Address, Address, Address) {
    setup_with_params(env, quorum, quorum_bps, 0)
}

fn mint_tokens(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
    let _ = admin;
    TokenContractClient::new(env, token).mint(to, &amount);
}

/// Helper: create a plain proposal (no action payload).
fn create_plain_proposal(
    client: &DaoContractClient,
    env: &Env,
    proposer: &Address,
) -> u32 {
    client.create_proposal(
        proposer,
        &String::from_str(env, "P"),
        &String::from_str(env, "D"),
        &None,
        &None,
        &None,
    )
}

// ---------------------------------------------------------------------------
// Core lifecycle tests
// Existing tests (updated for new initialize signature)
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup(&env);
    assert_eq!(client.proposal_count(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);
    client.initialize(&admin, &token, &100, &500, &0, &0, &0);
    client.initialize(&admin, &token, &100, &500, &0, &0);
}

#[test]
fn test_create_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);

    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Upgrade Protocol"),
        &String::from_str(&env, "Upgrade to v2"),
        &None,
        &None,
        &None,
    );
    assert_eq!(id, 0);
    assert_eq!(client.proposal_count(), 1);

    let proposal = client.get_proposal(&0);
    assert_eq!(proposal.state, ProposalState::Active);
    assert_eq!(proposal.yes_votes, 0);
    assert_eq!(proposal.no_votes, 0);
    assert_eq!(proposal.bond_amount, 0);
    // #1102: total_supply_at_creation should now be the real supply, not i128::MAX.
    assert_eq!(proposal.total_supply_at_creation, 1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_create_proposal_no_tokens_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup(&env);

    let proposer = Address::generate(&env);
    client.create_proposal(
        &proposer,
        &String::from_str(&env, "Bad Proposal"),
        &String::from_str(&env, "no tokens"),
        &None,
        &None,
        &None,
    );
}

#[test]
fn test_vote_yes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.yes_votes, 600);
    assert_eq!(proposal.no_votes, 0);
}

#[test]
fn test_vote_no() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 300);
    client.vote(&voter, &id, &false);

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.yes_votes, 0);
    assert_eq!(proposal.no_votes, 300);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_vote_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 100);
    client.vote(&voter, &id, &true);
    client.vote(&voter, &id, &true);
}

/// Full happy-path with execution_delay=0: vote → queue → execute in sequence.
#[test]
fn test_execute_proposal_passes() {
    let env = Env::default();
    env.mock_all_auths();
    // execution_delay=0: execute immediately after deadline.
    let (client, admin, token, _) = setup_with_params(&env, 500, 0, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    // Advance past voting deadline (voting_period = 100).
    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // With delay=0, queue sets execution_eta = deadline + 0 = deadline, which is ≤ current.
    client.queue_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Queued);

    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_queue_before_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);
    client.execute_proposal(&id);
    // Do NOT advance past deadline — queue_proposal should return DeadlineNotReached (#6).
    client.queue_proposal(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_queue_quorum_not_met_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 100); // 100 < 500 quorum
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.queue_proposal(&id);
}

#[test]
fn test_cancel_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    client.cancel_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_cancel_already_executed_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup_with_params(&env, 500, 0, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.execute_proposal(&id);

    client.cancel_proposal(&id);
}

// ---------------------------------------------------------------------------
// Issue #1106 — Proposal bond tests
// ---------------------------------------------------------------------------

#[test]
fn test_bond_escrowed_on_create() {
    let env = Env::default();
    env.mock_all_auths();
    let bond = 200_i128;
    let (client, admin, token, dao_addr) = setup_with_bond(&env, bond);

    mint_tokens(&env, &token, &admin, &admin, 1_000);

    let before_dao = soroban_sdk::token::Client::new(&env, &token).balance(&dao_addr);
    create_plain_proposal(&client, &env, &admin);
    let after_dao = soroban_sdk::token::Client::new(&env, &token).balance(&dao_addr);

    assert_eq!(after_dao - before_dao, bond, "bond should be held by DAO");
    let proposal = client.get_proposal(&0);
    assert_eq!(proposal.bond_amount, bond);
}

#[test]
fn test_bond_refunded_on_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let bond = 200_i128;
    let (client, admin, token, _) = setup_with_bond(&env, bond);

    mint_tokens(&env, &token, &admin, &admin, 2_000);

    let id = create_plain_proposal(&client, &env, &admin);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    let before = soroban_sdk::token::Client::new(&env, &token).balance(&admin);
    client.execute_proposal(&id);
    let after = soroban_sdk::token::Client::new(&env, &token).balance(&admin);

    assert_eq!(after - before, bond, "bond should be refunded to proposer");
}

#[test]
fn test_bond_slashed_on_quorum_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let bond = 200_i128;
    let (client, admin, token, _) = setup_with_bond(&env, bond);

    mint_tokens(&env, &token, &admin, &admin, 2_000);
    let id = create_plain_proposal(&client, &env, &admin);

    // Vote only 100 — below quorum of 500.
    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 100);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    let before_admin = soroban_sdk::token::Client::new(&env, &token).balance(&admin);
    // execute_proposal returns QuorumNotMet but still slashes the bond.
    let result = client.try_execute_proposal(&id);
    assert!(result.is_err(), "should fail with QuorumNotMet");

    let after_admin = soroban_sdk::token::Client::new(&env, &token).balance(&admin);
    assert_eq!(after_admin - before_admin, bond, "bond should be slashed to admin treasury");
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_create_proposal_insufficient_bond_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let bond = 500_i128;
    let (client, admin, token, _) = setup_with_bond(&env, bond);

    // Mint only 100 — less than the 500-token bond requirement.
    mint_tokens(&env, &token, &admin, &admin, 100);
    create_plain_proposal(&client, &env, &admin);
}

// ---------------------------------------------------------------------------
// Issue #1108 — Executable action payload tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_proposal_with_action_payload_stored() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);
    mint_tokens(&env, &token, &admin, &admin, 1_000);

    // Register a dummy target contract just to capture the address.
    let target = Address::generate(&env);
    let function = Symbol::new(&env, "transfer");
    let args: soroban_sdk::Vec<soroban_sdk::Val> = vec![&env];

    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Fund transfer"),
        &String::from_str(&env, "Send tokens"),
        &Some(target.clone()),
        &Some(function.clone()),
        &Some(args.clone()),
    );

    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.action_target, Some(target));
    assert_eq!(proposal.action_function, Some(function));
    assert!(proposal.action_args.is_some());
}

// ---------------------------------------------------------------------------
// Issue #1107 — Adaptive quorum tests
// ---------------------------------------------------------------------------

#[test]
fn test_adaptive_quorum_updates_after_execution() {
    let env = Env::default();
    env.mock_all_auths();
    // Enable adaptive quorum: 1000–9000 bps (10%–90%)
    let (client, admin, token, _) = setup_adaptive(&env, 1_000, 9_000);

    mint_tokens(&env, &token, &admin, &admin, 2_000);

    let initial_ema = client.current_quorum_bps();

    let id = create_plain_proposal(&client, &env, &admin);
    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.queue_proposal(&id);
    client.execute_proposal(&id);

    let updated_ema = client.current_quorum_bps();
    // EMA must change and remain within bounds.
    assert!(updated_ema >= 1_000, "ema below min");
    assert!(updated_ema <= 9_000, "ema above max");
    // With quorum=500 and total_votes=600, participation BPS = 10_000; EMA
    // should have moved upward from the midpoint of 5000.
    assert_ne!(updated_ema, initial_ema, "EMA should have changed");
}

#[test]
fn test_adaptive_quorum_disabled_when_bounds_zero() {
    let env = Env::default();
    env.mock_all_auths();
    // Both bounds zero → adaptive quorum disabled
    let (client, admin, token, _) = setup(&env);
    mint_tokens(&env, &token, &admin, &admin, 2_000);

    let initial_ema = client.current_quorum_bps();
    assert_eq!(initial_ema, 0, "EMA should start at 0 when disabled");

    let id = create_plain_proposal(&client, &env, &admin);
    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.execute_proposal(&id);

    assert_eq!(
        client.current_quorum_bps(),
        0,
        "EMA should remain 0 when adaptive quorum is disabled"
    );
}

#[test]
fn test_adaptive_quorum_clamped_to_min() {
    let env = Env::default();
    env.mock_all_auths();
    // Narrow band: 4000–6000 bps
    let (client, admin, token, _) = setup_adaptive(&env, 4_000, 6_000);
    mint_tokens(&env, &token, &admin, &admin, 2_000);

    // Run several low-participation proposals to push EMA toward min.
    for _ in 0..5u32 {
        let id = create_plain_proposal(&client, &env, &admin);
        // Vote exactly at quorum (500) → 100% participation BPS = 10_000.
        // Actually use a voter with just above quorum to test clamping.
        // Use a tiny voter (participation BPS = 0) — quorum not met path.
        let deadline = client.get_proposal(&id).deadline;
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
        // Don't vote — quorum will not be met, slash path, EMA gets 0.
        let _ = client.try_execute_proposal(&id);
    }

    let ema = client.current_quorum_bps();
    assert!(ema >= 4_000, "EMA must not go below min_quorum_bps");
    assert!(ema <= 6_000, "EMA must not exceed max_quorum_bps");
}

#[test]
fn test_total_votes_invariant() {
    // yes_votes + no_votes == total_votes always holds.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = create_plain_proposal(&client, &env, &admin);

    let v1 = Address::generate(&env);
    let v2 = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &v1, 300);
    mint_tokens(&env, &token, &admin, &v2, 400);

    client.vote(&v1, &id, &true);
    client.vote(&v2, &id, &false);

    let p = client.get_proposal(&id);
    assert_eq!(p.yes_votes + p.no_votes, 700);
    // cancel_proposal only works on Active proposals.
    client.cancel_proposal(&id);
}

// ---------------------------------------------------------------------------
// #830 — Proposer self-cancellation
// ---------------------------------------------------------------------------

/// Proposer can cancel their own proposal before any votes are cast.
#[test]
fn test_proposer_cancel_pre_vote_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Mistake"),
        &String::from_str(&env, "Oops"),
    );

    // No votes cast yet — proposer self-cancel should succeed.
    client.proposer_cancel_proposal(&admin, &id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Cancelled);
}

/// Proposer self-cancel is rejected once any vote has been cast.
#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_proposer_cancel_after_vote_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    // At least one vote cast — proposer self-cancel must be rejected.
    client.proposer_cancel_proposal(&admin, &id);
}

/// A non-proposer cannot use `proposer_cancel_proposal`.
#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_proposer_cancel_wrong_caller_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let impostor = Address::generate(&env);
    // impostor is not the original proposer
    client.proposer_cancel_proposal(&impostor, &id);
}

/// Proposer self-cancel fails on an already-cancelled proposal.
#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_proposer_cancel_already_cancelled_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    client.proposer_cancel_proposal(&admin, &id);
    // Second call on an already-cancelled proposal must fail.
    client.proposer_cancel_proposal(&admin, &id);
}

// ---------------------------------------------------------------------------
// #829 — quorum_bps (percentage-based quorum)
// ---------------------------------------------------------------------------

/// Proposal passes when participation meets both absolute quorum and quorum_bps.
///
/// Setup: total supply = 1_000, quorum_bps = 5_000 (50%).
/// Voter holds 600 tokens (60% > 50%) and votes yes → should queue and execute.
#[test]
fn test_execute_meets_quorum_bps() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=0 (disabled), quorum_bps=5_000 (50% of supply required).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 0, 5_000);

    // `create_proposal` requires the proposer to hold a nonzero balance, so
    // admin needs *some* tokens to propose — but those must not count toward
    // total supply when quorum_bps is checked at execution time, so admin
    // burns them again immediately after creating the proposal. Final total
    // supply = 600 (all held by `voter`).
    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );
    soroban_sdk::token::Client::new(&env, &token).burn(&admin, &1_000);

    let voter = Address::generate(&env);
    // Voter gets all 600 tokens of the remaining total supply: 600/600 = 100% ≥ 50%.
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // 600 / 600 = 100% ≥ 50% → quorum_bps met, should queue then execute.
    client.queue_proposal(&id);
    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

/// Proposal is rejected when participation is below quorum_bps.
///
/// Setup: total supply = 2_000, quorum_bps = 5_000 (50%).
/// Voter holds 500 tokens (25% < 50%) → QuorumNotMet.
#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_execute_below_quorum_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=0, quorum_bps=5_000 (50%).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 0, 5_000);

    // Mint 2_000 total: 1_500 to admin (won't vote), 500 to voter.
    mint_tokens(&env, &token, &admin, &admin, 1_500);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 500); // total supply = 2_000
    client.vote(&voter, &id, &true); // 500/2_000 = 25% < 50%

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.queue_proposal(&id); // must panic with QuorumNotMet (#8)
}

/// Both absolute quorum and quorum_bps are enforced simultaneously.
#[test]
fn test_execute_both_quorums_met() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=300 (absolute), quorum_bps=2_500 (25%).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 300, 2_500);

    // Total supply = 1_000: voter gets 400 (40% ≥ 25%, and 400 ≥ 300).
    mint_tokens(&env, &token, &admin, &admin, 600);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 400);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    client.queue_proposal(&id);
    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

/// `initialize` rejects quorum_bps > 10_000.
#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_initialize_invalid_quorum_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let addr = env.register_contract(None, DaoContract);
    let client = DaoContractClient::new(&env, &addr);
    // quorum_bps = 10_001 is out of range
    client.initialize(&admin, &token, &100, &500, &10_001, &0);
}

// ---------------------------------------------------------------------------
// #941 — VotingClosed error for post-deadline voting
// ---------------------------------------------------------------------------

/// vote() returns VotingClosed (not DeadlineNotReached) when deadline has passed.
#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_vote_after_deadline_returns_voting_closed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);

    // Advance past voting deadline (voting_period = 100)
    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // vote() after deadline should return VotingClosed (error code #13)
    client.vote(&voter, &id, &true);
}

// ---------------------------------------------------------------------------
// #942 — Snapshot voting power to prevent flash-loan attacks
// ---------------------------------------------------------------------------

/// Voting weight is capped by the total supply at proposal creation time,
/// preventing flash-loan style vote manipulation.
#[test]
fn test_flash_loan_voting_weight_capped() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=100 (low to ensure pass), quorum_bps=0 (disabled).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 100, 0);

    // Initial total supply = 1_000 (mint to admin only)
    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    // Verify proposal captured total_supply_at_creation = 1_000
    let proposal = client.get_proposal(&id);
    assert_eq!(proposal.total_supply_at_creation, 1_000);

    let voter = Address::generate(&env);
    // Mint a huge amount to simulate a flash loan (total supply becomes 10_000)
    mint_tokens(&env, &token, &admin, &voter, 9_000); // voter now has 9_000, total supply = 10_000

    // Vote with the inflated balance — weight should be capped at snapshot supply (1_000).
    client.vote(&voter, &id, &true);

    // Voting weight should be capped at total_supply_at_creation (1_000), not the actual balance (9_000)
    let proposal_after = client.get_proposal(&id);
    assert_eq!(proposal_after.yes_votes, 1_000); // capped, not 9_000
    assert_eq!(proposal_after.no_votes, 0);
}

/// Voting weight respects the snapshot even when checking execution quorum.
#[test]
fn test_quorum_bps_uses_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=0 (disabled), quorum_bps=5_000 (50% of supply at proposal time).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 0, 5_000);

    // Initial supply = 1_000
    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    // At vote time: 600/1_600 = 37.5% < 50% (would fail if using live supply)
    // But with snapshot: 600/1_000 = 60% ≥ 50% (should pass)

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // Should queue and execute successfully using the snapshot (600/1_000 = 60% ≥ 50%)
    client.queue_proposal(&id);
    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

// ---------------------------------------------------------------------------
// #1102 — Fix percentage quorum calculation and total_supply_at_creation
// ---------------------------------------------------------------------------

/// create_proposal stores the real total supply, not i128::MAX.
#[test]
fn test_create_proposal_captures_real_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 5_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Supply Check"),
        &String::from_str(&env, "Verifies snapshot"),
    );

    let proposal = client.get_proposal(&id);
    // Must be the real supply (5_000), not i128::MAX.
    assert_eq!(proposal.total_supply_at_creation, 5_000);
    assert_ne!(proposal.total_supply_at_creation, i128::MAX);
}

/// quorum_bps calculation correctly uses the snapshot supply, not live supply.
/// Minting tokens after proposal creation must not affect the quorum check.
#[test]
fn test_quorum_bps_uses_creation_snapshot_not_live_supply() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=0, quorum_bps=5_000 (50%).
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 0, 5_000);

    // At proposal creation: total supply = 1_000.
    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );
    assert_eq!(client.get_proposal(&id).total_supply_at_creation, 1_000);

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    // After voting, inflate total supply massively — must not affect quorum check.
    let whale = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &whale, 100_000);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // Quorum check: 600 * 10_000 >= 5_000 * 1_000 → 6_000_000 >= 5_000_000 → pass.
    client.queue_proposal(&id);
    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

// ---------------------------------------------------------------------------
// #1103 — Token locking prevents flash-loan vote manipulation
// ---------------------------------------------------------------------------

/// Tokens are physically transferred into the DAO contract on vote(),
/// proving the anti-flash-loan lock is in effect.
#[test]
fn test_tokens_locked_in_dao_on_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, dao_addr) = setup_with_quorum_bps(&env, 100, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    // Before vote: voter holds 600, DAO holds 0.
    assert_eq!(token_client.balance(&voter), 600);
    assert_eq!(token_client.balance(&dao_addr), 0);

    client.vote(&voter, &id, &true);

    // After vote: voter holds 0, DAO holds 600.
    assert_eq!(token_client.balance(&voter), 0);
    assert_eq!(token_client.balance(&dao_addr), 600);
}

/// After the voting deadline, voter can reclaim their locked tokens via unlock_tokens().
#[test]
fn test_unlock_tokens_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, dao_addr) = setup_with_params(&env, 100, 0, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    // Before unlock: voter has 0, DAO has 600.
    assert_eq!(token_client.balance(&voter), 0);
    assert_eq!(token_client.balance(&dao_addr), 600);

    client.unlock_tokens(&voter, &id);

    // After unlock: voter has 600 back, DAO has 0.
    assert_eq!(token_client.balance(&voter), 600);
    assert_eq!(token_client.balance(&dao_addr), 0);
}

/// unlock_tokens is rejected while the voting window is still open.
#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_unlock_tokens_during_voting_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    // Voting still open — unlock must fail with VotingStillOpen (#17).
    client.unlock_tokens(&voter, &id);
}

// ---------------------------------------------------------------------------
// #1104 — Execution timelock
// ---------------------------------------------------------------------------

/// execute_proposal is rejected before the execution timelock expires.
#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_execute_before_timelock_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();
    // execution_delay = 50 ledgers.
    let (client, admin, token, _) = setup_with_params(&env, 100, 0, 50);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    // Advance past voting deadline but NOT past execution_eta (deadline + 50).
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    client.queue_proposal(&id);

    // execution_eta = deadline + 50; current = deadline + 1 → too early.
    client.execute_proposal(&id); // must panic with TimelockNotExpired (#14)
}

/// execute_proposal succeeds once the execution timelock has fully elapsed.
#[test]
fn test_execute_after_timelock_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    // execution_delay = 50 ledgers.
    let (client, admin, token, _) = setup_with_params(&env, 100, 0, 50);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;

    // Queue: advance just past voting deadline.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.queue_proposal(&id);

    let execution_eta = client.get_proposal(&id).execution_eta;
    assert_eq!(execution_eta, deadline + 50);

    // Execute: advance to execution_eta.
    env.ledger().with_mut(|l| l.sequence_number = execution_eta);
    client.execute_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Executed);
}

/// Admin can veto a queued proposal before its execution_eta.
#[test]
fn test_admin_veto_queued_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    // execution_delay = 50 ledgers.
    let (client, admin, token, _) = setup_with_params(&env, 100, 0, 50);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Malicious"),
        &String::from_str(&env, "Bad"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.queue_proposal(&id);

    // Admin vetoes while timelock is still ticking.
    client.veto_proposal(&id);
    assert_eq!(client.get_proposal(&id).state, ProposalState::Cancelled);
}

/// execute_proposal is rejected on an Active (not-yet-queued) proposal.
#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_execute_active_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &voter, 600);
    client.vote(&voter, &id, &true);

    let deadline = client.get_proposal(&id).deadline;
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // Skip queue_proposal — execute directly on Active proposal: must fail (#5).
    client.execute_proposal(&id);
}

/// veto_proposal is rejected on a non-Queued (Active) proposal.
#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_veto_active_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup(&env);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    // Proposal is still Active — veto must fail with InvalidState (#5).
    client.veto_proposal(&id);
}

// ---------------------------------------------------------------------------
// #1105 — Vote delegation
// ---------------------------------------------------------------------------

/// A delegator can assign their vote weight to a delegatee.
#[test]
fn test_delegate_and_get_delegate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup(&env);

    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    client.delegate(&delegator, &delegatee);
    assert_eq!(client.get_delegate(&delegator), Some(delegatee));
}

/// Delegator can undelegate, removing the mapping.
#[test]
fn test_undelegate_removes_mapping() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup(&env);

    let delegator = Address::generate(&env);
    let delegatee = Address::generate(&env);

    client.delegate(&delegator, &delegatee);
    client.undelegate(&delegator);
    assert_eq!(client.get_delegate(&delegator), None);
}

/// Circular delegation (A→B, B→A) is rejected at delegate() time.
#[test]
#[should_panic(expected = "Error(Contract, #15)")]
fn test_circular_delegation_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // A→B is fine.
    client.delegate(&alice, &bob);
    // B→A would create a cycle — must fail with CircularDelegation (#15).
    client.delegate(&bob, &alice);
}

/// vote_with_delegators aggregates the delegator's locked weight.
#[test]
fn test_vote_with_delegators_aggregates_weight() {
    let env = Env::default();
    env.mock_all_auths();
    // quorum=100, quorum_bps=0.
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 100, 0);

    // Total supply = 1_000 (admin holds it all for proposal creation snapshot).
    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "Delegated Vote"),
        &String::from_str(&env, "D"),
    );

    // Delegator holds 200 tokens and delegates to voter.
    let delegator = Address::generate(&env);
    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &delegator, 200);
    mint_tokens(&env, &token, &admin, &voter, 300);
    client.delegate(&delegator, &voter);

    // Delegator locks tokens first.
    client.lock_for_vote(&delegator, &id);

    // Voter casts vote with delegated weight included.
    let mut delegators = Vec::new(&env);
    delegators.push_back(delegator.clone());
    client.vote_with_delegators(&voter, &id, &true, &delegators);

    let proposal = client.get_proposal(&id);
    // Expected weight = min(300 + 200, 1_000) = 500.
    assert_eq!(proposal.yes_votes, 500);
}

/// vote_with_delegators rejects a delegator that did not actually delegate to voter.
#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_vote_with_delegators_wrong_delegatee_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 100, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let delegator = Address::generate(&env);
    let voter = Address::generate(&env);
    let third_party = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &delegator, 200);
    mint_tokens(&env, &token, &admin, &voter, 300);

    // Delegator delegates to third_party, NOT to voter.
    client.delegate(&delegator, &third_party);
    client.lock_for_vote(&delegator, &id);

    let mut delegators = Vec::new(&env);
    delegators.push_back(delegator.clone());
    // voter tries to claim delegated weight they don't own — must fail (#1).
    client.vote_with_delegators(&voter, &id, &true, &delegators);
}

/// vote_with_delegators rejects a delegator who has not pre-locked tokens.
#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_vote_with_delegators_no_locked_tokens_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token, _) = setup_with_quorum_bps(&env, 100, 0);

    mint_tokens(&env, &token, &admin, &admin, 1_000);
    let id = client.create_proposal(
        &admin,
        &String::from_str(&env, "P"),
        &String::from_str(&env, "D"),
    );

    let delegator = Address::generate(&env);
    let voter = Address::generate(&env);
    mint_tokens(&env, &token, &admin, &delegator, 200);
    mint_tokens(&env, &token, &admin, &voter, 300);
    client.delegate(&delegator, &voter);

    // Delegator did NOT call lock_for_vote — must fail with NoLockedTokens (#16).
    let mut delegators = Vec::new(&env);
    delegators.push_back(delegator.clone());
    client.vote_with_delegators(&voter, &id, &true, &delegators);
}
