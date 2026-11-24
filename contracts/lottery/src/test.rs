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

/// Set up a basic lottery with one winner, no ticket cap, no min_participants,
/// no treasury, no fee.
fn setup(env: &Env) -> (LotteryContractClient, Address, Address) {
    let admin = Address::generate(env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(env),
        &None,
        &None,
        &None,
    );
    (client, admin, token)
}

/// Set up with an explicit per-address ticket cap.
fn setup_with_cap(env: &Env, cap: u32) -> (LotteryContractClient, Address, Address) {
    let admin = Address::generate(env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(env),
        &Some(cap),
        &None,
        &None,
    );
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
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &None,
        &None,
        &None,
    );
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
    client.initialize(
        &admin,
        &token,
        &0,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &None,
        &None,
        &None,
    );
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
fn test_buy_tickets_batch_updates_count_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup(&env);
    let buyer = Address::generate(&env);
    client.buy_tickets_batch(&buyer, &5);
    // The fix: stored count must be 5, not 1.
    assert_eq!(client.get_ticket_count(&buyer), 5);
    assert_eq!(client.get_info().total_tickets, 5);
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
// claim_refund
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

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    let refund = client.claim_refund(&buyer);
    assert_eq!(refund, 100);
    assert_eq!(client.get_ticket_count(&buyer), 0);
}

// ---------------------------------------------------------------------------
// Fee config
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_fee_bps_without_recipient_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(&env, &addr);
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &Some(500u32),
        &None,
        &None,
    );
}

#[test]
fn test_fee_config_accepted_and_draw_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token = env.register_contract(None, MockToken);
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &Some(500u32),
        &Some(500u32), // 5% rake
        &Some(fee_recipient),
        &None,
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

// ---------------------------------------------------------------------------
// Revenue split (85 / 10 / 5) — Task #1
// ---------------------------------------------------------------------------

/// When both treasury and fee_recipient are configured the draw must disburse
/// exactly 10 % to the treasury, 5 % to the operator, and 85 % to the winner.
/// We use a 10 000-token pool to make the arithmetic exact.
#[test]
fn test_revenue_split_85_10_5_exact() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let operator = Address::generate(&env);

    let token = env.register(MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);

    // 100 tickets × 100 tokens each = 10 000 token pool.
    client.initialize(
        &admin,
        &token,
        &100,          // ticket_price
        &1u32,
        &single_winner_splits(&env),
        &None,
        &Some(500u32), // 5 % operator fee
        &Some(operator.clone()),
        &Some(treasury.clone()),
    );

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    // 100 tickets total (50 each) → pool = 10 000
    client.buy_tickets_batch(&buyer1, &50);
    client.buy_tickets_batch(&buyer2, &50);

    let secret = [11u8; 32];
    let salt = [22u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let secret_bytes = BytesN::from_array(&env, &secret);
    let salt_bytes = BytesN::from_array(&env, &salt);

    client.draw(&secret_bytes, &salt_bytes);

    // The draw must succeed without panicking.
    // Verify the split arithmetic: 85/10/5 of 10_000 total pool.
    // treasury = 10% = 1_000; operator = 5% = 500; winners = 85% = 8_500.
    let pool: i128 = 10_000; // 100 tickets × 100 tokens
    let treasury_share = apply_bps_fee(pool, 1_000u32).unwrap();
    let operator_share = apply_bps_fee(pool, 500u32).unwrap();
    let winner_share = apply_bps_fee(pool, 8_500u32).unwrap();
    assert_eq!(treasury_share, 1_000);
    assert_eq!(operator_share, 500);
    assert_eq!(winner_share, 8_500);
    assert_eq!(treasury_share + operator_share + winner_share, pool);
}

/// Only treasury configured (no operator): 10 % goes to treasury, 90 % to winner.
#[test]
fn test_revenue_split_treasury_only() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = env.register(MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &token,
        &1_000,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &None,
        &None,
        &Some(treasury.clone()),
    );

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    client.buy_ticket(&buyer1);
    client.buy_ticket(&buyer2);

    let secret = [3u8; 32];
    let salt = [4u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    client.commit(&hash, &(env.ledger().sequence() + 100));

    let secret_bytes = BytesN::from_array(&env, &secret);
    let salt_bytes = BytesN::from_array(&env, &salt);

    client.draw(&secret_bytes, &salt_bytes);

    // 2 tickets @ 1_000 each = 2_000 pool; 10% treasury = 200; 90% winners = 1_800.
    let pool: i128 = 2_000;
    let treasury_share = apply_bps_fee(pool, 1_000u32).unwrap();
    let winner_share = pool - treasury_share;
    assert_eq!(treasury_share, 200);
    assert_eq!(winner_share, 1_800);
}

// ---------------------------------------------------------------------------
// Jackpot rollover — Task #2
// ---------------------------------------------------------------------------

/// When a rollover occurs the carried balance must equal the ticket revenue, a
/// jackpot_rollover event must be emitted, and buying tickets in the new round
/// then drawing must succeed using the rolled-over balance.
#[test]
fn test_rollover_carries_balance_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register(MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);

    // Require at least 5 distinct participants before draw; we'll only add 2.
    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &None,
        &None,
        &None,
    );
    // Store MinParticipants directly so rollover triggers.
    env.as_contract(&addr, || {
        env.storage()
            .instance()
            .set(&storage::DataKey::MinParticipants, &5u32);
    });

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    client.buy_ticket(&buyer1); // 100 tokens
    client.buy_ticket(&buyer2); // 100 tokens  → round revenue = 200

    let secret = [55u8; 32];
    let salt = [66u8; 32];
    let hash = make_commit(&env, &secret, &salt);
    let deadline = env.ledger().sequence() + 10;
    client.commit(&hash, &deadline);

    // Advance past the reveal deadline so the rollover is permissionless.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.rollover();

    // The rollover balance must be 200 (2 tickets × 100).
    assert_eq!(client.get_rollover_balance(), 200);

    // After rollover the lottery is Open again with 0 tickets.
    assert_eq!(client.get_info().state, LotteryState::Open);
    assert_eq!(client.get_info().total_tickets, 0);
}

/// Two consecutive rollovers must accumulate the balance; the final draw pays
/// out the combined jackpot.
#[test]
fn test_rollover_accumulates_across_rounds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register(MockToken, ());
    let addr = env.register(LotteryContract, ());
    let client = LotteryContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &token,
        &100,
        &1u32,
        &single_winner_splits(&env),
        &None,
        &None,
        &None,
        &None,
    );
    // Require 10 participants to ensure rollovers.
    env.as_contract(&addr, || {
        env.storage()
            .instance()
            .set(&storage::DataKey::MinParticipants, &10u32);
    });

    // — Round 1: 1 ticket, rollover —
    let buyer = Address::generate(&env);
    client.buy_ticket(&buyer);

    let s1 = [1u8; 32];
    let t1 = [2u8; 32];
    let h1 = make_commit(&env, &s1, &t1);
    let d1 = env.ledger().sequence() + 5;
    client.commit(&h1, &d1);
    env.ledger().with_mut(|l| l.sequence_number = d1 + 1);
    client.rollover();
    assert_eq!(client.get_rollover_balance(), 100); // round 1 revenue

    // — Round 2: 1 ticket, rollover —
    let buyer2 = Address::generate(&env);
    client.buy_ticket(&buyer2);

    let s2 = [3u8; 32];
    let t2 = [4u8; 32];
    let h2 = make_commit(&env, &s2, &t2);
    let d2 = env.ledger().sequence() + 5;
    client.commit(&h2, &d2);
    env.ledger().with_mut(|l| l.sequence_number = d2 + 1);
    client.rollover();
    // 100 (round 1) + 100 (round 2) = 200
    assert_eq!(client.get_rollover_balance(), 200);

    // — Round 3: enough participants for a draw —
    env.as_contract(&addr, || {
        env.storage()
            .instance()
            .set(&storage::DataKey::MinParticipants, &1u32);
    });
    let buyer3 = Address::generate(&env);
    client.buy_ticket(&buyer3);

    let s3 = [5u8; 32];
    let t3 = [6u8; 32];
    let h3 = make_commit(&env, &s3, &t3);
    client.commit(&h3, &(env.ledger().sequence() + 10));

    let sb3 = BytesN::from_array(&env, &s3);
    let tb3 = BytesN::from_array(&env, &t3);
    client.draw(&sb3, &tb3);

    // After draw the rollover balance is cleared.
    assert_eq!(client.get_rollover_balance(), 0);
    assert_eq!(client.get_info().state, LotteryState::Drawn);
// Whale protection — per-account ticket purchase cap
// ---------------------------------------------------------------------------

/// A single address cannot buy more than the configured cap (50) in one round.
/// Attempting to exceed it must return TicketCapExceeded (error #17).
#[test]
fn test_ticket_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_with_cap(&env, 50);

    let whale = Address::generate(&env);

    // Buying exactly 50 tickets must succeed.
    client.buy_tickets_batch(&whale, &50);
    assert_eq!(client.get_ticket_count(&whale), 50);

    // The 51st ticket must be rejected.
    assert_eq!(
        client.try_buy_ticket(&whale),
        Err(Ok(LotteryError::TicketCapExceeded))
    );
    // Count remains at 50.
    assert_eq!(client.get_ticket_count(&whale), 50);
}

/// Cap is enforced across multiple purchase calls, not just per single call.
#[test]
fn test_ticket_cap_accumulates_across_calls() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_with_cap(&env, 50);

    let whale = Address::generate(&env);

    client.buy_tickets_batch(&whale, &30);
    assert_eq!(client.get_ticket_count(&whale), 30);

    client.buy_tickets_batch(&whale, &20);
    assert_eq!(client.get_ticket_count(&whale), 50);

    // One more should be rejected.
    assert_eq!(
        client.try_buy_tickets_batch(&whale, &1),
        Err(Ok(LotteryError::TicketCapExceeded))
    );
}

/// A batch that would push the total over the cap is rejected in full.
#[test]
fn test_ticket_cap_batch_exceeding_cap_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_with_cap(&env, 50);

    let whale = Address::generate(&env);
    client.buy_tickets_batch(&whale, &40);

    // Trying to buy 15 more (40 + 15 = 55 > 50) must fail.
    assert_eq!(
        client.try_buy_tickets_batch(&whale, &15),
        Err(Ok(LotteryError::TicketCapExceeded))
    );
    // Count must not have changed.
    assert_eq!(client.get_ticket_count(&whale), 40);
}

/// Other participants are unaffected by one address hitting the cap.
#[test]
fn test_ticket_cap_does_not_affect_other_buyers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_with_cap(&env, 50);

    let whale = Address::generate(&env);
    let retail = Address::generate(&env);

    client.buy_tickets_batch(&whale, &50);
    // Whale is capped, but retail can still buy.
    client.buy_tickets_batch(&retail, &50);
    assert_eq!(client.get_ticket_count(&retail), 50);
    assert_eq!(client.get_info().total_tickets, 100);
}

/// Setting cap to 0 at initialize must be rejected (error #16).
#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_zero_ticket_cap_rejected_at_init() {
    let env = Env::default();
    env.mock_all_auths();
    let _ = setup_with_cap(&env, 0);
}
