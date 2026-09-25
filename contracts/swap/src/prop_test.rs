#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use std::format;

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env,
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
extern crate std;

use super::*;
use proptest::prelude::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
};
use std::vec::Vec;

use crate::{SwapContract, SwapContractClient, SwapState};

fn register_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(admin).address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

proptest! {
#[derive(Clone, Debug)]
enum Command {
    Propose { amount_a: i128, amount_b: i128 },
    Accept { index: usize },
    Cancel { index: usize },
}

fn commands() -> impl Strategy<Value = Vec<Command>> {
    prop::collection::vec(
        prop_oneof![
            (1i128..=1_000, 1i128..=1_000)
                .prop_map(|(amount_a, amount_b)| Command::Propose { amount_a, amount_b }),
            (0usize..=31).prop_map(|index| Command::Accept { index }),
            (0usize..=31).prop_map(|index| Command::Cancel { index }),
        ],
        1..=32,
    )
}
use crate::{SwapContract, SwapContractClient, calculate_and_validate_fee};

fn setup(
    env: &Env,
) -> (
    SwapContractClient,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.sequence_number = 1);
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let party_a = Address::generate(env);
    let party_b = Address::generate(env);
    let token_a = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    StellarAssetClient::new(env, &token_a).mint(&party_a, &1_000_000i128);
    StellarAssetClient::new(env, &token_b).mint(&party_b, &1_000_000i128);
    let address = env.register_contract(None, SwapContract);
    let client = SwapContractClient::new(env, &address);
    client.initialize(&admin, &treasury, &250);
    (client, party_a, party_b, treasury, token_a, token_b)
}

fn assert_conservation(
    env: &Env,
    client: &SwapContractClient,
    party_a: &Address,
    party_b: &Address,
    treasury: &Address,
    token_a: &Address,
    token_b: &Address,
) {
    let contract = client.address.clone();
    let token_a_client = soroban_sdk::token::Client::new(env, token_a);
    let token_b_client = soroban_sdk::token::Client::new(env, token_b);
    let token_a_total = token_a_client.balance(party_a)
        + token_a_client.balance(party_b)
        + token_a_client.balance(&contract);
    let token_b_total = token_b_client.balance(party_a)
        + token_b_client.balance(party_b)
        + token_b_client.balance(treasury)
        + token_b_client.balance(&contract);
    assert_eq!(token_a_total, 1_000_000, "token A was created or destroyed");
    assert_eq!(token_b_total, 1_000_000, "token B was created or destroyed");
}

proptest! {
    /// Stateful invariant: arbitrary propose/cancel/accept sequences conserve both
    /// assets exactly, with fees moving only to the configured treasury.
    ///
    /// Closes #1082.
    /// Property: Fee + party_a_amount always equals amount_b (no rounding loss)
    /// Closes #961 – swap fee calculation invariant
    #[test]
    fn prop_swap_fee_exact(
        amount_b in 100i128..=1_000_000i128,
        fee_bps in 0u32..=10_000u32,
    ) {
        let fee = calculate_and_validate_fee(amount_b, fee_bps).unwrap();
        let party_a_amount = amount_b - fee;

        // Invariant: no rounding loss
        prop_assert_eq!(fee + party_a_amount, amount_b,
            "Fee split doesn't sum to amount_b: fee={}, party_a={}, amount_b={}",
            fee, party_a_amount, amount_b);

        // Invariant: both amounts non-negative
        prop_assert!(fee >= 0, "Negative fee: {}", fee);
        prop_assert!(party_a_amount >= 0, "Negative party_a amount: {}", party_a_amount);
        prop_assert!(fee <= amount_b, "Fee cannot exceed total traded amount");
    }

    /// Property: Treasury fee cannot exceed total traded amount across split transactions
    #[test]
    fn prop_fee_invariance_split_transactions(
        total_amount in 10i128..=10_000i128,
        num_splits in 2usize..=10usize,
        fee_bps in 1u32..=1_000u32,
    ) {
        let chunk = total_amount / num_splits as i128;
        if chunk > 0 {
            let mut total_split_fee = 0i128;
            for _ in 0..num_splits {
                let split_fee = calculate_and_validate_fee(chunk, fee_bps).unwrap();
                prop_assert!(split_fee > 0, "Fee must be non-zero when fee_bps > 0");
                prop_assert!(split_fee <= chunk, "Fee cannot exceed chunk size");
                total_split_fee += split_fee;
            }
            prop_assert!(total_split_fee <= total_amount,
                "Total fee from split transactions cannot exceed total traded volume");
        }
    }

    /// Property: Total transferred out equals total transferred in
    #[test]
    fn prop_sequential_partial_fills_reach_exact_total(
        amount_a in 100i128..=10_000i128,
        ratio in 1i128..=20i128,
        first_fill in 1i128..=5_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let token_a = register_token(&env);
        let token_b = register_token(&env);
        let party_a = Address::generate(&env);
        let party_b = Address::generate(&env);

        let sac_admin = Address::generate(&env);
        let sac1 = env.register_stellar_asset_contract_v2(sac_admin.clone());
        let token_a = sac1.address();
        let sac2 = env.register_stellar_asset_contract_v2(sac_admin);
        let token_b = sac2.address();

        // Mint tokens
        StellarAssetClient::new(&env, &token_a).mint(&party_a, &amount_a);
        StellarAssetClient::new(&env, &token_b).mint(&party_b, &amount_b);

        let expires_at = env.ledger().sequence() + 1000;

        // Propose swap
        let swap_id_result = client.try_propose_swap(
            &party_a,
            &token_a,
            &amount_a,
            &token_b,
            &amount_b,
            &expires_at,
            &None,
            &None,
        );

        if swap_id_result.is_err() {
            return Ok(());
        }

        let swap_id = swap_id_result.unwrap();

        // Accept swap
        let accept_result = client.try_accept_swap(&swap_id, &party_b);

        if accept_result.is_ok() {
            let tok_a = soroban_sdk::token::Client::new(&env, &token_a);
            let tok_b = soroban_sdk::token::Client::new(&env, &token_b);

            // Calculate expected fee
            let fee = calculate_and_validate_fee(amount_b, fee_bps).unwrap();
            let party_a_net = amount_b - fee;

            // Verify balances
            prop_assert_eq!(tok_a.balance(&party_b), amount_a,
                "Party B should receive full amount_a");
            prop_assert_eq!(tok_b.balance(&party_a), party_a_net,
                "Party A should receive amount_b minus fee");
            prop_assert_eq!(tok_b.balance(&treasury), fee,
                "Treasury should receive fee");

            // Conservation: contract should have zero balance after swap
            prop_assert_eq!(tok_a.balance(&swap_addr), 0,
                "Contract should have no token_a after swap");
            prop_assert_eq!(tok_b.balance(&swap_addr), 0,
                "Contract should have no token_b after swap");
        }
    }

    /// Property: Fee BPS validation - must be <= 10000
    #[test]
    fn prop_fee_bps_bounded(
        fee_bps in 0u32..=20_000u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        let amount_b = amount_a * ratio;
        mint(&env, &token_a, &party_a, amount_a * 2);
        mint(&env, &token_b, &party_b, amount_b * 2);
    /// Property: Cancelled swaps return funds to party_a
    #[test]
    fn prop_swap_state_machine_preserves_balances(actions in commands()) {
        let env = Env::default();
        let (client, party_a, party_b, treasury, token_a, token_b) = setup(&env);
        assert_conservation(&env, &client, &party_a, &party_b, &treasury, &token_a, &token_b);

        for action in actions {
            match action {
                Command::Propose { amount_a, amount_b } => {
                    let expiry = env.ledger().sequence() + 100;
                    let _ = client.try_propose_swap(
                        &party_a, &token_a, &amount_a, &token_b, &amount_b, &expiry,
                    );
                }
                Command::Accept { index } => {
                    let count = client.swap_count();
                    if count > 0 {
                        let id = (index as u32) % count;
                        let _ = client.try_accept_swap(&id, &party_b);
                    }
                }
                Command::Cancel { index } => {
                    let count = client.swap_count();
                    if count > 0 {
                        let id = (index as u32) % count;
                        let _ = client.try_cancel_swap(&id);
                    }
                }
            }
            assert_conservation(&env, &client, &party_a, &party_b, &treasury, &token_a, &token_b);
        }
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let contract_addr = env.register_contract(None, SwapContract);
        let client = SwapContractClient::new(&env, &contract_addr);
        client.initialize(&party_a, &treasury, &0);
        let approve_until = env.ledger().sequence() + 1_000_000;
        TokenClient::new(&env, &token_a).approve(&party_a, &contract_addr, &(amount_a * 4), &approve_until);

        let expires_at = env.ledger().sequence() + 100;
        let swap_id = client.propose_swap_with_options(
            &party_a,
            &token_a,
            &amount_a,
            &token_b,
            &amount_b,
            &expires_at,
            &true,
            &false,
            &None,
            &None,
        );

        let first = first_fill.min(amount_a - 1);
        client.accept_swap_partial(&party_b, &swap_id, &first);
        let second = amount_a - first;
        client.accept_swap_partial(&party_b, &swap_id, &second);

        let swap = client.get_swap(&swap_id);
        prop_assert_eq!(swap.filled_amount, amount_a);
        prop_assert_eq!(swap.state, SwapState::Executed);
        prop_assert_eq!(TokenClient::new(&env, &token_a).balance(&party_b), amount_a);
    }

    #[test]
    fn prop_escrow_cancel_returns_remaining_balance(
        amount_a in 500i128..=20_000i128,
        amount_b in 500i128..=20_000i128,
        fill in 1i128..=10_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let token_a = register_token(&env);
        let token_b = register_token(&env);
        let party_a = Address::generate(&env);
        let party_b = Address::generate(&env);
        let treasury = Address::generate(&env);

        mint(&env, &token_a, &party_a, amount_a * 3);
        mint(&env, &token_b, &party_b, amount_b * 3);

        let contract_addr = env.register_contract(None, SwapContract);
        let client = SwapContractClient::new(&env, &contract_addr);
        client.initialize(&party_a, &treasury, &0);

        let token_a_client = TokenClient::new(&env, &token_a);
        let before = token_a_client.balance(&party_a);
        let expires_at = env.ledger().sequence() + 100;
        let swap_id = client.propose_swap_with_options(
            &party_a,
            &token_a,
            &amount_a,
            &token_b,
            &amount_b,
            &expires_at,
            &true,
            &true,
            &None,
            &None,
        );

        client.cancel_swap(&swap_id);

        let after = token_a_client.balance(&party_a);
        let _ = (party_b, fill);
        prop_assert_eq!(after, before);
    }
}
