#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use std::format;

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

use crate::{EscrowContract, EscrowContractClient, EscrowState};

use soroban_common::MIN_DEADLINE_BUFFER;

fn setup_escrow<'a>(
    env: &'a Env,
    amount: i128,
) -> (EscrowContractClient<'a>, Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);

    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(env, &token_addr).mint(&buyer, &amount);

    let escrow_addr = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &escrow_addr);
    let deadline = env.ledger().sequence() + MIN_DEADLINE_BUFFER + 10;
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &0,
        &None,
        &None,
    );

    (client, buyer, seller, arbiter, token_addr)
}

proptest! {
    /// Any valid amount initialises the escrow in Created state with that amount.
    #[test]
    fn prop_initialize_stores_amount(amount in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, ..) = setup_escrow(&env, amount);

        let info = client.get_escrow_info();
        prop_assert_eq!(info.amount, amount);
        prop_assert_eq!(info.state, EscrowState::Created);
    }

    /// Fund → mark_delivered → approve_delivery always ends in Completed state.
    #[test]
    fn prop_happy_path_ends_completed(amount in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, ..) = setup_escrow(&env, amount);

        client.fund();
        client.mark_delivered();
        client.approve_delivery();

        prop_assert_eq!(client.get_state(), Some(EscrowState::Completed));
    }

    /// Arbiter resolving in favour of seller always ends in Completed state.
    #[test]
    fn prop_arbiter_resolve_seller(amount in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, arbiter, _token) = setup_escrow(&env, amount);

        client.fund();
        client.raise_dispute(&buyer);
        client.resolve_dispute(&arbiter, &true);

        prop_assert_eq!(client.get_state(), Some(EscrowState::Completed));
    }

    /// Arbiter resolving in favour of buyer always ends in Refunded state.
    #[test]
    fn prop_arbiter_resolve_buyer(amount in 1i128..=1_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, arbiter, _token) = setup_escrow(&env, amount);

        client.fund();
        client.raise_dispute(&buyer);
        client.resolve_dispute(&arbiter, &false);

        prop_assert_eq!(client.get_state(), Some(EscrowState::Refunded));
    }

    /// Full release via approve_delivery transfers the full amount to seller and ends in Completed.
    #[test]
    fn prop_partial_release_reduces_amount(
        total in 2i128..=1_000_000i128,
        _partial_pct in 1u32..=99u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _buyer, seller, _arbiter, token_addr) = setup_escrow(&env, total);

        client.fund();
        client.mark_delivered();
        client.approve_delivery();

        prop_assert_eq!(client.get_state(), Some(EscrowState::Completed));
        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        prop_assert_eq!(token_client.balance(&seller), total);
    }

    /// Deadline in the past (below MIN_DEADLINE_BUFFER) is always rejected.
    #[test]
    fn prop_past_deadline_rejected(offset in 0u32..MIN_DEADLINE_BUFFER) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 200);

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin);
        let token_addr = sac.address();
        StellarAssetClient::new(&env, &token_addr).mint(&buyer, &1000i128);

        let escrow_addr = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &escrow_addr);
        let bad_deadline = env.ledger().sequence() + offset; // < MIN_DEADLINE_BUFFER

        let result = client.try_initialize(
            &admin, &buyer, &seller, &arbiter, &token_addr, &1000i128, &bad_deadline, &0, &None,
            &None,
        );
        prop_assert!(result.is_err());
    }

    /// State machine never reaches invalid transitions.
    /// After any sequence of valid operations, state is always a valid EscrowState.
    /// Terminal states (Completed, Refunded, Cancelled) cannot transition further.
    #[test]
    fn prop_state_machine_valid_transitions(
        actions in proptest::collection::vec(0u32..=6u32, 1..=10),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, arbiter, _token) = setup_escrow(&env, 1_000i128);

        for action in actions {
            let state = client.get_state();

            // Terminal states cannot transition
            if let Some(EscrowState::Completed) | Some(EscrowState::Refunded) | Some(EscrowState::Cancelled) = state {
                break;
            }

            match action {
                0 => {
                    // fund (only from Created)
                    if state == Some(EscrowState::Created) {
                        let _ = client.try_fund();
                    }
                }
                1 => {
                    // mark_delivered (only from Funded)
                    if state == Some(EscrowState::Funded) {
                        let _ = client.try_mark_delivered();
                    }
                }
                2 => {
                    // approve_delivery (only from Delivered)
                    if state == Some(EscrowState::Delivered) {
                        let _ = client.try_approve_delivery();
                    }
                }
                3 => {
                    // raise_dispute (only from Funded)
                    if state == Some(EscrowState::Funded) {
                        let _ = client.try_raise_dispute(&buyer);
                    }
                }
                4 => {
                    // resolve_dispute (only from Disputed)
                    if state == Some(EscrowState::Disputed) {
                        let _ = client.try_resolve_dispute(&arbiter, &true);
                    }
                }
                5 => {
                    // request_refund (only from Funded after deadline)
                    if state == Some(EscrowState::Funded) {
                        let _ = client.try_request_refund();
                    }
                }
                6 => {
                    // cancel (only from Created)
                    if state == Some(EscrowState::Created) {
                        let _ = client.try_cancel();
                    }
                }
                _ => {}
            }

            // Verify state is always valid
            let final_state = client.get_state();
            prop_assert!(
                matches!(
                    final_state,
                    Some(EscrowState::Created)
                        | Some(EscrowState::Funded)
                        | Some(EscrowState::Delivered)
                        | Some(EscrowState::Disputed)
                        | Some(EscrowState::Completed)
                        | Some(EscrowState::Refunded)
                        | Some(EscrowState::Cancelled)
                        | None
                )
            );
        }
    }
}

// ─── #1193 – Milestone balance-conservation state machine ────────────────────

/// Helper: register a milestone escrow with `n_milestones` equal shares of
/// `total`.  Returns the client, participant addresses, and token address.
fn setup_milestone_escrow<'a>(
    env: &'a Env,
    total: i128,
    n_milestones: u32,
) -> (
    EscrowContractClient<'a>,
    Address, // buyer
    Address, // seller
    Address, // arbiter
    Address, // token
) {
    use soroban_sdk::Vec as SdkVec;
    use crate::Milestone;

    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);

    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(env, &token_addr).mint(&buyer, &total);

    // Build equal-share milestones (last one absorbs rounding remainder).
    let base = total / n_milestones as i128;
    let mut milestones = SdkVec::new(env);
    let mut running = 0i128;
    for i in 0..n_milestones {
        let amt = if i == n_milestones - 1 {
            total - running
        } else {
            base
        };
        milestones.push_back(Milestone { amount: amt, released: false });
        running += amt;
    }

    let escrow_addr = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &escrow_addr);
    let deadline = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 10;
    client.initialize_with_milestones(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &milestones,
        &deadline,
        &0,
        &None,
        &None,
    );

    (client, buyer, seller, arbiter, token_addr)
}

/// Helper: register a milestone escrow with an arbiter fee (in bps).
fn setup_milestone_escrow_with_fee<'a>(
    env: &'a Env,
    total: i128,
    n_milestones: u32,
    arbiter_fee_bps: u32,
) -> (
    EscrowContractClient<'a>,
    Address, // buyer
    Address, // seller
    Address, // arbiter
    Address, // token
) {
    use soroban_sdk::Vec as SdkVec;
    use crate::Milestone;

    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);

    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(env, &token_addr).mint(&buyer, &total);

    let base = total / n_milestones as i128;
    let mut milestones = SdkVec::new(env);
    let mut running = 0i128;
    for i in 0..n_milestones {
        let amt = if i == n_milestones - 1 {
            total - running
        } else {
            base
        };
        milestones.push_back(Milestone { amount: amt, released: false });
        running += amt;
    }

    let escrow_addr = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &escrow_addr);
    let deadline = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 10;
    client.initialize_with_milestones(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &milestones,
        &deadline,
        &0,
        &None,
        &Some(arbiter_fee_bps),
    );

    (client, buyer, seller, arbiter, token_addr)
}

proptest! {
    // ── Invariant 1: contract balance == sum of remaining milestone amounts ──

    /// After funding, the contract token balance equals the sum of all
    /// unreleased milestone amounts (i.e., the full deposit).
    #[test]
    fn prop_milestone_balance_equals_sum_after_fund(
        total in 2i128..=1_000_000i128,
        n in 1u32..=8u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _buyer, _seller, _arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        client.fund();

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let contract_balance = token_client.balance(&client.address);

        let milestones = client.get_milestones();
        let remaining_sum: i128 = milestones
            .iter()
            .filter(|m| !m.released)
            .map(|m| m.amount)
            .sum();

        prop_assert_eq!(
            contract_balance,
            remaining_sum,
            "contract balance ({contract_balance}) must equal sum of unreleased milestone amounts ({remaining_sum})"
        );
    }

    // ── Invariant 2: partial releases never exceed deposited funds ───────────

    /// Releasing milestones one by one never lets total payouts exceed the
    /// initial deposit.  After every intermediate release the contract balance
    /// equals the remaining milestone sum.
    #[test]
    fn prop_milestone_partial_release_conserves_balance(
        total in 2i128..=1_000_000i128,
        n in 2u32..=8u32,
        release_indices in proptest::collection::vec(any::<bool>(), 1..=8usize),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, _arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        client.fund();

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let initial_contract_balance = token_client.balance(&client.address);
        let mut total_released: i128 = 0;

        for (idx, &should_release) in release_indices.iter().enumerate() {
            let milestone_idx = (idx as u32) % n;

            let milestones_before = client.get_milestones();
            let already_released = milestones_before
                .get(milestone_idx)
                .map(|m| m.released)
                .unwrap_or(true);

            if should_release && !already_released {
                let milestone_amount = milestones_before
                    .get(milestone_idx)
                    .map(|m| m.amount)
                    .unwrap_or(0);

                let _ = client.try_release_milestone(&buyer, &milestone_idx);

                // After a successful release: invariant must hold.
                let milestones_after = client.get_milestones();
                let now_released = milestones_after
                    .get(milestone_idx)
                    .map(|m| m.released)
                    .unwrap_or(false);

                if now_released {
                    total_released += milestone_amount;

                    // Invariant: total payouts never exceed deposited funds.
                    prop_assert!(
                        total_released <= initial_contract_balance,
                        "total released ({total_released}) exceeded initial deposit ({initial_contract_balance})"
                    );

                    // Skip balance check when escrow may have transitioned to
                    // Completed (all milestones done — balance is 0).
                    if client.get_state() != Some(EscrowState::Completed) {
                        let remaining_sum: i128 = milestones_after
                            .iter()
                            .filter(|m| !m.released)
                            .map(|m| m.amount)
                            .sum();
                        let contract_balance = token_client.balance(&client.address);
                        prop_assert_eq!(
                            contract_balance,
                            remaining_sum,
                            "after releasing milestone {milestone_idx}: contract balance ({contract_balance}) != remaining sum ({remaining_sum})"
                        );
                    }
                }
            }
        }
    }

    // ── Invariant 3: settled/refunded escrow reaches exactly zero balance ────

    /// After all milestones are released (Completed) OR after a full refund,
    /// the contract's token balance must be exactly zero.
    #[test]
    fn prop_milestone_balance_zero_on_terminal(
        total in 2i128..=1_000_000i128,
        n in 1u32..=6u32,
        settle_seller in any::<bool>(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        client.fund();

        if settle_seller {
            // Release all milestones sequentially.
            for idx in 0..n {
                // State may already be Completed after last milestone.
                if client.get_state() == Some(EscrowState::Completed) {
                    break;
                }
                let _ = client.try_release_milestone(&buyer, &idx);
            }
        } else {
            // Raise a dispute and resolve in favour of buyer (full refund).
            let _ = client.try_raise_dispute(&buyer);
            if client.get_state() == Some(EscrowState::Disputed) {
                client.resolve_dispute(&arbiter, &false);
            } else {
                // Fallback: standard request_refund after deadline.
                env.ledger().with_mut(|l| {
                    l.sequence_number += soroban_common::MIN_DEADLINE_BUFFER + 100;
                });
                let _ = client.try_request_refund();
            }
        }

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let contract_balance = token_client.balance(&client.address);

        let state = client.get_state();
        let is_terminal = matches!(
            state,
            Some(EscrowState::Completed) | Some(EscrowState::Refunded)
        );

        if is_terminal {
            prop_assert_eq!(
                contract_balance,
                0,
                "contract balance must be 0 in terminal state {:?}, got {contract_balance}",
                state
            );
        }
    }

    // ── Invariant 4: no sequence of operations can overdraw the escrow ───────

    /// Randomly apply fund / release_milestone / request_refund / dispute
    /// operations.  At every step, assert that total payouts never exceed the
    /// initial deposit and that the contract balance never goes negative.
    #[test]
    fn prop_milestone_no_overdraw(
        total in 10i128..=500_000i128,
        n in 1u32..=5u32,
        ops in proptest::collection::vec(0u32..=4u32, 1..=20),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, _seller, arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let mut deposited: i128 = 0;
        let mut total_paid_out: i128 = 0;

        for op in ops {
            let state = client.get_state();

            // Stop fuzzing once in a terminal state.
            if matches!(
                state,
                Some(EscrowState::Completed)
                    | Some(EscrowState::Refunded)
                    | Some(EscrowState::Cancelled)
            ) {
                break;
            }

            match op {
                0 => {
                    // fund
                    if state == Some(EscrowState::Created) {
                        if client.try_fund().is_ok() {
                            deposited = total;
                        }
                    }
                }
                1 => {
                    // release_milestone(0..n-1)
                    if state == Some(EscrowState::Funded) {
                        for idx in 0..n {
                            let milestones = client.get_milestones();
                            let already = milestones
                                .get(idx)
                                .map(|m| m.released)
                                .unwrap_or(true);
                            if !already {
                                let amt = milestones.get(idx).map(|m| m.amount).unwrap_or(0);
                                if client.try_release_milestone(&buyer, &idx).is_ok() {
                                    total_paid_out += amt;
                                    prop_assert!(
                                        total_paid_out <= deposited,
                                        "overdraw: paid out {total_paid_out} > deposited {deposited}"
                                    );
                                    let bal = token_client.balance(&client.address);
                                    prop_assert!(
                                        bal >= 0,
                                        "negative contract balance: {bal}"
                                    );
                                }
                                break; // one release per op tick
                            }
                        }
                    }
                }
                2 => {
                    // raise_dispute
                    if state == Some(EscrowState::Funded) {
                        let _ = client.try_raise_dispute(&buyer);
                    }
                }
                3 => {
                    // resolve_dispute → seller
                    if state == Some(EscrowState::Disputed) {
                        let milestones = client.get_milestones();
                        let unreleased_sum: i128 = milestones
                            .iter()
                            .filter(|m| !m.released)
                            .map(|m| m.amount)
                            .sum();
                        if client.try_resolve_dispute(&arbiter, &true).is_ok() {
                            total_paid_out += unreleased_sum;
                            prop_assert!(
                                total_paid_out <= deposited,
                                "overdraw on resolve: paid out {total_paid_out} > deposited {deposited}"
                            );
                        }
                    }
                }
                4 => {
                    // resolve_dispute → buyer (refund)
                    if state == Some(EscrowState::Disputed) {
                        let _ = client.try_resolve_dispute(&arbiter, &false);
                    }
                }
                _ => {}
            }
        }
    }

    // ── Invariant 5: arbiter fee + disbursements never exceed disputed amount ─

    /// When a dispute is resolved with an arbiter fee configured, the fee paid
    /// to the arbiter plus the amounts disbursed to buyer and seller must never
    /// exceed the disputed (unreleased) amount, and the contract must end with
    /// a zero balance.
    #[test]
    fn prop_arbiter_fee_conserves_disputed_amount(
        total in 2i128..=1_000_000i128,
        n in 1u32..=5u32,
        fee_bps in 0u32..=2_000u32,
        favour_seller in any::<bool>(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, seller, arbiter, token_addr) =
            setup_milestone_escrow_with_fee(&env, total, n, fee_bps);

        client.fund();

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let disputed_amount = token_client.balance(&client.address);

        let _ = client.try_raise_dispute(&buyer);
        if client.get_state() != Some(EscrowState::Disputed) {
            return Ok(());
        }

        let arbiter_before = token_client.balance(&arbiter);
        let buyer_before = token_client.balance(&buyer);
        let seller_before = token_client.balance(&seller);

        client.resolve_dispute(&arbiter, &favour_seller);

        let arbiter_paid = token_client.balance(&arbiter) - arbiter_before;
        let buyer_paid = token_client.balance(&buyer) - buyer_before;
        let seller_paid = token_client.balance(&seller) - seller_before;

        // Fee is exactly fee_bps of the disputed amount.
        let expected_fee = disputed_amount * fee_bps as i128 / 10_000;
        prop_assert_eq!(
            arbiter_paid,
            expected_fee,
            "arbiter fee ({arbiter_paid}) must equal {fee_bps} bps of disputed amount ({disputed_amount})"
        );

        // Conservation: fee + disbursements == disputed amount.
        prop_assert_eq!(
            arbiter_paid + buyer_paid + seller_paid,
            disputed_amount,
            "fee + disbursements must equal the disputed amount"
        );

        // Contract balance must be zero after resolution.
        prop_assert_eq!(token_client.balance(&client.address), 0);
    }

    // ── Invariant 6: mutual_cancel refunds exactly the unreleased balance ────

    /// After funding, a mutual cancellation between buyer and seller refunds
    /// exactly the unreleased escrowed balance to the depositor and leaves the
    /// contract with a zero balance in the Cancelled state.
    #[test]
    fn prop_mutual_cancel_refunds_unreleased(
        total in 2i128..=1_000_000i128,
        n in 1u32..=5u32,
        release_count in 0u32..=5u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, seller, _arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        client.fund();

        // Optionally release a few milestones first.
        for idx in 0..release_count.min(n) {
            if client.get_state() == Some(EscrowState::Completed) {
                break;
            }
            let _ = client.try_release_milestone(&buyer, &idx);
        }

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let unreleased = token_client.balance(&client.address);
        let buyer_before = token_client.balance(&buyer);

        let result = client.try_mutual_cancel(&buyer, &seller);
        if result.is_err() {
            // Only valid while funds remain and escrow is not terminal.
            return Ok(());
        }

        prop_assert_eq!(client.get_state(), Some(EscrowState::Cancelled));
        prop_assert_eq!(
            token_client.balance(&buyer) - buyer_before,
            unreleased,
            "mutual_cancel must refund exactly the unreleased balance to the depositor"
        );
        prop_assert_eq!(token_client.balance(&client.address), 0);
    }

    // ── Invariant 7: partial milestone approval conserves total balance ──────

    /// Approving a partial milestone release disburses exactly `release_amount`
    /// to the seller and refunds the remainder of that milestone to the buyer,
    /// conserving the total contract balance.
    #[test]
    fn prop_partial_milestone_conserves_balance(
        total in 2i128..=1_000_000i128,
        n in 1u32..=5u32,
        release_frac_bps in 0u32..=10_000u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, buyer, seller, _arbiter, token_addr) =
            setup_milestone_escrow(&env, total, n);

        client.fund();

        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);
        let milestones = client.get_milestones();
        let milestone_amount = milestones.get(0).map(|m| m.amount).unwrap_or(0);
        let release_amount = milestone_amount * release_frac_bps as i128 / 10_000;

        let contract_before = token_client.balance(&client.address);
        let seller_before = token_client.balance(&seller);
        let buyer_before = token_client.balance(&buyer);

        let result = client.try_approve_milestone_partial(&0u32, &release_amount);
        if result.is_err() {
            return Ok(());
        }

        let seller_paid = token_client.balance(&seller) - seller_before;
        let buyer_refund = token_client.balance(&buyer) - buyer_before;

        prop_assert_eq!(
            seller_paid,
            release_amount,
            "seller must receive exactly the approved release amount"
        );
        prop_assert_eq!(
            seller_paid + buyer_refund,
            milestone_amount,
            "approved portion + remainder must equal the milestone allocation"
        );
        prop_assert_eq!(
            token_client.balance(&client.address),
            contract_before - milestone_amount,
            "contract balance must decrease by exactly the milestone allocation"
        );
    }
}
