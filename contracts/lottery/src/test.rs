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
    Address, Bytes, BytesN, Env, MuxedAddress, String, Vec,
    testutils::{Address as _, Ledger as _},
    token::TokenInterface,
};

// ---------------------------------------------------------------------------
// MockToken
// ---------------------------------------------------------------------------

#[contract]
pub struct MockToken;

#[contractimpl]
impl TokenInterface for MockToken {
    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn balance(_env: Env, _id: Address) -> i128 {
        i128::MAX
    }
    fn transfer(_env: Env, _from: Address, _to: MuxedAddress, _amount: i128) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}
    fn burn(_env: Env, _from: Address, _amount: i128) {}
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {}
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> String {
        String::from_str(&env, "Mock")
    }
    fn symbol(env: Env) -> String {
        String::from_str(&env, "MCK")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_commit(env: &Env, secret: &[u8; 32], salt: &[u8; 32]) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.extend_from_array(secret);
    preimage.extend_from_array(salt);
    env.crypto().sha256(&preimage).into()
}

fn single_winner_splits(env: &Env) -> Vec<u32> {
    Vec::from_array(env, [10_000u32])
}

fn two_winner_splits(env: &Env) -> Vec<u32> {
    Vec::from_array(env, [6_000u32, 4_000u32])
}

/// Set up a basic lottery with one winner, no ticket cap, no min_participants.
fn setup(env: &Env) -> (LotteryContractClient, Address, Address) {
    let admin = Address::generate(env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(env, &addr);
    client.initialize(&admin, &token, &100, &1u32, &single_winner_splits(env), &None, &None, &None);
    (client, admin, token)
}

// ---------------------------------------------------------------------------
// Basic lifecycle tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token) = setup(&env);
    let info = client.get_info();
    assert_eq!(info.admin, admin);
    assert_eq!(info.token, token);
    assert_eq!(info.ticket_price, 100);
    assert_eq!(info.state, LotteryState::Open);
    assert_eq!(info.total_tickets, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, token) = setup(&env);
    client.initialize(&admin, &token, &100, &1u32, &single_winner_splits(&env), &None, &None, &None);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_initialize_zero_price_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(&env, &addr);
    client.initialize(&admin, &token, &0, &1u32, &single_winner_splits(&env), &None, &None, &None);
}

#[test]
fn test_buy_ticket() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);
    assert_eq!(client.get_info().total_tickets, 1);
    assert_eq!(client.get_ticket_count(&buyer), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_commit_with_no_tickets_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let secret = [1u8; 32];
    let salt = [2u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));
}

#[test]
fn test_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    client.buy_ticket(&buyer1);
    client.buy_ticket(&buyer2);

    let secret = [42u8; 32];
    let salt = [99u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));
    assert_eq!(client.get_info().state, LotteryState::Committed);

    let secret_bytes: BytesN<32> = BytesN::from_array(&env, &secret);
    let salt_bytes: BytesN<32> = BytesN::from_array(&env, &salt);
    let winners = client.draw(&secret_bytes, &salt_bytes);
    let winner = winners.get(0).unwrap();

    assert!(winner == buyer1 || winner == buyer2);
    assert_eq!(client.get_info().state, LotteryState::Drawn);
    assert_eq!(client.get_winner(), winner);
    assert_eq!(client.get_winners(), winners);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_draw_wrong_preimage_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);

    let secret = [1u8; 32];
    let salt = [2u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let bad_secret: BytesN<32> = BytesN::from_array(&env, &[9u8; 32]);
    let salt_bytes: BytesN<32> = BytesN::from_array(&env, &salt);
    client.draw(&bad_secret, &salt_bytes);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_buy_ticket_after_commit_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);

    let secret = [1u8; 32];
    let salt = [2u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let another = Address::generate(&env);
    client.buy_ticket(&another);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_draw_after_draw_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);

    let secret = [1u8; 32];
    let salt = [2u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let secret_bytes: BytesN<32> = BytesN::from_array(&env, &secret);
    let salt_bytes: BytesN<32> = BytesN::from_array(&env, &salt);
    client.draw(&secret_bytes, &salt_bytes);
    client.draw(&secret_bytes, &salt_bytes);
}

// ---------------------------------------------------------------------------
// #1256 — claim_refund clears TicketCount persistent storage
// ---------------------------------------------------------------------------

#[test]
fn test_claim_refund_clears_ticket_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);

    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);
    assert_eq!(client.get_ticket_count(&buyer), 1);

    let secret = [1u8; 32];
    let salt = [2u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    let deadline = env.ledger().sequence() + 10;
    client.commit(&hash, &deadline);

    // Advance past the reveal deadline.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.claim_refund(&buyer);

    // TicketCount must be cleared after refund (#1256).
    assert_eq!(client.get_ticket_count(&buyer), 0);
    // Participant list must also be empty.
    assert!(client.get_info().participants.is_empty());
}

// ---------------------------------------------------------------------------
// #1257 — Configurable fee (fee_bps + fee_recipient) deducted at draw time
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_fee_bps_without_recipient_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env.register(super::test::MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);
    // fee_bps set but fee_recipient = None should be rejected.
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &Some(500u32),
        &None,
    );
}

#[test]
fn test_fee_config_accepted_and_draw_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token = env.register(super::test::MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &Some(500u32),     // 5% rake
        &Some(fee_recipient),
    );

    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);

    let secret = [7u8; 32];
    let salt = [8u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let secret_bytes = BytesN::from_array(&env, &secret);
    let salt_bytes = BytesN::from_array(&env, &salt);
    let winners = client.draw(&secret_bytes, &salt_bytes);
    assert_eq!(winners.len(), 1);
}
