#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
//! Integration tests: deploys both TokenContract and EscrowContract in the
//! same Soroban test environment and exercises the full escrow lifecycle.
//!
//! Closes #221 – no integration test between token and escrow contracts.
//! Closes #222 – escrow tests used a mock token address; fund/release path untested.

#![cfg(test)]

use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

use soroban_escrow_template::{EscrowContract, EscrowContractClient, EscrowState};
use soroban_token_template::{TokenContract, TokenContractClient};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy_token<'a>(env: &'a Env, admin: &Address) -> (TokenContractClient<'a>, Address) {
    let addr = env.register_contract(None, TokenContract);
    let client = TokenContractClient::new(env, &addr);
    client.initialize(
        admin,
        &String::from_str(env, "Test Token"),
        &String::from_str(env, "TEST"),
        &18u32,
        &None,
    );
    (client, addr)
}

fn deploy_escrow<'a>(env: &'a Env) -> (EscrowContractClient<'a>, Address) {
    let addr = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &addr);
    (client, addr)
}

// ── #221 / #222: full happy-path lifecycle ────────────────────────────────────

/// initialize → fund → mark_delivered → approve_delivery
/// Verifies that real token balances move correctly at each step.
#[test]
fn test_full_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);
    assert_eq!(token.balance(&buyer), amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );

    // fund: buyer's tokens move into the escrow contract
    escrow.fund();
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&escrow_addr), amount);

    // mark delivered by seller
    escrow.mark_delivered();
    assert_eq!(escrow.get_state(), Some(EscrowState::Delivered));

    // buyer approves → tokens released to seller
    escrow.approve_delivery();
    assert_eq!(escrow.get_state(), Some(EscrowState::Completed));
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.balance(&seller), amount);
}

/// initialize → fund → deadline passes → request_refund
/// Verifies tokens are returned to the buyer.
#[test]
fn test_full_escrow_lifecycle_refund_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 500i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow.fund();

    assert_eq!(token.balance(&escrow_addr), amount);

    // advance past deadline
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    assert!(escrow.is_deadline_passed());

    escrow.request_refund();
    assert_eq!(escrow.get_state(), Some(EscrowState::Refunded));
    assert_eq!(token.balance(&buyer), amount);
    assert_eq!(token.balance(&escrow_addr), 0);
}

/// initialize → fund → raise_dispute → arbiter resolves (both paths)
/// Verifies Disputed state is reached and token balances after each resolution.
#[test]
fn test_escrow_dispute_and_arbiter_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 200;

    // ── Path 1: arbiter releases to seller ──────────────────────────────
    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow.fund();

    assert_eq!(token.balance(&escrow_addr), amount);
    assert_eq!(token.balance(&buyer), 0);

    escrow.raise_dispute(&buyer);
    assert_eq!(escrow.get_state(), Some(EscrowState::Disputed));
    assert_eq!(token.balance(&escrow_addr), amount);

    escrow.resolve_dispute(&arbiter, &true);
    assert_eq!(escrow.get_state(), Some(EscrowState::Completed));
    assert_eq!(token.balance(&seller), amount);
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.balance(&buyer), 0);

    // ── Path 2: arbiter refunds to buyer ────────────────────────────────
    let buyer2 = Address::generate(&env);
    let seller2 = Address::generate(&env);
    let arbiter2 = Address::generate(&env);
    let escrow_admin2 = Address::generate(&env);

    token.mint(&buyer2, &amount);

    let (escrow2, escrow_addr2) = deploy_escrow(&env);
    escrow2.initialize(
        &escrow_admin2,
        &buyer2,
        &seller2,
        &arbiter2,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow2.fund();

    assert_eq!(token.balance(&escrow_addr2), amount);

    escrow2.raise_dispute(&seller2);
    assert_eq!(escrow2.get_state(), Some(EscrowState::Disputed));

    escrow2.resolve_dispute(&arbiter2, &false);
    assert_eq!(escrow2.get_state(), Some(EscrowState::Refunded));
    assert_eq!(token.balance(&buyer2), amount);
    assert_eq!(token.balance(&escrow_addr2), 0);
    assert_eq!(token.balance(&seller2), 0);
}

/// initialize → fund → arbiter resolves in favour of seller
#[test]
fn test_full_escrow_lifecycle_arbiter_resolves_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 750i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow.fund();

    escrow.raise_dispute(&buyer);
    escrow.resolve_dispute(&arbiter, &true); // true → release to seller
    assert_eq!(escrow.get_state(), Some(EscrowState::Completed));
    assert_eq!(token.balance(&seller), amount);
    assert_eq!(token.balance(&escrow_addr), 0);
}

/// initialize → fund → arbiter resolves in favour of buyer
#[test]
fn test_full_escrow_lifecycle_arbiter_resolves_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 300i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow.fund();

    escrow.raise_dispute(&buyer);
    escrow.resolve_dispute(&arbiter, &false); // false → refund to buyer
    assert_eq!(escrow.get_state(), Some(EscrowState::Refunded));
    assert_eq!(token.balance(&buyer), amount);
    assert_eq!(token.balance(&escrow_addr), 0);
}

/// initialize → cancel (no funds involved)
#[test]
fn test_full_escrow_lifecycle_cancel_before_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 200i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, _) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );

    escrow.cancel();
    assert_eq!(escrow.get_state(), Some(EscrowState::Cancelled));
    // buyer still holds all tokens – nothing was transferred
    assert_eq!(token.balance(&buyer), amount);
}

/// initialize → fund → mark_delivered → approve_delivery with capped-supply token
/// Verifies escrow works correctly when token has a supply cap.
#[test]
fn test_escrow_with_capped_token() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let max_supply = 1_000i128;
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 200;

    // Deploy token with max_supply set
    let addr = env.register_contract(None, TokenContract);
    let token = TokenContractClient::new(&env, &addr);
    token.initialize(
        &token_admin,
        &String::from_str(&env, "Capped Token"),
        &String::from_str(&env, "CAP"),
        &18u32,
        &Some(max_supply),
    );
    let token_addr = addr;

    // Mint full supply to buyer
    token.mint(&buyer, &max_supply);
    assert_eq!(token.balance(&buyer), max_supply);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );

    // fund: buyer's tokens move into the escrow contract
    escrow.fund();
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&escrow_addr), amount);

    // mark delivered by seller
    escrow.mark_delivered();
    assert_eq!(escrow.get_state(), Some(EscrowState::Delivered));

    // buyer approves → tokens released to seller
    escrow.approve_delivery();
    assert_eq!(escrow.get_state(), Some(EscrowState::Completed));
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.balance(&seller), amount);
}

// ── SAC-based token variant (mirrors original escrow tests) ──────────────────

/// Same happy-path but using a Stellar Asset Contract token instead of the
/// custom TokenContract, confirming the escrow works with both token types.
#[test]
fn test_full_escrow_lifecycle_with_sac_token() {
    let env = Env::default();
    env.mock_all_auths();

    let sac_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let dispute_timeout_ledgers: u32 = 100;
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 200;

    let sac = env.register_stellar_asset_contract_v2(sac_admin.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(&env, &token_addr).mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &dispute_timeout_ledgers,
        &None,
    );
    escrow.fund();

    assert_eq!(
        soroban_sdk::token::Client::new(&env, &token_addr).balance(&escrow_addr),
        amount
    );

    escrow.mark_delivered();
    escrow.approve_delivery();

    assert_eq!(
        soroban_sdk::token::Client::new(&env, &token_addr).balance(&seller),
        amount
    );
}

// ── #442: token allowance expiry flow ──────────────────────────────────────

/// mint → approve with short expiry → advance ledger past expiry → transfer_from fails
/// Verifies that expired allowances cannot be used and balances remain unchanged.
#[test]
fn test_token_allowance_expiry_in_integration() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let amount = 1_000i128;
    let expiry = env.ledger().sequence() + 10;

    let (token, _) = deploy_token(&env, &token_admin);
    token.mint(&owner, &amount);
    assert_eq!(token.balance(&owner), amount);

    // approve with short expiry
    token.approve(&owner, &spender, &amount, &expiry);
    assert_eq!(token.allowance(&owner, &spender), amount);

    // advance ledger past expiry
    env.ledger().with_mut(|l| l.sequence_number = expiry + 1);

    // transfer_from should fail due to expired allowance
    let result = token.try_transfer_from(&spender, &owner, &receiver, &amount);
    assert!(result.is_err());

    // verify balances unchanged
    assert_eq!(token.balance(&owner), amount);
    assert_eq!(token.balance(&receiver), 0);
}

// ── #857: marketplace integration test ───────────────────────────────────────

use soroban_marketplace_template::{MarketplaceContract, MarketplaceContractClient};
use soroban_nft_template::{NftContract, NftContractClient};

fn deploy_nft<'a>(env: &'a Env, admin: &Address) -> (NftContractClient<'a>, Address) {
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        admin,
        &String::from_str(env, "Test NFT"),
        &String::from_str(env, "TNFT"),
        &Some(10u32),
        &None,
        &None,
    );
    (client, addr)
}

fn deploy_marketplace<'a>(env: &'a Env) -> (MarketplaceContractClient<'a>, Address) {
    let addr = env.register_contract(None, MarketplaceContract);
    let client = MarketplaceContractClient::new(env, &addr);
    (client, addr)
}

/// Full lifecycle: list → buy (with royalty split) → verify NFT ownership transfer
/// Closes #857 – integration test harness for marketplace (post build-fix)
#[test]
fn test_marketplace_full_lifecycle_with_royalty() {
    let env = Env::default();
    env.mock_all_auths();

    let nft_admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let marketplace_admin = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    // Deploy NFT contract and mint a token to seller
    let (nft, nft_addr) = deploy_nft(&env, &nft_admin);
    let token_id = 1u32;
    nft.mint(
        &seller,
        &token_id,
        &String::from_str(&env, "https://example.com/token/1"),
        &None,
        &None,
    );
    assert_eq!(nft.owner_of(&token_id), seller);

    // Deploy payment token and mint to buyer
    let (token, token_addr) = deploy_token(&env, &token_admin);
    let price = 1_000i128;
    let buyer_initial_balance = 5_000i128;
    token.mint(&buyer, &buyer_initial_balance);
    assert_eq!(token.balance(&buyer), buyer_initial_balance);

    // Deploy and initialize marketplace with 2.5% royalty
    let (marketplace, marketplace_addr) = deploy_marketplace(&env);
    let royalty_bps = 250u32; // 2.5%
    marketplace.initialize(
        &marketplace_admin,
        &token_addr,
        &royalty_bps,
        &royalty_recipient,
    );

    // Seller approves marketplace to transfer the NFT
    nft.approve(&token_id, &marketplace_addr);

    // Seller lists the NFT
    let listing_id = marketplace.list(&seller, &nft_addr, &token_id, &price, &token_addr);
    let listing = marketplace.get_listing(&listing_id).unwrap();
    assert_eq!(listing.seller, seller);
    assert_eq!(listing.price, price);
    assert_eq!(listing.active, true);

    // Buyer purchases the NFT
    marketplace.buy(&buyer, &listing_id, &price);

    // Verify NFT ownership transferred to buyer
    assert_eq!(nft.owner_of(&token_id), buyer);

    // Verify payment distribution with royalty split
    let royalty_amount = (price * i128::from(royalty_bps)) / 10_000i128;
    let seller_amount = price - royalty_amount;
    assert_eq!(token.balance(&seller), seller_amount);
    assert_eq!(token.balance(&royalty_recipient), royalty_amount);
    assert_eq!(token.balance(&buyer), buyer_initial_balance - price);

    // Verify listing is now inactive
    let listing_after = marketplace.get_listing(&listing_id).unwrap();
    assert_eq!(listing_after.active, false);
}

/// List → cancel → verify NFT remains with seller
/// Closes #857 – integration test harness for marketplace (post build-fix)
#[test]
fn test_marketplace_cancel_listing() {
    let env = Env::default();
    env.mock_all_auths();

    let nft_admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let marketplace_admin = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    let seller = Address::generate(&env);

    // Deploy NFT contract and mint a token to seller
    let (nft, nft_addr) = deploy_nft(&env, &nft_admin);
    let token_id = 1u32;
    nft.mint(
        &seller,
        &token_id,
        &String::from_str(&env, "https://example.com/token/1"),
        &None,
        &None,
    );
    assert_eq!(nft.owner_of(&token_id), seller);

    // Deploy payment token
    let (_, token_addr) = deploy_token(&env, &token_admin);

    // Deploy and initialize marketplace
    let (marketplace, marketplace_addr) = deploy_marketplace(&env);
    marketplace.initialize(&marketplace_admin, &token_addr, &250u32, &royalty_recipient);

    // Seller approves and lists the NFT
    nft.approve(&token_id, &marketplace_addr);
    let listing_id = marketplace.list(&seller, &nft_addr, &token_id, &1_000i128, &token_addr);

    // Seller cancels the listing
    marketplace.cancel(&seller, &listing_id);

    // Verify NFT still belongs to seller
    assert_eq!(nft.owner_of(&token_id), seller);

    // Verify listing is now inactive
    let listing = marketplace.get_listing(&listing_id).unwrap();
    assert_eq!(listing.active, false);
}

// ── #855: auction integration test ───────────────────────────────────────────

use soroban_auction_template::{AuctionContract, AuctionContractClient};

fn deploy_auction<'a>(env: &'a Env) -> (AuctionContractClient<'a>, Address) {
    let addr = env.register_contract(None, AuctionContract);
    let client = AuctionContractClient::new(env, &addr);
    (client, addr)
}

/// Full auction lifecycle: start → multiple competing bids → outbid withdrawal → end.
///
/// Verifies that:
/// - The first bid at start_price is accepted.
/// - A higher bid from a second bidder is accepted and the first bidder's funds
///   are queued as a pending refund.
/// - The outbid bidder can withdraw their pending refund at any time.
/// - After the deadline the seller receives the winning bid.
///
/// Closes #855 – integration test harness for auction (post build-fix).
#[test]
fn test_auction_full_lifecycle_competing_bids_and_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let bidder1 = Address::generate(&env);
    let bidder2 = Address::generate(&env);

    // Mint tokens for both bidders via a SAC
    let sac = env.register_stellar_asset_contract_v2(seller.clone());
    let token_addr = sac.address();
    let sac_client = StellarAssetClient::new(&env, &token_addr);
    sac_client.mint(&bidder1, &100_000);
    sac_client.mint(&bidder2, &100_000);

    let token = soroban_sdk::token::Client::new(&env, &token_addr);

    let (auction, auction_addr) = deploy_auction(&env);

    // start: 1 000 start price, 100 min increment, deadline = current + 200
    let start_price = 1_000i128;
    let min_increment = 100i128;
    let deadline = env.ledger().sequence() + 200;
    auction.start(
        &seller,
        &token_addr,
        &start_price,
        &min_increment,
        &deadline,
        &None,
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    // bidder1 places the opening bid at start_price
    auction.bid(&bidder1, &start_price);
    assert_eq!(token.balance(&auction_addr), start_price);
    assert_eq!(token.balance(&bidder1), 100_000 - start_price);

    // bidder2 outbids by min_increment
    let bid2 = start_price + min_increment; // 1 100
    auction.bid(&bidder2, &bid2);
    assert_eq!(token.balance(&auction_addr), start_price + bid2);
    // bidder1 should have a pending refund of their original bid
    assert_eq!(auction.get_pending(&bidder1), start_price);

    // bidder1 withdraws their pending refund
    auction.withdraw(&bidder1);
    assert_eq!(auction.get_pending(&bidder1), 0);
    assert_eq!(token.balance(&bidder1), 100_000 - start_price + start_price); // back to full

    // advance past deadline and settle
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    auction.end();

    // seller receives the winning bid; bidder2's tokens went to seller
    assert_eq!(token.balance(&seller), bid2);
    assert_eq!(token.balance(&auction_addr), 0);

    // auction info reflects settled state
    let info = auction.get_info();
    assert!(info.settled);
    assert_eq!(info.highest_bid, bid2);
}

/// Reserve price not met: seller gets nothing, highest bidder's funds returned directly.
///
/// Closes #855 – integration test harness for auction (post build-fix).
#[test]
fn test_auction_reserve_not_met_refunds_bidder() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let bidder = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(seller.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(&env, &token_addr).mint(&bidder, &50_000);

    let token = soroban_sdk::token::Client::new(&env, &token_addr);
    let (auction, auction_addr) = deploy_auction(&env);

    let start_price = 1_000i128;
    let reserve_price = 5_000i128;
    let deadline = env.ledger().sequence() + 100;
    auction.start(
        &seller,
        &token_addr,
        &start_price,
        &100,
        &deadline,
        &Some(reserve_price),
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    // bid below reserve
    auction.bid(&bidder, &start_price);
    assert_eq!(token.balance(&auction_addr), start_price);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    auction.end();

    // reserve not met: bidder gets funds back directly, seller gets nothing
    assert_eq!(token.balance(&bidder), 50_000);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(token.balance(&auction_addr), 0);
}

/// Custodial NFT auction with refund-credit counter-bidding (#1068, #1069, #1070).
///
/// Verifies that:
/// - The NFT is escrowed into the auction contract on `start`.
/// - An outbid bidder can counter-bid with `bid_with_credit`, paying only the delta.
/// - `end` pays the seller and delivers the NFT to the winner atomically.
/// - The contract is left holding exactly the losing bidder's pending refund.
#[test]
fn test_auction_custodial_nft_with_credit_counter_bid() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let bidder1 = Address::generate(&env);
    let bidder2 = Address::generate(&env);
    let nft_admin = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(seller.clone());
    let token_addr = sac.address();
    let sac_client = StellarAssetClient::new(&env, &token_addr);
    sac_client.mint(&bidder1, &10_000);
    sac_client.mint(&bidder2, &10_000);
    let token = soroban_sdk::token::Client::new(&env, &token_addr);

    let (nft, nft_addr) = deploy_nft(&env, &nft_admin);
    let token_id = nft.mint(
        &seller,
        &String::from_str(&env, "ipfs://auction-lot"),
        &None,
        &None,
    );

    let (auction, auction_addr) = deploy_auction(&env);
    let deadline = env.ledger().sequence() + 100;
    auction.start(
        &seller,
        &token_addr,
        &1_000,
        &100,
        &deadline,
        &Some(1_500i128),
        &0,
        &Some(nft_addr),
        &Some(token_id),
    );
    assert_eq!(nft.owner_of(&token_id), auction_addr);

    auction.bid(&bidder1, &1_000);
    auction.bid(&bidder2, &1_200);
    assert_eq!(auction.get_pending(&bidder1), 1_000);

    // bidder1 counter-bids 1 600 using the 1 000 credit: only 600 leaves the wallet.
    auction.bid_with_credit(&bidder1, &1_600);
    assert_eq!(token.balance(&bidder1), 10_000 - 1_000 - 600);
    assert_eq!(auction.get_pending(&bidder1), 0);
    assert_eq!(auction.get_pending(&bidder2), 1_200);

    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    auction.end();

    assert_eq!(nft.owner_of(&token_id), bidder1);
    assert_eq!(token.balance(&seller), 1_600);
    assert_eq!(token.balance(&auction_addr), 1_200);

    auction.withdraw(&bidder2);
    assert_eq!(token.balance(&bidder2), 10_000);
    assert_eq!(token.balance(&auction_addr), 0);
}

/// Dutch auction with custodial NFT: `buy` settles payment and delivery at once (#1071).
#[test]
fn test_dutch_auction_custodial_nft_buy() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let nft_admin = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(seller.clone());
    let token_addr = sac.address();
    StellarAssetClient::new(&env, &token_addr).mint(&buyer, &10_000);
    let token = soroban_sdk::token::Client::new(&env, &token_addr);

    let (nft, nft_addr) = deploy_nft(&env, &nft_admin);
    let token_id = nft.mint(
        &seller,
        &String::from_str(&env, "ipfs://dutch-auction-lot"),
        &None,
        &None,
    );

    let (auction, auction_addr) = deploy_auction(&env);
    let start_ledger = env.ledger().sequence();
    auction.start_dutch(
        &seller,
        &token_addr,
        &8_000,
        &2_000,
        &start_ledger,
        &60,
        &Some(nft_addr),
        &Some(token_id),
    );
    assert_eq!(nft.owner_of(&token_id), auction_addr);

    // Halfway through the schedule the price is (8 000 + 2 000) / 2.
    env.ledger()
        .with_mut(|l| l.sequence_number = start_ledger + 30);
    assert_eq!(auction.get_current_price(), 5_000);

    let paid = auction.buy(&buyer, &5_000);
    assert_eq!(paid, 5_000);
    assert_eq!(nft.owner_of(&token_id), buyer);
    assert_eq!(token.balance(&seller), 5_000);
    assert_eq!(token.balance(&buyer), 5_000);
    assert_eq!(token.balance(&auction_addr), 0);
    assert!(auction.get_info().settled);
}

// ── #856: lottery integration test ───────────────────────────────────────────

use soroban_lottery_template::{LotteryContract, LotteryContractClient};
use soroban_sdk::{Bytes, BytesN, Vec as SorobanVec};

fn deploy_lottery<'a>(env: &'a Env) -> (LotteryContractClient<'a>, Address) {
    let addr = env.register_contract(None, LotteryContract);
    let client = LotteryContractClient::new(env, &addr);
    (client, addr)
}

/// Helper: compute SHA-256(secret ++ salt) to derive the commitment hash.
fn make_commit_hash(env: &Env, secret: &[u8; 32], salt: &[u8; 32]) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.extend_from_array(secret);
    preimage.extend_from_array(salt);
    env.crypto().sha256(&preimage).into()
}

/// Full lottery lifecycle: initialize → buy tickets → commit → draw/reveal → winner payout.
///
/// Uses a single-winner lottery (100 % prize split) so we can assert an exact balance.
/// Verifies:
/// - Ticket purchases transfer tokens to the contract.
/// - `commit` closes ticket sales.
/// - `draw` with correct preimage distributes the entire prize pool and returns winners.
/// - Winning address balance increases by the full pool.
///
/// Closes #856 – integration test harness for lottery (post build-fix).
#[test]
fn test_lottery_full_lifecycle_ticket_purchase_commit_reveal_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let buyer3 = Address::generate(&env);

    // SAC token: mint enough for each buyer to purchase tickets
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let sac_client = StellarAssetClient::new(&env, &token_addr);
    let ticket_price = 1_000i128;
    sac_client.mint(&buyer1, &ticket_price);
    sac_client.mint(&buyer2, &ticket_price);
    sac_client.mint(&buyer3, &ticket_price);

    let token = soroban_sdk::token::Client::new(&env, &token_addr);

    let (lottery, lottery_addr) = deploy_lottery(&env);

    // Single winner, full prize pool
    let mut prize_splits = SorobanVec::new(&env);
    prize_splits.push_back(10_000u32); // 100 % in basis points
    lottery.initialize(&admin, &token_addr, &ticket_price, &1, &prize_splits, &None);

    // Buyers purchase tickets
    lottery.buy_ticket(&buyer1);
    lottery.buy_ticket(&buyer2);
    lottery.buy_ticket(&buyer3);
    let total_prize = ticket_price * 3;
    assert_eq!(token.balance(&lottery_addr), total_prize);
    assert_eq!(token.balance(&buyer1), 0);

    // Admin commits: hash(secret ++ salt)
    let secret: [u8; 32] = [0xAB; 32];
    let salt: [u8; 32] = [0xCD; 32];
    let commit_hash = make_commit_hash(&env, &secret, &salt);
    let reveal_deadline = env.ledger().sequence() + 50;
    lottery.commit(&commit_hash, &reveal_deadline);

    // Advance one ledger so we're still within the reveal window, then draw
    env.ledger().with_mut(|l| l.sequence_number += 1);

    let secret_bytes: BytesN<32> = BytesN::from_array(&env, &secret);
    let salt_bytes: BytesN<32> = BytesN::from_array(&env, &salt);
    let winners = lottery.draw(&secret_bytes, &salt_bytes);

    // Exactly one winner must be declared
    assert_eq!(winners.len(), 1);
    #[allow(clippy::unwrap_used)]
    let winner = winners.get(0).unwrap();

    // Winner must be one of the participants
    assert!(winner == buyer1 || winner == buyer2 || winner == buyer3);

    // Winner receives the full prize pool; contract balance is drained
    assert_eq!(token.balance(&winner), total_prize);
    assert_eq!(token.balance(&lottery_addr), 0);
}

/// Ticket-cap enforcement: a buyer cannot exceed `max_tickets_per_address`.
///
/// Closes #856 – integration test harness for lottery (post build-fix).
#[test]
fn test_lottery_ticket_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let ticket_price = 500i128;
    StellarAssetClient::new(&env, &token_addr).mint(&buyer, &(ticket_price * 5));

    let (lottery, _) = deploy_lottery(&env);
    let mut prize_splits = SorobanVec::new(&env);
    prize_splits.push_back(10_000u32);
    lottery.initialize(
        &admin,
        &token_addr,
        &ticket_price,
        &1,
        &prize_splits,
        &Some(2),
    );

    // First two tickets succeed
    lottery.buy_ticket(&buyer);
    lottery.buy_ticket(&buyer);

    // Third ticket must fail with TicketCapExceeded
    let result = lottery.try_buy_ticket(&buyer);
    assert!(result.is_err());
}

// ── #863: cross-contract integration test: marketplace + nft + token ─────────

/// End-to-end flow: mint NFT → list on marketplace → purchase with token contract.
///
/// Verifies:
/// - NFT ownership transfers from seller to buyer on purchase.
/// - Token balances (buyer, seller, royalty recipient) are correct after the sale.
/// - The listing is marked inactive after the purchase.
/// - Royalty recipient receives the correct basis-point share.
///
/// Closes #863 – cross-contract integration test: marketplace + nft + token end-to-end.
#[test]
fn test_marketplace_nft_token_end_to_end() {
    let env = Env::default();
    env.mock_all_auths();

    let nft_admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let marketplace_admin = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    // ── Deploy NFT contract ──────────────────────────────────────────────
    let (nft, nft_addr) = deploy_nft(&env, &nft_admin);
    let token_id = 42u32;
    nft.mint(
        &seller,
        &token_id,
        &String::from_str(&env, "https://example.com/nft/42"),
        &None,
        &None,
    );
    // Confirm initial ownership
    assert_eq!(nft.owner_of(&token_id), seller);

    // ── Deploy payment token and fund buyer ──────────────────────────────
    let (token, token_addr) = deploy_token(&env, &token_admin);
    let price = 2_000i128;
    let buyer_funds = 10_000i128;
    token.mint(&buyer, &buyer_funds);
    assert_eq!(token.balance(&buyer), buyer_funds);

    // Seller and royalty recipient start with zero balance
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(token.balance(&royalty_recipient), 0);

    // ── Deploy and initialize marketplace (5 % royalty) ──────────────────
    let (marketplace, marketplace_addr) = deploy_marketplace(&env);
    let royalty_bps = 500u32; // 5 %
    marketplace.initialize(
        &marketplace_admin,
        &token_addr,
        &royalty_bps,
        &royalty_recipient,
    );

    // ── Seller approves marketplace to transfer the NFT, then lists it ───
    nft.approve(&token_id, &marketplace_addr);
    let listing_id = marketplace.list(&seller, &nft_addr, &token_id, &price, &token_addr);

    // Confirm listing is active
    let listing = marketplace.get_listing(&listing_id).unwrap();
    assert_eq!(listing.seller, seller);
    assert_eq!(listing.price, price);
    assert!(listing.active);

    // ── Buyer purchases the NFT ──────────────────────────────────────────
    marketplace.buy(&buyer, &listing_id, &price);

    // ── Assert NFT ownership transferred ────────────────────────────────
    assert_eq!(nft.owner_of(&token_id), buyer);

    // ── Assert final token balances ──────────────────────────────────────
    let royalty_amount = (price * i128::from(royalty_bps)) / 10_000i128; // 100
    let seller_amount = price - royalty_amount; // 1 900

    assert_eq!(
        token.balance(&buyer),
        buyer_funds - price,
        "buyer balance should decrease by full price"
    );
    assert_eq!(
        token.balance(&seller),
        seller_amount,
        "seller receives price minus royalty"
    );
    assert_eq!(
        token.balance(&royalty_recipient),
        royalty_amount,
        "royalty recipient receives royalty share"
    );

    // ── Listing is now inactive ──────────────────────────────────────────
    let listing_after = marketplace.get_listing(&listing_id).unwrap();
    assert!(!listing_after.active);
}

/// Mint NFT with per-token royalty → list on marketplace → purchase.
/// Verifies that the marketplace correctly surfaces royalty intent via
/// royalty_info, even though the marketplace uses its own initialized
/// royalty config. This test checks the NFT royalty_info view independently
/// alongside the marketplace purchase to confirm both are consistent.
///
/// Closes #863, #831 – cross-contract royalty integration.
#[test]
fn test_marketplace_nft_token_with_per_token_royalty() {
    let env = Env::default();
    env.mock_all_auths();

    let nft_admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let marketplace_admin = Address::generate(&env);
    let nft_royalty_recipient = Address::generate(&env);
    let marketplace_royalty_recipient = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);

    // ── Deploy NFT contract with collection-level 2 % royalty ───────────
    let nft_addr_raw = env.register_contract(None, NftContract);
    let nft = NftContractClient::new(&env, &nft_addr_raw);
    nft.initialize(
        &nft_admin,
        &String::from_str(&env, "Royalty NFT"),
        &String::from_str(&env, "RNFT"),
        &0u32,
        &Some(200u32), // 2 % collection royalty
        &Some(nft_royalty_recipient.clone()),
    );

    let token_id = 7u32;
    let per_token_bps = 300u32; // 3 % per-token override
    nft.mint(
        &seller,
        &token_id,
        &String::from_str(&env, "ipfs://rnft/7"),
        &Some(per_token_bps),
        &Some(nft_royalty_recipient.clone()),
    );

    // Verify royalty_info returns the per-token override (3 % of 1 000 = 30)
    let sale_price = 1_000i128;
    let royalty = nft.royalty_info(&token_id, &sale_price).unwrap();
    assert_eq!(royalty.recipient, nft_royalty_recipient);
    assert_eq!(royalty.amount, 30i128); // 3 % of 1 000

    // ── Deploy payment token and fund buyer ──────────────────────────────
    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &5_000i128);

    // ── Deploy marketplace (1 % royalty to marketplace_royalty_recipient) ─
    let (marketplace, marketplace_addr) = deploy_marketplace(&env);
    marketplace.initialize(
        &marketplace_admin,
        &token_addr,
        &100u32, // 1 %
        &marketplace_royalty_recipient,
    );

    // ── List and buy ─────────────────────────────────────────────────────
    nft.approve(&token_id, &marketplace_addr);
    let listing_id = marketplace.list(&seller, &nft_addr_raw, &token_id, &sale_price, &token_addr);
    marketplace.buy(&buyer, &listing_id);
    let listing_id = marketplace.list(&seller, &nft_addr_raw, &token_id, &sale_price);
    marketplace.buy(&buyer, &listing_id, &sale_price);

    // Buyer owns the NFT
    assert_eq!(nft.owner_of(&token_id), buyer);

    // Marketplace distributes 1 % to its royalty recipient, 99 % to seller
    let mkt_royalty = (sale_price * 100i128) / 10_000i128; // 10
    let seller_amount = sale_price - mkt_royalty; // 990
    assert_eq!(token.balance(&buyer), 5_000 - sale_price);
    assert_eq!(token.balance(&seller), seller_amount);
    assert_eq!(token.balance(&marketplace_royalty_recipient), mkt_royalty);
}
