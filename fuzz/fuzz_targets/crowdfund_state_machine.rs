#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_crowdfund_template::{CrowdfundContract, CrowdfundContractClient};
use soroban_sdk::{Address, Env, Vec as SdkVec, testutils::Address as _, token::StellarAssetClient};

/// State machine operations for crowdfund fuzz test
/// Closes #1172 – crowdfund state machine fuzz target for pledge conservation
#[derive(Debug, Clone, Copy)]
enum CrowdfundOp {
    Pledge,
    Withdraw,
    Refund,
    Claim,
    CancelCampaign,
}

fn byte_to_op(byte: u8) -> CrowdfundOp {
    match byte % 5 {
        0 => CrowdfundOp::Pledge,
        1 => CrowdfundOp::Withdraw,
        2 => CrowdfundOp::Refund,
        3 => CrowdfundOp::Claim,
        _ => CrowdfundOp::CancelCampaign,
    }
}

fn bytes_to_i128(data: &[u8], offset: usize) -> i128 {
    if offset + 8 > data.len() {
        return 100;
    }
    let raw = i64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ]);
    (raw.abs() as i128).max(1).min(100_000)
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 20 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 100);

    let creator = Address::generate(&env);
    let sac_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(sac_admin);
    let token_addr = sac.address();

    // Fuzz goal (bytes 0-7) and deadline offset (bytes 8-9)
    let goal = bytes_to_i128(data, 0);
    let deadline_offset = u16::from_le_bytes([data[8], data[9]]) as u32;
    let deadline = env.ledger().sequence() + deadline_offset.max(10).min(5000);

    // Deploy crowdfund contract
    let crowdfund_addr = env.register_contract(None, CrowdfundContract);
    let client = CrowdfundContractClient::new(&env, &crowdfund_addr);

    let init_result = client.try_initialize(
        &creator,
        &token_addr,
        &goal,
        &deadline,
        &SdkVec::new(&env),
        &None,
    );

    if init_result.is_err() {
        return;
    }

    // Create 5 pledgers
    let pledgers: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for pledger in &pledgers {
        StellarAssetClient::new(&env, &token_addr).mint(pledger, &1_000_000);
    }

    // Track pledges locally for verification
    let mut local_pledges: std::collections::HashMap<usize, i128> = std::collections::HashMap::new();
    let mut total_pledged_local = 0i128;

    // Track initial balances
    let initial_contract_balance =
        soroban_sdk::token::Client::new(&env, &token_addr).balance(&crowdfund_addr);

    // Execute operations from fuzz input
    let mut offset = 10;
    let mut claimed = false;
    let mut cancelled = false;

    while offset < data.len() {
        if offset + 2 >= data.len() {
            break;
        }

        let op = byte_to_op(data[offset]);
        let pledger_idx = (data[offset + 1] % 5) as usize;
        offset += 2;

        let pledger = &pledgers[pledger_idx];

        match op {
            CrowdfundOp::Pledge => {
                if offset + 8 > data.len() {
                    break;
                }
                let amount = bytes_to_i128(data, offset);
                offset += 8;

                let result = client.try_pledge(pledger, &amount, &None);
                if result.is_ok() {
                    *local_pledges.entry(pledger_idx).or_insert(0) += amount;
                    total_pledged_local += amount;
                }
            }

            CrowdfundOp::Withdraw => {
                let result = client.try_withdraw(pledger);
                if result.is_ok() {
                    if let Some(pledge) = local_pledges.get(&pledger_idx) {
                        total_pledged_local -= pledge;
                    }
                    local_pledges.remove(&pledger_idx);
                }
            }

            CrowdfundOp::Refund => {
                let result = client.try_refund(pledger);
                if result.is_ok() {
                    local_pledges.remove(&pledger_idx);
                    claimed = false;
                }
            }

            CrowdfundOp::Claim => {
                let result = client.try_claim();
                if result.is_ok() {
                    claimed = true;
                }
            }

            CrowdfundOp::CancelCampaign => {
                let result = client.try_cancel_campaign();
                if result.is_ok() {
                    cancelled = true;
                }
            }
        }

        // Invariants after each operation
        let info_result = client.try_get_info();
        if let Ok(info) = info_result {
            let contract_balance =
                soroban_sdk::token::Client::new(&env, &token_addr).balance(&crowdfund_addr);

            // Invariant 1: Contract token balance == total_pledged while active
            if !claimed && !cancelled {
                let expected_balance = initial_contract_balance + info.total_pledged;
                assert!(
                    contract_balance == expected_balance,
                    "Balance mismatch: contract={}, expected={}, total_pledged={}",
                    contract_balance,
                    expected_balance,
                    info.total_pledged
                );
            }

            // Invariant 2: If goal met and past deadline, creator can claim exact total
            if info.total_pledged >= goal && env.ledger().sequence() > deadline && !claimed {
                let creator_balance_before =
                    soroban_sdk::token::Client::new(&env, &token_addr).balance(&creator);
                let claim_result = client.try_claim();
                if claim_result.is_ok() {
                    let creator_balance_after =
                        soroban_sdk::token::Client::new(&env, &token_addr).balance(&creator);
                    let claimed_amount = creator_balance_after - creator_balance_before;
                    assert!(
                        claimed_amount == info.total_pledged,
                        "Claim amount mismatch: claimed={}, expected={}",
                        claimed_amount,
                        info.total_pledged
                    );
                    claimed = true;
                }
            }

            // Invariant 3: If goal not met post-deadline or cancelled, 
            // sum of all refunds equals total_pledged
            if ((info.total_pledged < goal && env.ledger().sequence() > deadline) || cancelled)
                && !claimed
            {
                let mut total_refundable = 0i128;
                for (idx, pledger) in pledgers.iter().enumerate() {
                    let pledge_amount = client.get_pledge(pledger);
                    total_refundable += pledge_amount;

                    // Verify local tracking matches on-chain
                    if let Some(local_pledge) = local_pledges.get(&idx) {
                        assert!(
                            pledge_amount == *local_pledge,
                            "Pledge mismatch for pledger {}: on-chain={}, local={}",
                            idx,
                            pledge_amount,
                            local_pledge
                        );
                    }
                }

                assert!(
                    total_refundable == info.total_pledged,
                    "Refundable sum mismatch: refundable={}, total_pledged={}",
                    total_refundable,
                    info.total_pledged
                );
            }

            // Invariant 4: No double refunds - after refund, pledge must be zero
            for pledger in &pledgers {
                let pledge_before = client.get_pledge(pledger);
                if pledge_before > 0 && (cancelled || env.ledger().sequence() > deadline) {
                    let refund_result = client.try_refund(pledger);
                    if refund_result.is_ok() {
                        let pledge_after = client.get_pledge(pledger);
                        assert!(
                            pledge_after == 0,
                            "Double refund possible: pledge after refund = {}",
                            pledge_after
                        );
                    }
                }
            }

            // Invariant 5: If goal met, contributors cannot withdraw
            if info.total_pledged >= goal && env.ledger().sequence() <= deadline {
                for pledger in &pledgers {
                    if client.get_pledge(pledger) > 0 {
                        let withdraw_result = client.try_withdraw(pledger);
                        assert!(
                            withdraw_result.is_err(),
                            "Withdraw succeeded after goal met"
                        );
                    }
                }
            }

            // Invariant 6: Cancelled campaigns allow immediate refunds
            if cancelled {
                assert!(
                    info.cancelled,
                    "Campaign not marked as cancelled"
                );
            }
        }

        // Invariant 7: All balances remain non-negative
        let contract_balance =
            soroban_sdk::token::Client::new(&env, &token_addr).balance(&crowdfund_addr);
        assert!(
            contract_balance >= 0,
            "Contract balance went negative: {}",
            contract_balance
        );

        for pledger in &pledgers {
            let balance = soroban_sdk::token::Client::new(&env, &token_addr).balance(pledger);
            assert!(
                balance >= 0,
                "Pledger balance went negative: {}",
                balance
            );
        }
    }
});
