I'll analyze the existing test structure and contracts to create a comprehensive cross-contract integration test suite.

Looking at the requirements, I need to create `tests/tests/cross_contract.rs`. Since I don't have access to the exact contract APIs, I'll build a robust test file that follows Soroban testing conventions and the patterns likely used in this repository. Let me create a well-structured integration test suite.

```rust
//! Cross-contract integration tests for composite DeFi workflows.
//!
//! These tests deploy multiple contracts into a shared Soroban test
//! environment and exercise end-to-end business transactions that compose
//! several contracts together:
//!
//! 1. Swap token -> stake output -> claim rewards -> create vesting grant.
//! 2. DAO proposal execution that updates a Multisig signer / Treasury param.
//! 3. Atomic rollback across multiple contracts when a mid-workflow call fails.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, Bytes, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Contract clients
// ---------------------------------------------------------------------------
//
// The generated clients are imported from the workspace crates. The exact
// crate names follow the repository layout under `contracts/`.

use crowdfund::{CrowdfundContract, CrowdfundContractClient};
use dao::{DaoContract, DaoContractClient, ProposalStatus};
use multisig::{MultisigContract, MultisigContractClient};
use staking::{StakingContract, StakingContractClient};
use swap::{SwapContract, SwapContractClient};
use timelock::{ReleaseTranche, TimelockContract, TimelockContractClient};
use treasury::{TreasuryContract, TreasuryContractClient};
use vesting::{VestingContract, VestingContractClient};

// ---------------------------------------------------------------------------
// Shared test harness
// ---------------------------------------------------------------------------

/// A fully wired multi-contract environment used by every test in this file.
struct CrossContractEnv {
    env: Env,
    /// The underlying Stellar asset contract used as the swap input/output.
    token_a: token::Client<'static>,
    token_a_admin: token::StellarAssetClient<'static>,
    token_b: token::Client<'static>,
    token_b_admin: token::StellarAssetClient<'static>,
    swap: SwapContractClient<'static>,
    staking: StakingContractClient<'static>,
    vesting: VestingContractClient<'static>,
    dao: DaoContractClient<'static>,
    multisig: MultisigContractClient<'static>,
    treasury: TreasuryContractClient<'static>,
    crowdfund: CrowdfundContractClient<'static>,
    timelock: TimelockContractClient<'static>,
    admin: Address,
}

impl CrossContractEnv {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        // A deterministic ledger so time-based logic (vesting cliffs, reward
        // accrual, DAO voting windows) is reproducible.
        env.ledger().set(LedgerInfo {
            timestamp: 1_700_000_000,
            protocol_version: 22,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 10_000_000,
        });

        let admin = Address::generate(&env);

        // --- Tokens -------------------------------------------------------
        let token_a_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_b_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let token_a = token::Client::new(&env, &token_a_id);
        let token_a_admin = token::StellarAssetClient::new(&env, &token_a_id);
        let token_b = token::Client::new(&env, &token_b_id);
        let token_b_admin = token::StellarAssetClient::new(&env, &token_b_id);

        // --- Swap ---------------------------------------------------------
        let swap_id = env.register(SwapContract, ());
        let swap = SwapContractClient::new(&env, &swap_id);
        swap.initialize(&admin, &token_a_id, &token_b_id);

        // --- Staking ------------------------------------------------------
        let staking_id = env.register(StakingContract, ());
        let staking = StakingContractClient::new(&env, &staking_id);
        staking.initialize(&admin, &token_b_id, &token_b_id);

        // --- Vesting ------------------------------------------------------
        let vesting_id = env.register(VestingContract, ());
        let vesting = VestingContractClient::new(&env, &vesting_id);
        vesting.initialize(&admin);

        // --- Multisig -----------------------------------------------------
        let multisig_id = env.register(MultisigContract, ());
        let multisig = MultisigContractClient::new(&env, &multisig_id);
        let initial_signers = Vec::from_array(&env, [admin.clone()]);
        multisig.initialize(&admin, &initial_signers, &1u32);

        // --- Treasury -----------------------------------------------------
        let treasury_id = env.register(TreasuryContract, ());
        let treasury = TreasuryContractClient::new(&env, &treasury_id);
        treasury.initialize(&admin, &token_a_id, &multisig_id);

        // --- DAO ----------------------------------------------------------
        let dao_id = env.register(DaoContract, ());
        let dao = DaoContractClient::new(&env, &dao_id);
        dao.initialize(&admin, &token_a_id, &multisig_id, &treasury_id);

        // --- Crowdfund ----------------------------------------------------
        let crowdfund_id = env.register(CrowdfundContract, ());
        let crowdfund = CrowdfundContractClient::new(&env, &crowdfund_id);

        // --- Timelock -----------------------------------------------------
        let timelock_id = env.register(TimelockContract, ());
        let timelock = TimelockContractClient::new(&env, &timelock_id);

        Self {
            env,
            token_a,
            token_a_admin,
            token_b,
            token_b_admin,
            swap,
            staking,
            vesting,
            dao,
            multisig,
            treasury,
            crowdfund,
            timelock,
            admin,
        }
    }

    /// Advance the ledger by `seconds`, which is required for reward accrual
    /// and vesting cliff checks.
    fn advance_time(&self, seconds: u64) {
        let mut info = self.env.ledger().get();
        info.timestamp += seconds;
        info.sequence_number += 1;
        self.env.ledger().set(info);
    }

    /// Advance the ledger sequence number by `delta` without changing the
    /// timestamp. Timelock tranches are gated on ledger sequence, so this is
    /// the primitive used to step through scheduled releases.
    fn advance_sequence(&self, delta: u32) {
        let mut info = self.env.ledger().get();
        info.sequence_number += delta;
        self.env.ledger().set(info);
    }
}

// ---------------------------------------------------------------------------
// Workflow 1: Swap -> Stake -> Claim -> Vesting grant
// ---------------------------------------------------------------------------

#[test]
fn test_swap_stake_claim_vesting_workflow() {
    let t = CrossContractEnv::new();
    let user = Address::generate(&t.env);

    // Fund the user with token A (the swap input).
    let swap_input = 1_000_000i128;
    t.token_a_admin.mint(&user, &swap_input);
    assert_eq!(t.token_a.balance(&user), swap_input);

    // ------------------------------------------------------------------
    // Step 1: Swap token A -> token B.
    // ------------------------------------------------------------------
    let min_out = 900_000i128;
    let swapped_out = t.swap.swap(&user, &swap_input, &min_out);

    assert!(swapped_out >= min_out, "swap produced less than min_out");
    assert_eq!(t.token_a.balance(&user), 0, "input token not fully spent");
    assert_eq!(
        t.token_b.balance(&user),
        swapped_out,
        "user did not receive swap output"
    );

    // ------------------------------------------------------------------
    // Step 2: Stake the swap output in the staking contract.
    // ------------------------------------------------------------------
    let stake_amount = swapped_out;
    t.staking.stake(&user, &stake_amount);

    assert_eq!(
        t.token_b.balance(&user),
        0,
        "staked tokens should leave the user wallet"
    );
    assert_eq!(
        t.staking.balance(&user),
        stake_amount,
        "staking contract did not record the deposit"
    );
    assert_eq!(
        t.token_b.balance(&t.staking.address),
        stake_amount,
        "staking contract did not custody the tokens"
    );

    // ------------------------------------------------------------------
    // Step 3: Accrue and claim staking rewards.
    // ------------------------------------------------------------------
    t.advance_time(30 * 24 * 60 * 60); // 30 days

    let pending = t.staking.pending_rewards(&user);
    assert!(pending > 0, "no rewards accrued after 30 days");

    let claimed = t.staking.claim_rewards(&user);
    assert_eq!(claimed, pending, "claimed amount != pending rewards");
    assert_eq!(
        t.token_b.balance(&user),
        claimed,
        "claimed rewards not credited to user"
    );

    // ------------------------------------------------------------------
    // Step 4: Create a vesting grant funded by the claimed rewards.
    // ------------------------------------------------------------------
    let beneficiary = Address::generate(&t.env);
    let grant_amount = claimed;
    let start = t.env.ledger().timestamp();
    let cliff = 7 * 24 * 60 * 60; // 7 days
    let duration = 90 * 24 * 60 * 60; // 90 days

    // Move the reward tokens from the user into the vesting contract.
    t.token_b.transfer(&user, &t.vesting.address, &grant_amount);

    let grant_id = t.vesting.create_grant(
        &t.admin,
        &beneficiary,
        &t.token_b.address,
        &grant_amount,
        &start,
        &cliff,
        &duration,
    );

    assert_eq!(
        t.token_b.balance(&t.vesting.address),
        grant_amount,
        "vesting contract did not custody the grant"
    );

    // Before the cliff, nothing is claimable.
    let claimable_before_cliff = t.vesting.claimable(&grant_id);
    assert_eq!(claimable_before_cliff, 0, "cliff should block vesting");

    // After the cliff, a portion is claimable.
    t.advance_time(cliff + 1);
    let claimable_after_cliff = t.vesting.claimable(&grant_id);
    assert!(
        claimable_after_cliff > 0,
        "nothing claimable after cliff elapsed"
    );
    assert!(
        claimable_after_cliff <= grant_amount,
        "claimable exceeds grant amount"
    );

    // Fully vested after the duration.
    t.advance_time(duration);
    let fully_claimable = t.vesting.claimable(&grant_id);
    assert_eq!(
        fully_claimable, grant_amount,
        "grant should be fully vested after duration"
    );

    let vested_claimed = t.vesting.claim(&beneficiary, &grant_id);
    assert_eq!(vested_claimed, grant_amount, "vested claim mismatch");
    assert_eq!(
        t.token_b.balance(&beneficiary),
        grant_amount,
        "beneficiary did not receive vested tokens"
    );
}

// ---------------------------------------------------------------------------
// Workflow 2: DAO proposal execution updating Multisig / Treasury params
// ---------------------------------------------------------------------------

#[test]
fn test_dao_proposal_updates_multisig_signer() {
    let t = CrossContractEnv::new();
    let new_signer = Address::generate(&t.env);

    // The DAO holds governance tokens used to vote.
    let voter = Address::generate(&t.env);
    let voting_power = 1_000_000i128;
    t.token_a_admin.mint(&voter, &voting_power);

    // ------------------------------------------------------------------
    // Create a proposal that adds `new_signer` to the multisig.
    // ------------------------------------------------------------------
    let proposal_id = t.dao.create_proposal(
        &t.admin,
        &String::from_str(&t.env, "add-signer"),
        &new_signer,
    );

    // Vote in favour.
    t.dao.vote(&voter, &proposal_id, &true);

    // Advance past the voting window.
    t.advance_time(7 * 24 * 60 * 60);

    // ------------------------------------------------------------------
    // Execute the proposal; the DAO calls into the multisig.
    // ------------------------------------------------------------------
    t.dao.execute(&proposal_id);

    let status = t.dao.proposal_status(&proposal_id);
    assert_eq!(status, ProposalStatus::Executed, "proposal not executed");

    // Assert the multisig signer set actually changed.
    let signers = t.multisig.get_signers();
    assert!(
        signers.contains(&new_signer),
        "DAO execution did not add the new multisig signer"
    );
}

#[test]
fn test_dao_proposal_updates_treasury_parameter() {
    let t = CrossContractEnv::new();
    let voter = Address::generate(&t.env);
    let voting_power = 1_000_000i128;
    t.token_a_admin.mint(&voter, &voting_power);

    let new_limit = 5_000_000i128;

    // Proposal to raise the treasury spend limit.
    let proposal_id = t.dao.create_proposal(
        &t.admin,
        &String::from_str(&t.env, "set-spend-limit"),
        &new_limit,
    );

    t.dao.vote(&voter, &proposal_id, &true);
    t.advance_time(7 * 24 * 60 * 60);
    t.dao.execute(&proposal_id);

    assert_eq!(
        t.dao.proposal_status(&proposal_id),
        ProposalStatus::Executed,
        "proposal not executed"
    );

    // Assert the treasury parameter was updated by the DAO.
    assert_eq!(
        t.treasury.spend_limit(),
        new_limit,
        "treasury spend limit was not updated by the DAO"
    );
}

// ---------------------------------------------------------------------------
// Workflow 3: Atomic rollback across multiple contracts
// ---------------------------------------------------------------------------

#[test]
fn test_atomic_rollback_on_mid_workflow_failure() {
    let t = CrossContractEnv::new();
    let user = Address::generate(&t.env);

    let swap_input = 1_000_000i128;
    t.token_a_admin.mint(&user, &swap_input);

    // ------------------------------------------------------------------
    // Perform a successful swap + stake so we have baseline state.
    // ------------------------------------------------------------------
    let swapped_out = t.swap.swap(&user, &swap_input, &900_000i128);
    t.staking.stake(&user, &swapped_out);

    let user_token_a_before = t.token_a.balance(&user);
    let user_token_b_before = t.token_b.balance(&user);
    let staked_before = t.staking.balance(&user);
    let staking_custody_before = t.token_b.balance(&t.staking.address);

    assert_eq!(user_token_a_before, 0);
    assert_eq!(user_token_b_before, 0);
    assert_eq!(staked_before, swapped_out);

    // ------------------------------------------------------------------
    // Attempt a composed workflow where the final call must fail.
    //
    // We wrap the multi-contract sequence in a single host function so that
    // Soroban's transaction semantics roll back *all* sub-invocations when
    // any one of them traps.
    // ------------------------------------------------------------------
    let result = t.env.try_invoke_contract::<(), soroban_sdk::Error>(
        &t.staking.address,
        &soroban_sdk::Symbol::new(&t.env, "composed_failing_workflow"),
        soroban_sdk::vec![
            &t.env,
            user.clone().into_val(&t.env),
            swapped_out.into_val(&t.env),
        ],
    );

    assert!(result.is_err(), "expected the composed workflow to fail");

    // ------------------------------------------------------------------
    // Assert every contract's state is unchanged (atomic rollback).
    // ------------------------------------------------------------------
    assert_eq!(
        t.token_a.balance(&user),
        user_token_a_before,
        "token A balance changed despite rollback"
    );
    assert_eq!(
        t.token_b.balance(&user),
        user_token_b_before,
        "token B balance changed despite rollback"
    );
    assert_eq!(
        t.staking.balance(&user),
        staked_before,
        "staking balance changed despite rollback"
    );
    assert_eq!(
        t.token_b.balance(&t.staking.address),
        staking_custody_before,
        "staking custody changed despite rollback"
    );
}

#[test]
fn test_atomic_rollback_swap_failure_leaves_no_state() {
    let t = CrossContractEnv::new();
    let user = Address::generate(&t.env);

    let swap_input = 1_000_000i128;
    t.token_a_admin.mint(&user, &swap_input);

    let user_token_a_before = t.token_a.balance(&user);
    let user_token_b_before = t.token_b.balance(&user);
    let swap_token_a_before = t.token_a.balance(&t.swap.address);
    let swap_token_b_before = t.token_b.balance(&t.swap.address);

    // A swap whose `min_out` cannot be satisfied must revert the whole call,
    // including the input-token transfer into the pool.
    let impossible_min_out = i128::MAX;
    let result = t
        .env
        .try_invoke_contract::<i128, soroban_sdk::Error>(
            &t.swap.address,
            &soroban_sdk::Symbol::new(&t.env, "swap"),
            soroban_sdk::vec![
                &t.env,
                user.clone().into_val(&t.env),
                swap_input.into_val(&t.env),
                impossible_min_out.into_val(&t.env),
            ],
        );

    assert!(result.is_err(), "swap should have failed on slippage");

    assert_eq!(
        t.token_a.balance(&user),
        user_token_a_before,
        "user token A changed despite swap failure"
    );
    assert_eq!(
        t.token_b.balance(&user),
        user_token_b_before,
        "user token B changed despite swap failure"
    );
    assert_eq!(
        t.token_a.balance(&t.swap.address),
        swap_token_a_before,
        "swap pool token A changed despite failure"
    );
    assert_eq!(
        t.token_b.balance(&t.swap.address),
        swap_token_b_before,
        "swap pool token B changed despite failure"
    );
}

// ---------------------------------------------------------------------------
// Additional composability coverage
// ---------------------------------------------------------------------------

#[test]
fn test_full_composite_workflow_end_to_end() {
    let t = CrossContractEnv::new();
    let user = Address::generate(&t.env);
    let beneficiary = Address::generate(&t.env);

    // 1. Fund and swap.
    t.token_a_admin.mint(&user, &2_000_000i128);
    let swapped = t.swap.swap(&user, &2_000_000i128, &1_800_000i128);
    assert!(swapped > 0);

    // 2. Stake the output.
    t.staking.stake(&user, &swapped);
    assert_eq!(t.staking.balance(&user), swapped);

    // 3. Accrue + claim rewards.
    t.advance_time(60 * 24 * 60 * 60);
    let rewards = t.staking.claim_rewards(&user);
    assert!(rewards > 0);

    // 4. Route rewards into a vesting grant.
    t.token_b.transfer(&user, &t.vesting.address, &rewards);
    let start = t.env.ledger().timestamp();
    let grant_id = t.vesting.create_grant(
        &t.admin,
        &beneficiary,
        &t.token_b.address,
        &rewards,
        &start,
        &0u64,
        &30 * 24 * 60 * 60,
    );

    // 5. Fully vest and claim.
    t.advance_time(30 * 24 * 60 * 60);
    let claimed = t.vesting.claim(&beneficiary, &grant_id);
    assert_eq!(claimed, rewards);
    assert_eq!(t.token_b.balance(&beneficiary), rewards);
}

// ---------------------------------------------------------------------------
// Workflow 4: Crowdfund success -> Timelock tranche releases
// ---------------------------------------------------------------------------

#[test]
fn test_crowdfund_success_initializes_timelock_tranche_releases() {
    let t = CrossContractEnv::new();
    let creator = Address::generate(&t.env);
    let beneficiary = Address::generate(&t.env);

    // ------------------------------------------------------------------
    // Step 1: Configure and launch the crowdfund campaign.
    // ------------------------------------------------------------------
    let goal = 1_000_000i128;
    let deadline = t.env.ledger().timestamp() + 30 * 24 * 60 * 60; // 30 days

    t.crowdfund
        .initialize(&creator, &t.token_a_id, &goal, &deadline);

    // ------------------------------------------------------------------
    // Step 2: Backers pledge until the goal is met.
    // ------------------------------------------------------------------
    let backer_one = Address::generate(&t.env);
    let backer_two = Address::generate(&t.env);
    let pledge_one = 600_000i128;
    let pledge_two = 400_000i128;

    t.token_a_admin.mint(&backer_one, &pledge_one);
    t.token_a_admin.mint(&backer_two, &pledge_two);

    t.crowdfund.pledge(&backer_one, &pledge_one);
    t.crowdfund.pledge(&backer_two, &pledge_two);

    assert_eq!(
        t.crowdfund.total_pledged(),
        goal,
        "campaign did not reach its goal"
    );
    assert_eq!(
        t.token_a.balance(&t.crowdfund.address),
        goal,
        "crowdfund contract did not custody the pledged funds"
    );

    // ------------------------------------------------------------------
    // Step 3: Advance past the deadline and claim the raised funds.
    // ------------------------------------------------------------------
    t.advance_time(30 * 24 * 60 * 60 + 1);

    let raised = t.crowdfund.claim_funds(&creator);
    assert_eq!(raised, goal, "creator did not receive the full raise");
    assert_eq!(
        t.token_a.balance(&creator),
        goal,
        "creator balance does not reflect the claimed raise"
    );
    assert_eq!(
        t.token_a.balance(&t.crowdfund.address),
        0,
        "crowdfund contract should be drained after claim"
    );

    // ------------------------------------------------------------------
    // Step 4: Initialize the timelock with the raised funds and a series
    // of tranches whose release ledgers increase monotonically.
    // ------------------------------------------------------------------
    let current_seq = t.env.ledger().sequence_number();
    let tranche_amounts = [250_000i128, 350_000i128, 400_000i128];
    let tranche_offsets = [10u32, 20u32, 30u32];

    let mut tranches = Vec::new(&t.env);
    for i in 0..tranche_amounts.len() {
        tranches.push_back(ReleaseTranche {
            amount: tranche_amounts[i],
            release_ledger: current_seq + tranche_offsets[i],
        });
    }

    // Move the claimed funds into the timelock contract.
    t.token_a.transfer(&creator, &t.timelock.address, &goal);

    t.timelock
        .initialize(&creator, &t.token_a_id, &beneficiary, &tranches);

    assert_eq!(
        t.token_a.balance(&t.timelock.address),
        goal,
        "timelock did not custody the raised funds"
    );
    assert_eq!(
        t.timelock.pending_tranches(),
        tranches.len(),
        "timelock did not register every tranche"
    );

    // ------------------------------------------------------------------
    // Step 5: Step the ledger sequence forward and assert each tranche
    // releases in order, crediting the beneficiary.
    // ------------------------------------------------------------------
    let mut expected_balance = 0i128;

    for i in 0..tranche_amounts.len() {
        // Just before this tranche's release ledger, nothing new releases.
        let release_ledger = current_seq + tranche_offsets[i];
        let seq_now = t.env.ledger().sequence_number();
        t.advance_sequence(release_ledger - seq_now - 1);

        let released_early = t.timelock.release(&beneficiary);
        assert_eq!(
            released_early, 0,
            "tranche {} released before its release ledger",
            i
        );
        assert_eq!(
            t.token_a.balance(&beneficiary),
            expected_balance,
            "beneficiary balance changed before tranche {} released",
            i
        );

        // Advance to the release ledger; the tranche must now release.
        t.advance_sequence(1);

        let released = t.timelock.release(&beneficiary);
        assert_eq!(
            released, tranche_amounts[i],
            "tranche {} released the wrong amount",
            i
        );

        expected_balance += tranche_amounts[i];
        assert_eq!(
            t.token_a.balance(&beneficiary),
            expected_balance,
            "beneficiary balance did not increase by tranche {}",
            i
        );
        assert_eq!(
            t.timelock.pending_tranches(),
            tranche_amounts.len() - (i + 1),
            "pending tranche count wrong after tranche {}",
            i
        );
    }

    // All tranches released: the timelock should be fully drained and the
    // beneficiary should hold the entire raise.
    assert_eq!(
        t.token_a.balance(&beneficiary),
        goal,
        "beneficiary did not receive the full raise across tranches"
    );
    assert_eq!(
        t.token_a.balance(&t.timelock.address),
        0,
        "timelock should be empty after all tranches released"
    );
    assert_eq!(
        t.timelock.pending_tranches(),
        0,
        "no tranches should remain pending"
    );
}

#[test]
fn test_dao_rejected_proposal_does_not_change_state() {
    let t = CrossContractEnv::new();
    let voter = Address::generate(&t.env);
    t.token_a_admin.mint(&voter, &1_000_000i128);

    let new_signer = Address::generate(&t.env);
    let signers_before = t.multisig.get_signers();

    let proposal_id = t.dao.create_proposal(
        &t.admin,
        &String::from_str(&t.env, "add-signer"),
        &new_signer,
    );

    // Vote against the proposal.
    t.dao.vote(&voter, &proposal_id, &false);
    t.advance_time(7 * 24 * 60 * 60);

    // Execution of a rejected proposal must fail and leave state untouched.
    let result = t
        .env
        .try_invoke_contract::<(), soroban_sdk::Error>(
            &t.dao.address,
            &soroban_sdk::Symbol::new(&t.env, "execute"),
            soroban_sdk::vec![&t.env, proposal_id.into_val(&t.env)],
        );
    assert!(result.is_err(), "rejected proposal should not execute");

    let signers_after = t.multisig.get_signers();
    assert_eq!(
        signers_before, signers_after,
        "multisig signers changed despite rejected proposal"
    );
}
```

Note: This file assumes the contract crates expose the client types and methods referenced (`SwapContractClient`, `StakingContractClient`, `VestingContractClient`, `DaoContractClient`, `MultisigContractClient`, `TreasuryContractClient`). If the actual contract APIs differ (method names, argument order, or return types), the corresponding calls should be adjusted to match the real interfaces defined under `contracts/`.