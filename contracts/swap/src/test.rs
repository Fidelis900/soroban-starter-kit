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
    Address, Env, Vec,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

fn register_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin);
    sac.address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    let token_admin = Address::generate(env);
    let _ = token_admin;
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn setup(env: &Env) -> (SwapContractClient, Address, Address, Address, Address, Address) {
    let token_a = register_token(env);
    let token_b = register_token(env);
    let party_a = Address::generate(env);
    let party_b = Address::generate(env);
    let treasury = Address::generate(env);

    mint(env, &token_a, &party_a, 100_000);
    mint(env, &token_b, &party_b, 100_000);

    let addr = env.register_contract(None, SwapContract);
    let client = SwapContractClient::new(env, &addr);
    client.initialize(&party_a, &treasury, &50); // 0.5% fee
    let approve_until = env.ledger().sequence() + 1_000_000;
    TokenClient::new(env, &token_a).approve(&party_a, &addr, &1_000_000_000, &approve_until);

    (client, party_a, party_b, token_a, token_b, treasury)
}

#[test]
fn test_propose_and_accept_full_swap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, treasury) = setup(&env);

    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap(&party_a, &token_a, &1_000, &token_b, &500, &expires_at);
    client.accept_swap(&swap_id, &party_b);
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    assert_eq!(swap_id, 0);
    assert_eq!(client.swap_count(), 1);

    let swap = client.get_swap(&0);
    assert_eq!(swap.party_a, party_a);
    assert_eq!(swap.state, SwapState::Pending);
    assert_eq!(swap.amount_a, 1_000);
    assert_eq!(swap.amount_b, 500);
    assert_eq!(swap.allowed_counterparty, None);
    assert_eq!(swap.max_execution_delay, None);
}

    let swap = client.get_swap(&swap_id);
    assert_eq!(swap.state, SwapState::Executed);
    assert_eq!(swap.filled_amount, 1_000);

    let token_a_client = TokenClient::new(&env, &token_a);
    let token_b_client = TokenClient::new(&env, &token_b);
    assert_eq!(token_a_client.balance(&party_b), 1_000);
    assert_eq!(token_b_client.balance(&treasury), 2); // 500 * 50 / 10000 = 2
    client.propose_swap(
        &party_a,
        &token_a,
        &0,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
}

#[test]
fn test_partial_fill_multiple_takes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);

    let (client, party_a, _, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &100,
        &None,
        &None,
    );
}

#[test]
fn test_accept_swap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap_with_options(
        &party_a, &token_a, &1_000, &token_b, &500, &expires_at, &true, &false,
    );

    client.accept_swap_partial(&party_b, &swap_id, &400);
    let swap = client.get_swap(&swap_id);
    assert_eq!(swap.state, SwapState::Pending);
    assert_eq!(swap.filled_amount, 400);
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    client.accept_swap(&swap_id, &party_b);

    client.accept_swap_partial(&party_b, &swap_id, &600);
    let swap = client.get_swap(&swap_id);
    assert_eq!(swap.state, SwapState::Executed);
    assert_eq!(swap.filled_amount, 1_000);
    assert_eq!(swap.state, SwapState::Accepted);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_partial_fill_rejected_when_not_enabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);

    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap(&party_a, &token_a, &1_000, &token_b, &500, &expires_at);
    client.accept_swap_partial(&party_b, &swap_id, &500);
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);
    client.accept_swap(&swap_id, &party_b);
}

#[test]
fn test_cancel_swap_by_party_a() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;

    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    client.cancel_swap(&swap_id);

    assert_eq!(client.get_swap(&swap_id).state, SwapState::Cancelled);
}

#[test]
fn test_escrowed_cancel_returns_unfilled_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);
    let token_a_client = TokenClient::new(&env, &token_a);
    let contract_addr = client.address.clone();

    let balance_before = token_a_client.balance(&party_a);
    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap_with_options(
        &party_a, &token_a, &1_000, &token_b, &500, &expires_at, &true, &true,
    );
    assert_eq!(token_a_client.balance(&contract_addr), 1_000);
    assert_eq!(token_a_client.balance(&party_a), balance_before - 1_000);
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);
    client.cancel_swap(&swap_id);

    client.accept_swap_partial(&party_b, &swap_id, &400);
    assert_eq!(token_a_client.balance(&contract_addr), 600);

    client.cancel_swap(&swap_id);
    assert_eq!(token_a_client.balance(&contract_addr), 0);
    assert_eq!(token_a_client.balance(&party_a), balance_before - 400);
#[test]
fn test_proposer_reclaims_escrowed_assets_after_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 10;

    let token_a_client = token::Client::new(&env, &token_a);
    let initial_balance = token_a_client.balance(&party_a);

    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );

    let contract_address = client.address.clone();
    assert_eq!(token_a_client.balance(&contract_address), 1000);
    assert_eq!(token_a_client.balance(&party_a), initial_balance - 1000);

    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);

    client.cancel_swap(&swap_id);

    assert_eq!(token_a_client.balance(&contract_address), 0);
    assert_eq!(token_a_client.balance(&party_a), initial_balance);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Cancelled);
}

#[test]
fn test_escrowed_accept_transfers_from_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);
    let token_a_client = TokenClient::new(&env, &token_a);
    let contract_addr = client.address.clone();

    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap_with_options(
        &party_a, &token_a, &900, &token_b, &450, &expires_at, &false, &true,
    );
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    client.accept_swap(&swap_id, &party_b);
    client.accept_swap(&swap_id, &party_b);

    assert_eq!(token_a_client.balance(&contract_addr), 0);
    assert_eq!(token_a_client.balance(&party_b), 900);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Executed);
}

#[test]
fn test_basket_swap_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);
    let token_c = register_token(&env);
    let token_d = register_token(&env);

    mint(&env, &token_c, &party_a, 2_000);
    mint(&env, &token_d, &party_b, 2_000);
    let approve_until = env.ledger().sequence() + 1_000_000;
    TokenClient::new(&env, &token_c).approve(&party_a, &client.address, &1_000_000_000, &approve_until);

    let offers = Vec::from_array(
        &env,
        [
            BasketLeg {
                token: token_a.clone(),
                amount: 500,
            },
            BasketLeg {
                token: token_c.clone(),
                amount: 700,
            },
        ],
    );
    let demands = Vec::from_array(
        &env,
        [
            BasketLeg {
                token: token_b.clone(),
                amount: 300,
            },
            BasketLeg {
                token: token_d.clone(),
                amount: 400,
            },
        ],
    );
    let expires_at = env.ledger().sequence() + 100;
    let basket_id = client.propose_basket_swap(&party_a, &offers, &demands, &expires_at);
    client.accept_basket_swap(&basket_id, &party_b);

    let info = client.get_basket_swap(&basket_id);
    assert_eq!(info.state, SwapState::Executed);
    assert_eq!(TokenClient::new(&env, &token_c).balance(&party_b), 700);
    assert_eq!(TokenClient::new(&env, &token_d).balance(&party_a), 400);
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
    client.cancel_swap(&swap_id);
    client.cancel_swap(&swap_id);
}

#[test]
fn test_basket_swap_atomic_failure_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b, _treasury) = setup(&env);

    let offers = Vec::from_array(
        &env,
        [BasketLeg {
            token: token_a.clone(),
            amount: 500,
        }],
    );
    let demands = Vec::from_array(
        &env,
        [BasketLeg {
            token: token_b.clone(),
            amount: 200_000, // impossible for party_b
        }],
    );
    let expires_at = env.ledger().sequence() + 100;
    let basket_id = client.propose_basket_swap(&party_a, &offers, &demands, &expires_at);

    let party_a_before = TokenClient::new(&env, &token_b).balance(&party_a);
    let party_b_before = TokenClient::new(&env, &token_a).balance(&party_b);
    let result = client.try_accept_basket_swap(&basket_id, &party_b);
    assert!(result.is_err());

    // No partial transfers should persist.
    assert_eq!(TokenClient::new(&env, &token_b).balance(&party_a), party_a_before);
    assert_eq!(TokenClient::new(&env, &token_a).balance(&party_b), party_b_before);
    assert_eq!(client.get_basket_swap(&basket_id).state, SwapState::Pending);
    let (client, party_a, _, _, _) = setup(&env);
    let treasury = Address::generate(&env);
    let fee_bps = 50;

    client.initialize(&party_a, &treasury, &fee_bps);

    assert_eq!(client.get_admin(), party_a);
    assert_eq!(client.get_treasury(), treasury);
    assert_eq!(client.get_fee_bps(), 50);
}

#[test]
fn test_fee_deducted_when_swap_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);

    let treasury = Address::generate(&env);
    client.initialize(&party_a, &treasury, &50);

    let token_b_client = token::Client::new(&env, &token_b);
    let contract_address = client.address.clone();

    let initial_party_b_balance = token_b_client.balance(&party_b);
    let initial_party_a_balance = token_b_client.balance(&party_a);
    let initial_treasury_balance = token_b_client.balance(&treasury);

    let expires_at = env.ledger().sequence() + 100;
    let swap_amount_b = 500;
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &swap_amount_b,
        &expires_at,
        &None,
        &None,
    );

    client.accept_swap(&swap_id, &party_b);

    let fee = (swap_amount_b * 50) / 10_000; // 2
    let party_a_receives = swap_amount_b - fee; // 498

    assert_eq!(
        token_b_client.balance(&party_b),
        initial_party_b_balance - swap_amount_b
    );
    assert_eq!(
        token_b_client.balance(&party_a),
        initial_party_a_balance + party_a_receives
    );
    assert_eq!(
        token_b_client.balance(&treasury),
        initial_treasury_balance + fee
    );
    assert_eq!(token_b_client.balance(&contract_address), 0);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_non_admin_cannot_update_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _party_b, _, _) = setup(&env);
    let initial_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.initialize(&party_a, &initial_treasury, &50);

    env.set_auths(&[]);
    client.set_treasury(&new_treasury);
}

#[test]
fn test_admin_can_update_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, _, _) = setup(&env);
    let initial_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    let new_admin = Address::generate(&env);

    client.initialize(&party_a, &initial_treasury, &50);

    client.set_treasury(&new_treasury);
    assert_eq!(client.get_treasury(), new_treasury);

    client.set_fee_bps(&100);
    assert_eq!(client.get_fee_bps(), 100);

    client.set_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_cannot_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, _, _) = setup(&env);
    let treasury = Address::generate(&env);

    client.initialize(&party_a, &treasury, &50);
    client.initialize(&party_a, &treasury, &50);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_cannot_set_invalid_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, _, _) = setup(&env);
    let treasury = Address::generate(&env);

    client.initialize(&party_a, &treasury, &10100);
}

#[test]
fn test_cancel_after_expiry_without_party_a_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _party_b, token_a, token_b, _treasury) = setup(&env);
    let expires_at = env.ledger().sequence() + 5;
    let swap_id = client.propose_swap(&party_a, &token_a, &1000, &token_b, &500, &expires_at);
    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);
    client.cancel_swap(&swap_id);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Cancelled);
}
    let (client, party_a, _, token_a, token_b) = setup(&env);
    let expires_at = env.ledger().sequence() + 100;

    client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );
}

#[test]
fn test_multiple_swaps_increment_id() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;

    let id0 = client.propose_swap(
        &party_a,
        &token_a,
        &100,
        &token_b,
        &50,
        &expires_at,
        &None,
        &None,
    );
    let id1 = client.propose_swap(
        &party_a,
        &token_a,
        &200,
        &token_b,
        &100,
        &expires_at,
        &None,
        &None,
    );
    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
    assert_eq!(client.swap_count(), 2);
}

// ---------------------------------------------------------------------------
// Issue #1077: Counterparty restriction tests
// ---------------------------------------------------------------------------

#[test]
fn test_counterparty_restriction_allowed_taker_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;

    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &Some(party_b.clone()),
        &None,
    );

    let swap = client.get_swap(&swap_id);
    assert_eq!(swap.allowed_counterparty, Some(party_b.clone()));

    // Designated taker accepts successfully
    client.accept_swap(&swap_id, &party_b);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Accepted);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_counterparty_restriction_unauthorized_taker_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    let unauthorized_taker = Address::generate(&env);
    mint(&env, &token_b, &unauthorized_taker, 10_000);

    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;

    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &Some(party_b),
        &None,
    );

    // Unauthorized taker attempts acceptance -> SwapError::NotAuthorized (#1)
    client.accept_swap(&swap_id, &unauthorized_taker);
}

#[test]
fn test_open_taker_allows_any_counterparty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, _, token_a, token_b) = setup(&env);
    let random_taker = Address::generate(&env);
    mint(&env, &token_b, &random_taker, 10_000);

    client.initialize(&party_a, &Address::generate(&env), &0);
    let expires_at = env.ledger().sequence() + 100;

    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &expires_at,
        &None,
        &None,
    );

    client.accept_swap(&swap_id, &random_taker);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Accepted);
}

// ---------------------------------------------------------------------------
// Issue #1079: Treasury fee rounding audit and minimum fee tests
// ---------------------------------------------------------------------------

#[test]
fn test_minimum_fee_enforced_on_small_micro_swaps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    let treasury = Address::generate(&env);

    // 10 bps = 0.1%. For amount_b = 5: 5 * 10 / 10_000 = 0 in standard int division.
    // Minimum non-zero fee policy ensures fee = 1.
    client.initialize(&party_a, &treasury, &10);

    let token_b_client = token::Client::new(&env, &token_b);
    let initial_treasury_balance = token_b_client.balance(&treasury);
    let initial_party_a_balance = token_b_client.balance(&party_a);

    let expires_at = env.ledger().sequence() + 100;
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &10,
        &token_b,
        &5,
        &expires_at,
        &None,
        &None,
    );

    client.accept_swap(&swap_id, &party_b);

    assert_eq!(token_b_client.balance(&treasury), initial_treasury_balance + 1);
    assert_eq!(token_b_client.balance(&party_a), initial_party_a_balance + 4);
}

#[test]
fn test_fee_calculation_across_different_decimal_scales() {
    // 7-decimal XLM scale (e.g. 100 XLM = 1_000_000_000 stroops)
    let amount_xlm = 1_000_000_000i128;
    let fee_xlm = calculate_and_validate_fee(amount_xlm, 50).unwrap(); // 0.5% = 5_000_000
    assert_eq!(fee_xlm, 5_000_000);

    // 2-decimal USDC scale (e.g. 100 USDC = 10_000 cents)
    let amount_usdc = 10_000i128;
    let fee_usdc = calculate_and_validate_fee(amount_usdc, 50).unwrap(); // 0.5% = 50 cents
    assert_eq!(fee_usdc, 50);

    // Small USDC trade (5 cents) where 0.5% = 0.025 cents -> minimum fee = 1 cent
    let small_usdc = 5i128;
    let fee_small = calculate_and_validate_fee(small_usdc, 50).unwrap();
    assert_eq!(fee_small, 1);
}

// ---------------------------------------------------------------------------
// Issue #1080: Pagination tests
// ---------------------------------------------------------------------------

#[test]
fn test_pagination_active_swaps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);

    let base_ledger = 100;
    env.ledger().with_mut(|l| l.sequence_number = base_ledger);

    // Create 5 swaps:
    // id 0: active (expires 200)
    // id 1: active -> accepted (expires 200)
    // id 2: active (expires 200)
    // id 3: expired (expires 105)
    // id 4: active (expires 200)
    let id0 = client.propose_swap(&party_a, &token_a, &100, &token_b, &100, &200, &None, &None);
    let id1 = client.propose_swap(&party_a, &token_a, &200, &token_b, &200, &200, &None, &None);
    let id2 = client.propose_swap(&party_a, &token_a, &300, &token_b, &300, &200, &None, &None);
    let id3 = client.propose_swap(&party_a, &token_a, &400, &token_b, &400, &105, &None, &None);
    let id4 = client.propose_swap(&party_a, &token_a, &500, &token_b, &500, &200, &None, &None);

    // Accept id 1
    client.accept_swap(&id1, &party_b);

    // Advance ledger past id3 expiry
    env.ledger().with_mut(|l| l.sequence_number = 110);

    // Active pending non-expired swaps are: id0, id2, id4
    let page1 = client.get_active_swaps(&0, &2);
    assert_eq!(page1.swaps.len(), 2);
    assert_eq!(page1.swaps.get(0).unwrap().id, id0);
    assert_eq!(page1.swaps.get(1).unwrap().id, id2);
    assert_eq!(page1.next_cursor, Some(3));

    // Page 2 resuming from cursor 3
    let page2 = client.get_active_swaps(&page1.next_cursor.unwrap(), &2);
    assert_eq!(page2.swaps.len(), 1);
    assert_eq!(page2.swaps.get(0).unwrap().id, id4);
    assert_eq!(page2.next_cursor, None);
}

// ---------------------------------------------------------------------------
// Issue #1081: Front-running protection and execution delay tests
// ---------------------------------------------------------------------------

#[test]
fn test_execution_delay_within_window_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);

    env.ledger().with_mut(|l| l.sequence_number = 100);

    // Max execution delay of 20 ledgers
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &500,
        &None,
        &Some(20),
    );

    // Advance 15 ledgers (ledger 115 <= 100 + 20)
    env.ledger().with_mut(|l| l.sequence_number = 115);

    client.accept_swap(&swap_id, &party_b);
    assert_eq!(client.get_swap(&swap_id).state, SwapState::Accepted);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_execution_delay_exceeded_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, party_a, party_b, token_a, token_b) = setup(&env);
    client.initialize(&party_a, &Address::generate(&env), &0);

    env.ledger().with_mut(|l| l.sequence_number = 100);

    // Max execution delay of 10 ledgers
    let swap_id = client.propose_swap(
        &party_a,
        &token_a,
        &1_000,
        &token_b,
        &500,
        &500,
        &None,
        &Some(10),
    );

    // Advance 11 ledgers (ledger 111 > 100 + 10) -> SwapError::ExecutionDelayExceeded (#13)
    env.ledger().with_mut(|l| l.sequence_number = 111);

    client.accept_swap(&swap_id, &party_b);
}
