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
    Address, Env, FromVal, IntoVal, Symbol, Val, Vec,
    testutils::{Address as _, Events as _, Ledger as _},
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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200); // overbids b1

    // b1 should have a pending refund of 1_000
    assert_eq!(client.get_pending(&b1), 1_000);
    assert_eq!(client.get_info().highest_bid, 1_200);

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
    client.start(&seller, &token, &1_000, &500, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);
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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &500, &deadline, &None, &0);

    client.bid(&b1, &1_000);
    client.bid(&b2, &1_200); // needs >= 1_500 (1000 + 500)
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_double_settle_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    // Deadline sits outside the default anti-sniping window so the bid does not extend it.
    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);
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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);
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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &window);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &window);

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
    client.start(&seller, &token, &1_000, &500, &deadline, &None, &window);

    // First bid at ledger 50 — early, no extension
    env.ledger().with_mut(|l| l.sequence_number = 50);
    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().deadline, deadline);

    // Second bid at ledger 97 — within 5-ledger window, should extend
    env.ledger().with_mut(|l| l.sequence_number = 97);
    client.bid(&b2, &1_500);
    assert_eq!(client.get_info().deadline, deadline + window);
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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

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
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    // b1 is not the seller — should fail with NotAuthorized (#8)
    client.cancel(&b1);
}

// ---------------------------------------------------------------------------
// Minimum bid increment percentage
// ---------------------------------------------------------------------------

/// A bid that clears the absolute `min_increment` but not the 5 % increment is rejected.
#[test]
fn test_bid_below_min_percentage_increment_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    // Absolute increment of 1 so only the percentage rule can reject.
    client.start(&seller, &token, &1_000, &1, &deadline, &None, &0);
    client.bid(&b1, &1_000);

    // 5 % of 1_000 = 50, so anything below 1_050 must fail.
    let result = client.try_bid(&b2, &1_049);
    assert!(matches!(result, Err(Ok(AuctionError::BidIncrementTooLow))));

    let info = client.get_info();
    assert_eq!(info.highest_bid, 1_000);
    assert_eq!(info.highest_bidder, Some(b1));
}

/// A bid exactly at the 5 % threshold, and one above it, are accepted.
#[test]
fn test_bid_at_min_percentage_increment_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &1, &deadline, &None, &0);
    client.bid(&b1, &1_000);

    client.bid(&b2, &1_050);
    assert_eq!(client.get_info().highest_bid, 1_050);

    // Next threshold: 1_050 + ceil(52.5) = 1_103.
    assert!(matches!(
        client.try_bid(&b1, &1_102),
        Err(Ok(AuctionError::BidIncrementTooLow))
    ));
    client.bid(&b1, &1_103);
    assert_eq!(client.get_info().highest_bid, 1_103);
    assert_eq!(client.get_pending(&b2), 1_050);
}

/// The percentage increment is rounded up so small bids cannot truncate it away.
#[test]
fn test_min_percentage_increment_rounds_up() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &30, &1, &deadline, &None, &0);
    client.bid(&b1, &30);

    // 5 % of 30 = 1.5, rounded up to 2: 31 fails and 32 succeeds.
    assert!(matches!(
        client.try_bid(&b2, &31),
        Err(Ok(AuctionError::BidIncrementTooLow))
    ));
    client.bid(&b2, &32);
    assert_eq!(client.get_info().highest_bid, 32);
}

/// The opening bid only needs to meet the start price; no percentage increment applies.
#[test]
fn test_opening_bid_not_subject_to_percentage_increment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    client.start(&seller, &token, &1_000, &1, &deadline, &None, &0);

    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().highest_bid, 1_000);
}

/// The absolute increment still applies when it is larger than the percentage one.
#[test]
fn test_absolute_increment_still_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline = env.ledger().sequence() + 100;
    // Absolute 200 > 5 % of 1_000 (50).
    client.start(&seller, &token, &1_000, &200, &deadline, &None, &0);
    client.bid(&b1, &1_000);

    assert!(matches!(
        client.try_bid(&b2, &1_100),
        Err(Ok(AuctionError::BidTooLow))
    ));
    client.bid(&b2, &1_200);
    assert_eq!(client.get_info().highest_bid, 1_200);
}

// ---------------------------------------------------------------------------
// Default anti-sniping: 10-ledger window, 20-ledger extension
// ---------------------------------------------------------------------------

/// Returns the new deadline carried by the most recent `AuctionExtended` event
/// emitted by `auction`, if any.
fn auction_extended_deadline(env: &Env, auction: &Address) -> Option<u32> {
    let expected: Vec<Val> = (Symbol::new(env, "AuctionExtended"), auction.clone()).into_val(env);
    env.events()
        .all()
        .iter()
        .filter(|(contract, topics, _)| contract == auction && *topics == expected)
        .last()
        .map(|(_, _, data)| u32::from_val(env, &data))
}

/// A bid inside the final 10 ledgers extends the deadline by 20 ledgers.
#[test]
fn test_default_anti_snipe_extends_deadline_in_final_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    env.ledger().with_mut(|l| l.sequence_number = 95);
    client.bid(&b1, &1_000);

    let expected = deadline + ANTI_SNIPE_EXTENSION_LEDGERS;
    assert_eq!(client.get_info().deadline, expected);
    assert_eq!(
        auction_extended_deadline(&env, &client.address),
        Some(expected)
    );
}

/// A bid exactly 10 ledgers before the deadline is inside the window.
#[test]
fn test_default_anti_snipe_window_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    env.ledger()
        .with_mut(|l| l.sequence_number = deadline - ANTI_SNIPE_WINDOW_LEDGERS);
    client.bid(&b1, &1_000);

    assert_eq!(client.get_info().deadline, 120);
}

/// A bid placed on the deadline ledger itself still extends the auction.
#[test]
fn test_default_anti_snipe_bid_on_deadline_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    env.ledger().with_mut(|l| l.sequence_number = deadline);
    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().deadline, 120);

    // The auction can no longer be settled at the original deadline + 1.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    assert!(matches!(
        client.try_end(),
        Err(Ok(AuctionError::AuctionNotEnded))
    ));
}

/// A bid outside the final 10 ledgers leaves the deadline untouched and emits no event.
#[test]
fn test_default_anti_snipe_no_extension_outside_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    env.ledger()
        .with_mut(|l| l.sequence_number = deadline - ANTI_SNIPE_WINDOW_LEDGERS - 1);
    client.bid(&b1, &1_000);

    assert_eq!(client.get_info().deadline, deadline);
    assert_eq!(auction_extended_deadline(&env, &client.address), None);
}

/// Consecutive late bids keep pushing the deadline back by 20 ledgers each.
#[test]
fn test_default_anti_snipe_repeated_extensions() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, b2, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &0);

    env.ledger().with_mut(|l| l.sequence_number = 98);
    client.bid(&b1, &1_000);
    assert_eq!(client.get_info().deadline, 120);

    env.ledger().with_mut(|l| l.sequence_number = 115);
    client.bid(&b2, &1_100);
    assert_eq!(client.get_info().deadline, 140);
    assert_eq!(auction_extended_deadline(&env, &client.address), Some(140));
}

/// A custom `extension_window` also emits `AuctionExtended`.
#[test]
fn test_custom_window_emits_auction_extended() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, b1, _, token) = setup(&env);

    let deadline: u32 = 100;
    client.start(&seller, &token, &1_000, &100, &deadline, &None, &5);

    env.ledger().with_mut(|l| l.sequence_number = 97);
    client.bid(&b1, &1_000);

    assert_eq!(client.get_info().deadline, 105);
    assert_eq!(auction_extended_deadline(&env, &client.address), Some(105));
}
