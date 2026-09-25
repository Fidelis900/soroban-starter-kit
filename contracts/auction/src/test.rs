#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_nft_template::{NftContract, NftContractClient};
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

fn setup(env: &Env) -> (AuctionContractClient, Address, Address, Address, Address) {
    let seller = Address::generate(env);
    let bidder1 = Address::generate(env);
    let bidder2 = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(seller.clone());
    let token = sac.address();
    StellarAssetClient::new(env, &token).mint(&bidder1, &100_000);
    StellarAssetClient::new(env, &token).mint(&bidder2, &100_000);

    let addr = env.register_contract(None, AuctionContract);
    let client = AuctionContractClient::new(env, &addr);

    (client, seller, bidder1, bidder2, token)
}

// ---------------------------------------------------------------------------
// Happy-path / overbid scenario
// ---------------------------------------------------------------------------

#[test]
fn test_single_bid_and_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    // no reserve, no extension window
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_500);
    assert_eq!(client.get_info().highest_bid, 1_500);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    assert!(client.get_info().settled);
}

#[test]
fn test_overbid_refunds_previous_bidder() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200); // overbids b1

    // b1 should have a pending refund of 1_000
    assert_eq!(client.get_pending(&b1), 1_000);
    assert_eq!(client.get_info().highest_bid, 1_200);
    use soroban_sdk::{IntoVal, Symbol};
    let contract_address = client.address.clone();
    let all_events = env.events().all();
    assert!(all_events.contains(&(
        contract_address.clone(),
        (Symbol::new(&env, "outbid"), b1.clone()).into_val(&env),
        (1_000i128, 1_200i128).into_val(&env),
    )));
    assert!(all_events.contains(&(
        contract_address,
        (Symbol::new(&env, "refund_queued"), b1.clone()).into_val(&env),
        1_000i128.into_val(&env),
    )));

    // b1 withdraws refund
    client.withdraw(&b1);
    assert_eq!(client.get_pending(&b1), 0);
}

#[test]
fn test_multiple_overbids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &500, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &500,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_500);
    client.bid(&b1, &2_000);

    // b2 is outbid; b2 pending = 1_500
    assert_eq!(client.get_pending(&b2), 1_500);
    assert_eq!(client.get_info().highest_bid, 2_000);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();
    assert!(client.get_info().settled);
}

// ---------------------------------------------------------------------------
// Deadline scenario
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_bid_after_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.bid(&b1, &1_500);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_end_before_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    client.bid(&b1, &1_500);
    client.end(); // deadline not reached
}

// ---------------------------------------------------------------------------
// No-bids scenario
// ---------------------------------------------------------------------------

#[test]
fn test_end_with_no_bids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    let info = client.get_info();
    assert!(info.settled);
    assert!(info.highest_bidder.is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_bid_too_low_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &500, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &500,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200); // needs >= 1_500 (1000 + 500)
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_double_settle_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 10;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    client.bid(&b1, &1_000);
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();
    client.end();
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_start_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
}

// ---------------------------------------------------------------------------
// Reserve price — issue #783
// ---------------------------------------------------------------------------

/// Reserve is met: seller receives the winning bid.
#[test]
fn test_reserve_met_settles_to_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    // reserve = 2_000, bid = 2_500 → reserve met
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &Some(2_000i128),
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &2_500);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    let info = client.get_info();
    assert!(info.settled);
    assert_eq!(info.reserve_price, Some(2_000));
    // Bidder has no pending refund — the auction settled normally
    assert_eq!(client.get_pending(&b1), 0);
}

/// Reserve is NOT met: highest bidder gets funds back, item unsold.
#[test]
fn test_reserve_not_met_returns_funds_to_bidder() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    use soroban_sdk::token::Client as TokenClient;
    let before = TokenClient::new(&env, &token).balance(&b1);

    let deadline = env.ledger().sequence() + 100;
    // reserve = 5_000, bid = 1_500 → reserve NOT met
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &Some(5_000i128),
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_500);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    let info = client.get_info();
    assert!(info.settled);
    // Bidder's balance restored (contract transferred back directly)
    assert_eq!(TokenClient::new(&env, &token).balance(&b1), before);
}

/// No reserve set behaves identically to the original contract.
#[test]
fn test_no_reserve_settles_any_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    assert!(client.get_info().settled);
    assert_eq!(client.get_info().reserve_price, None);
}

// ---------------------------------------------------------------------------
// Issue #784 — Anti-sniping time extension
// ---------------------------------------------------------------------------

/// A bid placed outside the extension window must NOT extend the deadline.
#[test]
fn test_no_extension_when_bid_is_early() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    // Start at ledger 0, deadline = 100, window = 10
    let deadline = env.ledger().sequence() + 100;
    let window: u32 = 10;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &window,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    // Bid at ledger 5 — well outside the 10-ledger window; deadline stays 100
    env.ledger().with_mut(|l| l.sequence_number = 5);
    client.bid(&b1, &1_000);

    let info = client.get_info();
    assert_eq!(info.deadline, deadline, "deadline should not have changed");
    assert_eq!(info.extension_window, window);
}

/// A bid placed within the extension window must extend the deadline.
#[test]
fn test_deadline_extended_when_bid_is_near_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    let window: u32 = 10;
    // Advance ledger to deadline - window exactly (right on the boundary)
    env.ledger()
        .with_mut(|l| l.sequence_number = deadline - window);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &window,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    // Bid at the same ledger — within the window; deadline should be extended
    client.bid(&b1, &1_000);

    let info = client.get_info();
    assert_eq!(
        info.deadline,
        deadline + window,
        "deadline should be extended by the window"
    );
}

/// Verify that only near-deadline bids trigger extension (multiple bids scenario).
#[test]
fn test_only_near_deadline_bid_extends() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline: u32 = 100;
    let window: u32 = 5;
    // Start at ledger 0
    client.start(
        &seller,
        &token,
        &1_000,
        &500,
        &deadline,
        &None,
        &window,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    // First bid at ledger 50 — early, no extension
    env.ledger().with_mut(|l| l.sequence_number = 50);
    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().deadline, deadline);

    // Second bid at ledger 97 — within 5-ledger window, should extend
    env.ledger().with_mut(|l| l.sequence_number = 97);
    client.bid(&b2, &1_500);
    assert_eq!(client.get_info().deadline, deadline + window);
}

/// Anti-sniping extensions are clamped to `max_deadline` and stop once it is
/// reached, so bidders cannot postpone settlement indefinitely.
#[test]
fn test_extensions_halt_at_max_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline: u32 = 100;
    let window: u32 = 10;
    let max_deadline: u32 = 115;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &window,
        &max_deadline,
        &0,
        &0,
        &None,
        &None,
    );

    // First extension: 100 -> 110.
    env.ledger().with_mut(|l| l.sequence_number = 95);
    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().deadline, 110);

    // Second extension is clamped: 110 -> 115 (not 120).
    env.ledger().with_mut(|l| l.sequence_number = 105);
    client.bid(&b2, &1_100);
    assert_eq!(client.get_info().deadline, max_deadline);

    // Further near-deadline bids no longer extend.
    env.ledger().with_mut(|l| l.sequence_number = 114);
    client.bid(&b1, &1_200);
    let info = client.get_info();
    assert_eq!(info.deadline, max_deadline);
    assert_eq!(info.max_deadline, max_deadline);

    // Past the cap, bidding is closed.
    env.ledger().with_mut(|l| l.sequence_number = max_deadline + 1);
    assert_eq!(
        client.try_bid(&b2, &1_300),
        Err(Ok(AuctionError::AuctionEnded))
    );
}

/// `max_deadline` below `deadline` is rejected at start.
#[test]
fn test_start_rejects_max_deadline_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);

    let deadline: u32 = 100;
    assert_eq!(
        client.try_start(
            &seller,
            &token,
            &1_000,
            &100,
            &deadline,
            &None,
            &10,
            &(deadline - 1),
            &0,
            &0,
            &None,
            &None,
        ),
        Err(Ok(AuctionError::InvalidDeadline))
    );
}

// ---------------------------------------------------------------------------
// Issue #785 — Cancel before first bid
// ---------------------------------------------------------------------------

/// Seller can cancel before any bid is placed.
#[test]
fn test_cancel_succeeds_before_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    // No bids; cancel should succeed
    client.cancel(&seller);

    // Attempting to bid on a cancelled auction should fail with AuctionEnded (#3)
    let result = client.try_bid(&Address::generate(&env), &1_000);
    assert!(result.is_err());
}

/// Cancel is rejected once a bid has been placed.
#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_cancel_fails_after_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);

    // Should panic with BidAlreadyPlaced (#13)
    client.cancel(&seller);
}

/// Cancel is rejected if called by a non-seller.
#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_cancel_fails_for_non_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0, &0, &0);
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    // b1 is not the seller — should fail with NotAuthorized (#8)
    client.cancel(&b1);
}

// ---------------------------------------------------------------------------
// Issue #1072 — Seller cancellation grace window & bidder compensation
// ---------------------------------------------------------------------------

const GRACE_START: u32 = 10;
const GRACE_LEDGERS: u32 = 20;
const CANCEL_FEE: i128 = 250;

/// Start an auction at ledger `GRACE_START` with a `GRACE_LEDGERS` grace window
/// and `fee` compensation, and fund the seller so they can pay it.
fn start_with_grace(
    env: &Env,
    client: &AuctionContractClient,
    seller: &Address,
    token: &Address,
    fee: i128,
) {
    env.ledger().with_mut(|l| l.sequence_number = GRACE_START);
    let deadline = GRACE_START + 100;
    client.start(
        seller,
        token,
// Shared helpers for issues #1068–#1071
// ---------------------------------------------------------------------------

/// Register the repo's NFT template and mint one token to `owner`.
fn setup_nft<'a>(env: &'a Env, owner: &Address) -> (NftContractClient<'a>, Address, u32) {
    let admin = Address::generate(env);
    let addr = env.register_contract(None, NftContract);
    let nft = NftContractClient::new(env, &addr);
    nft.initialize(
        &admin,
        &String::from_str(env, "Auction Lots"),
        &String::from_str(env, "LOT"),
        &None,
        &None,
        &None,
    );
    let token_id = nft.mint(owner, &String::from_str(env, "ipfs://lot-1"), &None, &None);
    (nft, addr, token_id)
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(who)
}

// ---------------------------------------------------------------------------
// Issue #1070 — checked arithmetic for bids and refunds
// ---------------------------------------------------------------------------

/// `highest_bid + min_increment` overflowing returns `Overflow` instead of trapping.
#[test]
fn test_min_required_overflow_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, b2, token) = setup(&env);

    let whale = Address::generate(&env);
    let start_price = i128::MAX - 5;
    StellarAssetClient::new(&env, &token).mint(&whale, &start_price);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &start_price,
        &10,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    client.bid(&whale, &start_price);

    let result = client.try_bid(&b2, &i128::MAX);
    assert_eq!(result, Err(Ok(AuctionError::Overflow)));
    // State is untouched by the failed bid.
    assert_eq!(client.get_info().highest_bid, start_price);
    assert_eq!(client.get_pending(&whale), 0);
}

/// `pending + highest_bid` overflowing returns `Overflow` instead of trapping.
#[test]
fn test_pending_refund_overflow_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &GRACE_LEDGERS,
        &fee,
    );
    if fee > 0 {
        StellarAssetClient::new(env, token).mint(seller, &fee);
    }
}

/// Top bidder receives their full bid plus the compensation fee; earlier
/// outbid bidders keep their plain refunds, and no funds remain trapped.
#[test]
fn test_grace_cancel_refunds_top_bidder_bid_plus_fee() {
    use soroban_sdk::token::Client as TokenClient;

    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);
    let tc = TokenClient::new(&env, &token);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200);

    // Last ledger inside the grace window (inclusive boundary).
    env.ledger()
        .with_mut(|l| l.sequence_number = GRACE_START + GRACE_LEDGERS);
    client.cancel(&seller);

    assert!(client.is_cancelled());
    assert!(client.get_info().highest_bidder.is_none());
    assert_eq!(client.get_pending(&b1), 1_000);
    assert_eq!(client.get_pending(&b2), 1_200 + CANCEL_FEE);
    assert_eq!(tc.balance(&seller), 0, "seller paid the fee");
    assert_eq!(tc.balance(&client.address), 1_000 + 1_200 + CANCEL_FEE);

    client.withdraw(&b1);
    client.withdraw(&b2);

    assert_eq!(tc.balance(&b1), 100_000);
    assert_eq!(tc.balance(&b2), 100_000 + CANCEL_FEE);
    assert_eq!(tc.balance(&client.address), 0, "no trapped funds");
}

/// A grace-window cancellation emits `AuctionCancelledWithCompensation`.
#[test]
fn test_grace_cancel_emits_compensation_event() {
    use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};

    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);
    client.cancel(&seller);

    let (contract, topics, data) = env.events().all().last().unwrap();
    assert_eq!(contract, client.address);
    let name: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "cancelled_with_compensation"));
    let payload: AuctionCancelledWithCompensation = data.into_val(&env);
    assert_eq!(
        payload,
        AuctionCancelledWithCompensation {
            seller: seller.clone(),
            top_bidder: b1.clone(),
            compensation_amount: CANCEL_FEE,
        }
    );
}

/// A zero fee still refunds the full bid; the seller needs no funds.
#[test]
fn test_grace_cancel_with_zero_fee_refunds_bid_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, 0);

    client.bid(&b1, &1_500);
    client.cancel(&seller);

    assert_eq!(client.get_pending(&b1), 1_500);
}

/// One ledger past the grace window, cancellation after a bid is rejected.
#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_cancel_after_grace_window_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);

    env.ledger()
        .with_mut(|l| l.sequence_number = GRACE_START + GRACE_LEDGERS + 1);
    client.cancel(&seller);
}

/// Before any bid the grace window is irrelevant and no fee is charged.
#[test]
fn test_cancel_without_bids_after_grace_charges_no_fee() {
    use soroban_sdk::token::Client as TokenClient;

    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    env.ledger()
        .with_mut(|l| l.sequence_number = GRACE_START + GRACE_LEDGERS + 5);
    client.cancel(&seller);

    assert!(client.is_cancelled());
    assert_eq!(TokenClient::new(&env, &token).balance(&seller), CANCEL_FEE);
}

/// Bidding on an auction cancelled inside the grace window fails.
#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_bid_after_grace_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);
    client.cancel(&seller);
    client.bid(&b2, &2_000);
}

/// `end` cannot settle a cancelled auction, so the seller can never collect
/// the bid that was refunded to the top bidder.
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_end_after_grace_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);
    client.cancel(&seller);

    env.ledger()
        .with_mut(|l| l.sequence_number = GRACE_START + 101);
    client.end();
}

/// Cancelling twice is rejected, so the top bidder cannot be credited twice.
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_double_grace_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    client.bid(&b1, &1_000);
    client.cancel(&seller);
    client.cancel(&seller);
}

/// A negative cancellation fee is rejected at `start`.
#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_start_rejects_negative_cancellation_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
        &None,
        &None,
    );
    client.bid(&b1, &1_000);

    // Force b1's existing pending balance to the edge of i128.
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::Pending(b1.clone()), &(i128::MAX - 10));
    });

    let result = client.try_bid(&b2, &1_100);
    assert_eq!(result, Err(Ok(AuctionError::Overflow)));
    assert_eq!(client.get_info().highest_bidder, Some(b1.clone()));
    assert_eq!(balance(&env, &token, &b2), 100_000);
}

// ---------------------------------------------------------------------------
// Issue #1068 — counter-bidding with pending refund credit
// ---------------------------------------------------------------------------

/// Credit smaller than the bid: only the difference is transferred.
#[test]
fn test_bid_with_credit_partial_credit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200);
    assert_eq!(client.get_pending(&b1), 1_000);

    client.bid_with_credit(&b1, &1_500);

    // 1_000 credit applied, 500 pulled from b1's wallet.
    assert_eq!(balance(&env, &token, &b1), 100_000 - 1_000 - 500);
    assert_eq!(client.get_pending(&b1), 0);
    assert_eq!(client.get_pending(&b2), 1_200);
    let info = client.get_info();
    assert_eq!(info.highest_bid, 1_500);
    assert_eq!(info.highest_bidder, Some(b1));
    // Contract holds exactly the highest bid plus outstanding refunds.
    assert_eq!(balance(&env, &token, &client.address), 1_500 + 1_200);
}

/// Credit larger than the bid: nothing is transferred and the excess stays pending.
#[test]
fn test_bid_with_credit_full_credit_keeps_excess() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_100);
    client.bid(&b1, &1_200);
    client.bid(&b2, &1_300);
    assert_eq!(client.get_pending(&b1), 2_200);
    let wallet_before = balance(&env, &token, &b1);
    let contract_before = balance(&env, &token, &client.address);

    client.bid_with_credit(&b1, &1_400);

    assert_eq!(balance(&env, &token, &b1), wallet_before);
    assert_eq!(balance(&env, &token, &client.address), contract_before);
    assert_eq!(client.get_pending(&b1), 800);
    assert_eq!(client.get_pending(&b2), 1_100 + 1_300);
    assert_eq!(client.get_info().highest_bid, 1_400);

    // The unused credit is still withdrawable.
    client.withdraw(&b1);
    assert_eq!(balance(&env, &token, &b1), wallet_before + 800);
}

/// Credit exactly equal to the bid clears the pending entry.
#[test]
fn test_bid_with_credit_exact_credit_clears_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_100);
    client.bid(&b1, &1_200);
    client.bid(&b2, &1_300);
    let wallet_before = balance(&env, &token, &b1);

    client.bid_with_credit(&b1, &2_200);

    assert_eq!(balance(&env, &token, &b1), wallet_before);
    assert_eq!(client.get_pending(&b1), 0);
    assert_eq!(
        client.try_withdraw(&b1),
        Err(Ok(AuctionError::NothingToWithdraw))
    );
}

/// The current highest bidder can raise their own bid paying only the delta.
#[test]
fn test_bid_with_credit_self_raise_pays_delta() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid_with_credit(&b1, &1_100);

    assert_eq!(balance(&env, &token, &b1), 100_000 - 1_100);
    assert_eq!(client.get_pending(&b1), 0);
    assert_eq!(balance(&env, &token, &client.address), 1_100);
}

/// Without any credit `bid_with_credit` behaves exactly like `bid`.
#[test]
fn test_bid_with_credit_without_credit_behaves_like_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid_with_credit(&b1, &1_000);
    assert_eq!(balance(&env, &token, &b1), 100_000 - 1_000);
    assert_eq!(client.get_info().highest_bid, 1_000);
}

/// Credit does not bypass bid validation; a rejected bid leaves credit intact.
#[test]
fn test_bid_with_credit_too_low_keeps_credit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &500,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_500);

    let result = client.try_bid_with_credit(&b1, &1_900); // needs >= 2_000
    assert_eq!(result, Err(Ok(AuctionError::BidTooLow)));
    assert_eq!(client.get_pending(&b1), 1_000);
}

/// `bid_with_credit` requires the bidder's authorization.
#[test]
fn test_bid_with_credit_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    client.bid(&b1, &1_000);
    client.bid(&b2, &1_100);

    client.bid_with_credit(&b1, &1_200);
    assert!(
        env.auths().iter().any(|(addr, _)| *addr == b1),
        "bid_with_credit must call bidder.require_auth()"
    );
}

// ---------------------------------------------------------------------------
// Issue #1069 — custodial NFT escrow
// ---------------------------------------------------------------------------

/// The NFT is held by the auction contract once `start` succeeds.
#[test]
fn test_nft_escrowed_on_start() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &Some(nft_addr.clone()),
        &Some(token_id),
    );

    assert_eq!(nft.owner_of(&token_id), client.address);
    let info = client.get_info();
    assert_eq!(info.nft_contract, Some(nft_addr));
    assert_eq!(info.nft_token_id, Some(token_id));
}

/// Payment to the seller and NFT delivery to the winner happen in the same `end`.
#[test]
fn test_nft_delivered_to_winner_with_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &Some(1_500i128),
        &0,
        &Some(nft_addr),
        &Some(token_id),
    );
    client.bid(&b1, &1_000);
    client.bid(&b2, &2_000);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    assert_eq!(nft.owner_of(&token_id), b2);
    assert_eq!(balance(&env, &token, &seller), 2_000);
    assert_eq!(client.get_pending(&b1), 1_000);
}

/// No bids: the NFT goes back to the seller.
#[test]
fn test_nft_returned_when_no_bids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 10;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &Some(nft_addr),
        &Some(token_id),
    );

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();
    assert_eq!(nft.owner_of(&token_id), seller);
}

/// Reserve not met: bidder is refunded and the NFT goes back to the seller.
#[test]
fn test_nft_returned_when_reserve_not_met() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &Some(5_000i128),
        &0,
        &Some(nft_addr),
        &Some(token_id),
    );
    client.bid(&b1, &1_500);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    client.end();

    assert_eq!(nft.owner_of(&token_id), seller);
    assert_eq!(balance(&env, &token, &b1), 100_000);
    assert_eq!(balance(&env, &token, &seller), 0);
}

/// Cancellation returns the NFT to the seller.
#[test]
fn test_nft_returned_on_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 100;
    client.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &10,
        &-1,
    );
}

/// Cancellation settings are exposed through `get_info`.
#[test]
fn test_get_info_reports_cancellation_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    start_with_grace(&env, &client, &seller, &token, CANCEL_FEE);

    let info = client.get_info();
    assert_eq!(info.start_ledger, GRACE_START);
    assert_eq!(info.cancellation_grace_ledgers, GRACE_LEDGERS);
    assert_eq!(info.cancellation_fee, CANCEL_FEE);
        &Some(nft_addr),
        &Some(token_id),
    );
    assert_eq!(nft.owner_of(&token_id), client.address);

    client.cancel(&seller);
    assert_eq!(nft.owner_of(&token_id), seller);
}

/// Supplying only one of `nft_contract` / `token_id` is rejected.
#[test]
fn test_nft_params_must_be_paired() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    let (_, nft_addr, _) = setup_nft(&env, &seller);

    let deadline = env.ledger().sequence() + 100;
    let result = client.try_start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &Some(nft_addr),
        &None,
    );
    assert_eq!(result, Err(Ok(AuctionError::InvalidNftParams)));

    let result = client.try_start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &Some(1u32),
    );
    assert_eq!(result, Err(Ok(AuctionError::InvalidNftParams)));
}

// ---------------------------------------------------------------------------
// Issue #1071 — Dutch (descending price) auction
// ---------------------------------------------------------------------------

const DUTCH_START: i128 = 10_000;
const DUTCH_FLOOR: i128 = 1_000;
const DUTCH_START_LEDGER: u32 = 10;
const DUTCH_DURATION: u32 = 100;

fn start_dutch_default(
    client: &AuctionContractClient,
    seller: &Address,
    token: &Address,
    nft: Option<(Address, u32)>,
) {
    let (nft_contract, token_id) = match nft {
        Some((addr, id)) => (Some(addr), Some(id)),
        None => (None, None),
    };
    client.start_dutch(
        seller,
        token,
        &DUTCH_START,
        &DUTCH_FLOOR,
        &DUTCH_START_LEDGER,
        &DUTCH_DURATION,
        &nft_contract,
        &token_id,
    );
}

/// Price follows the linear schedule and clamps at both ends.
#[test]
fn test_dutch_price_decays_linearly_and_clamps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    start_dutch_default(&client, &seller, &token, None);

    let price_at = |ledger: u32| {
        env.ledger().with_mut(|l| l.sequence_number = ledger);
        client.get_current_price()
    };

    assert_eq!(price_at(0), DUTCH_START); // before start_ledger
    assert_eq!(price_at(10), DUTCH_START);
    assert_eq!(price_at(35), 10_000 - 9_000 * 25 / 100);
    assert_eq!(price_at(60), 5_500);
    assert_eq!(price_at(109), 10_000 - 9_000 * 99 / 100);
    assert_eq!(price_at(110), DUTCH_FLOOR);
    assert_eq!(price_at(10_000), DUTCH_FLOOR);
}

/// `buy` pays the seller, delivers the NFT, and settles in a single call.
#[test]
fn test_dutch_buy_settles_atomically() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);
    start_dutch_default(&client, &seller, &token, Some((nft_addr, token_id)));
    assert_eq!(nft.owner_of(&token_id), client.address);

    env.ledger().with_mut(|l| l.sequence_number = 60);
    let paid = client.buy(&b1, &6_000);

    assert_eq!(paid, 5_500);
    assert_eq!(balance(&env, &token, &seller), 5_500);
    assert_eq!(balance(&env, &token, &b1), 100_000 - 5_500);
    assert_eq!(balance(&env, &token, &client.address), 0);
    assert_eq!(nft.owner_of(&token_id), b1);

    let info = client.get_info();
    assert!(info.settled);
    assert_eq!(info.highest_bid, 5_500);
    assert_eq!(info.highest_bidder, Some(b1));
}

/// A buyer whose `max_price` is below the current price is rejected.
#[test]
fn test_dutch_buy_below_current_price_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_dutch_default(&client, &seller, &token, None);

    env.ledger().with_mut(|l| l.sequence_number = 60);
    assert_eq!(
        client.try_buy(&b1, &5_499),
        Err(Ok(AuctionError::BidTooLow))
    );
    assert_eq!(balance(&env, &token, &b1), 100_000);
    assert!(!client.get_info().settled);
}

/// `buy` before `start_ledger` is rejected.
#[test]
fn test_dutch_buy_before_start_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    start_dutch_default(&client, &seller, &token, None);

    assert_eq!(
        client.try_buy(&b1, &DUTCH_START),
        Err(Ok(AuctionError::AuctionNotStarted))
    );
}

/// Only the first buyer wins; a second `buy` fails without moving funds.
#[test]
fn test_dutch_second_buy_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);
    start_dutch_default(&client, &seller, &token, None);

    env.ledger().with_mut(|l| l.sequence_number = 200);
    client.buy(&b1, &DUTCH_FLOOR);
    assert_eq!(
        client.try_buy(&b2, &DUTCH_START),
        Err(Ok(AuctionError::AuctionEnded))
    );
    assert_eq!(balance(&env, &token, &b2), 100_000);
    assert_eq!(balance(&env, &token, &seller), DUTCH_FLOOR);
}

/// English-only and Dutch-only entry points reject the other mode.
#[test]
fn test_dutch_and_english_modes_are_exclusive() {
    let env = Env::default();
    env.mock_all_auths();

    let (dutch, seller, b1, _, token) = setup(&env);
    start_dutch_default(&dutch, &seller, &token, None);
    env.ledger().with_mut(|l| l.sequence_number = 20);
    assert_eq!(
        dutch.try_bid(&b1, &DUTCH_START),
        Err(Ok(AuctionError::WrongMode))
    );
    assert_eq!(
        dutch.try_bid_with_credit(&b1, &DUTCH_START),
        Err(Ok(AuctionError::WrongMode))
    );
    env.ledger().with_mut(|l| l.sequence_number = 1_000);
    assert_eq!(dutch.try_end(), Err(Ok(AuctionError::WrongMode)));

    let (english, seller, b1, _, token) = setup(&env);
    let deadline = env.ledger().sequence() + 100;
    english.start(
        &seller,
        &token,
        &1_000,
        &100,
        &deadline,
        &None,
        &0,
        &None,
        &None,
    );
    assert_eq!(
        english.try_get_current_price(),
        Err(Ok(AuctionError::WrongMode))
    );
    assert_eq!(
        english.try_buy(&b1, &1_000),
        Err(Ok(AuctionError::WrongMode))
    );
    assert_eq!(english.get_dutch_config(), None);
}

/// An unsold Dutch auction can be cancelled and the NFT is returned.
#[test]
fn test_dutch_cancel_returns_nft() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);
    let (nft, nft_addr, token_id) = setup_nft(&env, &seller);
    start_dutch_default(&client, &seller, &token, Some((nft_addr, token_id)));

    client.cancel(&seller);
    assert_eq!(nft.owner_of(&token_id), seller);

    env.ledger().with_mut(|l| l.sequence_number = 50);
    assert_eq!(
        client.try_buy(&b1, &DUTCH_START),
        Err(Ok(AuctionError::AuctionEnded))
    );
}

/// Invalid Dutch schedules are rejected.
#[test]
fn test_dutch_invalid_schedule_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, _, _, token) = setup(&env);
    env.ledger().with_mut(|l| l.sequence_number = 50);

    let try_start = |start: i128, floor: i128, start_ledger: u32, duration: u32| {
        client.try_start_dutch(
            &seller,
            &token,
            &start,
            &floor,
            &start_ledger,
            &duration,
            &None,
            &None,
        )
    };

    // floor >= start
    assert_eq!(
        try_start(1_000, 1_000, 50, 10),
        Err(Ok(AuctionError::InvalidAmount))
    );
    // negative floor
    assert_eq!(
        try_start(1_000, -1, 50, 10),
        Err(Ok(AuctionError::InvalidAmount))
    );
    // zero duration
    assert_eq!(
        try_start(1_000, 10, 50, 0),
        Err(Ok(AuctionError::InvalidDeadline))
    );
    // start_ledger in the past
    assert_eq!(
        try_start(1_000, 10, 49, 10),
        Err(Ok(AuctionError::InvalidDeadline))
    );
    // start_ledger + duration overflows u32
    assert_eq!(
        try_start(1_000, 10, u32::MAX, 1),
        Err(Ok(AuctionError::Overflow))
    );
}
