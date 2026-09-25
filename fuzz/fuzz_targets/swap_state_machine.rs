#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
};
use soroban_swap_template::SwapContract;
use soroban_token_template::TokenContract;

/// State machine operations for the swap contract fuzz test.
#[derive(Debug, Clone, Copy)]
enum SwapOp {
    Propose,
    Accept,
    Cancel,
}

/// Extract operation from byte.
fn byte_to_op(byte: u8) -> SwapOp {
    match byte % 3 {
        0 => SwapOp::Propose,
        1 => SwapOp::Accept,
        _ => SwapOp::Cancel,
    }
}

/// Convert bytes to i128 amount (always positive for valid amounts).
fn bytes_to_amount(data: &[u8], offset: usize) -> i128 {
    let raw = i128::from_le_bytes([
        data.get(offset).copied().unwrap_or(1),
        data.get(offset + 1).copied().unwrap_or(0),
        data.get(offset + 2).copied().unwrap_or(0),
        data.get(offset + 3).copied().unwrap_or(0),
        data.get(offset + 4).copied().unwrap_or(0),
        data.get(offset + 5).copied().unwrap_or(0),
        data.get(offset + 6).copied().unwrap_or(0),
        data.get(offset + 7).copied().unwrap_or(0),
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ]);
    // Ensure positive amount
    raw.abs().max(1)
}

/// Convert bytes to u32.
fn bytes_to_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data.get(offset).copied().unwrap_or(0),
        data.get(offset + 1).copied().unwrap_or(0),
        data.get(offset + 2).copied().unwrap_or(0),
        data.get(offset + 3).copied().unwrap_or(0),
    ])
}

fuzz_target!(|data: &[u8]| {
    // Minimum data: at least one operation byte
    if data.is_empty() {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    // Setup: register two tokens
    let token_admin = Address::generate(&env);

    let token_a_addr = env.register_contract(None, TokenContract);
    let token_a = soroban_token_template::TokenContractClient::new(&env, &token_a_addr);
    let _ = token_a.try_initialize(
        &token_admin,
        &String::from_str(&env, "Token A"),
        &String::from_str(&env, "TKA"),
        &18u32,
        &None,
    );

    let token_b_addr = env.register_contract(None, TokenContract);
    let token_b = soroban_token_template::TokenContractClient::new(&env, &token_b_addr);
    let _ = token_b.try_initialize(
        &token_admin,
        &String::from_str(&env, "Token B"),
        &String::from_str(&env, "TKB"),
        &18u32,
        &None,
    );

    // Register swap contract
    let swap_addr = env.register_contract(None, SwapContract);
    let swap = soroban_swap_template::SwapContractClient::new(&env, &swap_addr);
    let swap_admin = Address::generate(&env);
    let swap_treasury = Address::generate(&env);
    let _ = swap.try_initialize(&swap_admin, &swap_treasury, &0u32);

    // Create a pool of addresses
    let pool = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];

    // Mint initial balances for all parties
    for addr in &pool {
        let _ = token_a.try_mint(addr, &1_000_000_000);
        let _ = token_b.try_mint(addr, &1_000_000_000);
    }

    // Track proposed swap IDs
    let mut proposed_swaps: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut data_offset = 0;

    // Execute operations from fuzz input
    while data_offset < data.len() {
        if data_offset + 20 > data.len() {
            break;
        }

        let op = byte_to_op(data[data_offset]);
        data_offset += 1;

        match op {
            SwapOp::Propose => {
                // Extract parameters for propose
                let party_a_idx = (data[data_offset] as usize) % pool.len();
                let party_a = pool[party_a_idx].clone();
                data_offset += 1;

                let amount_a = bytes_to_amount(data, data_offset);
                data_offset += 8;

                let amount_b = bytes_to_amount(data, data_offset);
                data_offset += 8;

                let deadline_offset = bytes_to_u32(data, data_offset);
                data_offset += 4;

                // Set ledger sequence and calculate future deadline
                env.ledger().set_sequence_number(1000);
                let deadline = 1000 + (deadline_offset % 10000) + 1;

                // Try to propose swap
                let result = swap.try_propose_swap(
                    &party_a,
                    &token_a_addr,
                    &amount_a,
                    &token_b_addr,
                    &amount_b,
                    &deadline,
                );

                if let Ok(Ok(swap_id)) = result {
                    proposed_swaps.push(swap_id);

                    // Invariant: swap_count should increase
                    let count = swap.swap_count();
                    assert!(count > 0, "swap_count should be > 0 after propose");

                    // Invariant: can retrieve the swap
                    let swap_info = swap.try_get_swap(&swap_id);
                    assert!(
                        swap_info.is_ok(),
                        "should be able to get swap after propose"
                    );
                }
            }

            SwapOp::Accept => {
                if proposed_swaps.is_empty() {
                    data_offset += 2; // Skip bytes
                    continue;
                }

                let swap_idx = (data[data_offset] as usize) % proposed_swaps.len();
                let swap_id = proposed_swaps[swap_idx];
                data_offset += 1;

                let party_b_idx = (data[data_offset] as usize) % pool.len();
                let party_b = pool[party_b_idx].clone();
                data_offset += 1;

                // Try to accept
                let result = swap.try_accept_swap(&swap_id, &party_b);

                if result.is_ok() {
                    // Invariant: swap state should be Completed
                    if let Ok(Ok(swap_info)) = swap.try_get_swap(&swap_id) {
                        assert_eq!(
                            swap_info.state,
                            soroban_swap_template::SwapState::Executed,
                            "swap should be in Executed state after accept"
                        );
                    }

                    // Invariant: cannot accept again (double-accept protection)
                    let second_accept = swap.try_accept_swap(&swap_id, &party_b);
                    assert!(
                        second_accept.is_err(),
                        "should not be able to accept a swap twice"
                    );
                }
            }

            SwapOp::Cancel => {
                if proposed_swaps.is_empty() {
                    data_offset += 1; // Skip byte
                    continue;
                }

                let swap_idx = (data[data_offset] as usize) % proposed_swaps.len();
                let swap_id = proposed_swaps[swap_idx];
                data_offset += 1;

                // Try to cancel
                let result = swap.try_cancel_swap(&swap_id);

                if result.is_ok() {
                    // Invariant: swap state should be Cancelled
                    if let Ok(Ok(swap_info)) = swap.try_get_swap(&swap_id) {
                        assert_eq!(
                            swap_info.state,
                            soroban_swap_template::SwapState::Cancelled,
                            "swap should be in Cancelled state after cancel"
                        );
                    }

                    // Invariant: cannot cancel again
                    let second_cancel = swap.try_cancel_swap(&swap_id);
                    assert!(
                        second_cancel.is_err(),
                        "should not be able to cancel a swap twice"
                    );

                    // Invariant: cannot accept after cancel
                    let party_b = pool[0].clone();
                    let accept_after_cancel = swap.try_accept_swap(&swap_id, &party_b);
                    assert!(
                        accept_after_cancel.is_err(),
                        "should not be able to accept a cancelled swap"
                    );
                }
            }
        }

        // Invariant: all token balances should be non-negative
        for addr in &pool {
            if let Ok(Ok(balance_a)) = token_a.try_balance(addr) {
                assert!(balance_a >= 0, "token A balance should never be negative");
            }
            if let Ok(Ok(balance_b)) = token_b.try_balance(addr) {
                assert!(balance_b >= 0, "token B balance should never be negative");
            }
        }
    }
});
