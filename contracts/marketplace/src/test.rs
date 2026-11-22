#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

// ---------------------------------------------------------------------------
// Minimal mock NFT contract for testing
// ---------------------------------------------------------------------------

#[contracttype]
enum NftKey {
    Owner(u32),
    Approved(u32),
    /// Optional (recipient, bps) royalty returned by `royalty_info`.
    Royalty,
}

#[contract]
pub struct MockNft;

#[contractimpl]
impl MockNft {
    pub fn init(env: Env, owner: Address, token_id: u32) {
        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &owner);
    }

    pub fn approve(env: Env, _caller: Address, spender: Address, token_id: u32, _expiry: u32) {
        env.storage()
            .persistent()
            .set(&NftKey::Approved(token_id), &spender);
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32) {
        let approved: Address = env
            .storage()
            .persistent()
            .get(&NftKey::Approved(token_id))
            .expect("no approval");
        assert_eq!(approved, spender);
        let owner: Address = env
            .storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .expect("no owner");
        assert_eq!(owner, from);
        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &to);
    }

    pub fn owner_of(env: Env, token_id: u32) -> Address {
        env.storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .expect("token not found")
    }

    /// Configure an NFT-level royalty of `bps` basis points paid to `recipient`.
    pub fn set_royalty(env: Env, recipient: Address, bps: i128) {
        env.storage()
            .persistent()
            .set(&NftKey::Royalty, &(recipient, bps));
    }

    pub fn royalty_info(env: Env, _token_id: u32, sale_price: i128) -> Option<super::contract::RoyaltyInfo> {
        // No royalty by default; `set_royalty` overrides it.
        let (recipient, bps): (Address, i128) = env.storage().persistent().get(&NftKey::Royalty)?;
        Some(super::contract::RoyaltyInfo {
            recipient,
            amount: sale_price * bps / 10_000,
        })
    }
}

// ---------------------------------------------------------------------------
// Test setup
// ---------------------------------------------------------------------------

struct TestEnv<'a> {
    env: Env,
    client: MarketplaceContractClient<'a>,
    marketplace: Address,
    token: Address,
    nft: Address,
    admin: Address,
    seller: Address,
    buyer: Address,
    royalty_recipient: Address,
}

fn setup<'a>(env: &'a Env) -> TestEnv<'a> {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let royalty_recipient = Address::generate(env);

    // Deploy payment token and mint to buyer
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(env, &token).mint(&buyer, &10_000i128);

    // Deploy mock NFT and init token_id=1 owned by seller
    let nft = env.register_contract(None, MockNft);
    MockNftClient::new(env, &nft).init(&seller, &1u32);

    // Deploy marketplace and initialize
    let marketplace = env.register_contract(None, MarketplaceContract);
    MarketplaceContractClient::new(env, &marketplace).initialize(
        &admin,
        &token,
        &250u32,
        &royalty_recipient,
    );

    // Approve marketplace as NFT spender for token_id=1
    MockNftClient::new(env, &nft).approve(
        &seller,
        &marketplace,
        &1u32,
        &(env.ledger().sequence() + 10_000),
    );

    let client = MarketplaceContractClient::new(env, &marketplace);

    TestEnv {
        env: env.clone(),
        client,
        marketplace,
        token,
        nft,
        admin,
        seller,
        buyer,
        royalty_recipient,
    }
}

fn tok<'a>(env: &'a Env, token: &Address) -> TokenClient<'a> {
    TokenClient::new(env, token)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_rejects_duplicate() {
    let env = Env::default();
    let t = setup(&env);
    let res = t
        .client
        .try_initialize(&t.admin, &t.token, &100u32, &t.royalty_recipient);
    assert!(res.is_err());
}

#[test]
fn test_list_and_get_listing() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    assert_eq!(id, 0);
    let listing = t.client.get_listing(&id).expect("listing");
    assert!(listing.active);
    assert_eq!(listing.price, 1_000);
    assert_eq!(listing.seller, t.seller);
}

#[test]
fn test_list_rejects_zero_price() {
    let env = Env::default();
    let t = setup(&env);
    let res = t.client.try_list(&t.seller, &t.nft, &1u32, &0i128);
    assert!(res.is_err());
}

#[test]
fn test_buy_happy_path() {
    let env = Env::default();
    let t = setup(&env);

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let seller_before = tok(&env, &t.token).balance(&t.seller);
    let royalty_before = tok(&env, &t.token).balance(&t.royalty_recipient);
    let buyer_before = tok(&env, &t.token).balance(&t.buyer);

    t.client.buy(&t.buyer, &id);

    let royalty = (price * 250) / 10_000; // 25
    let seller_amount = price - royalty; // 975

    assert_eq!(
        tok(&env, &t.token).balance(&t.seller),
        seller_before + seller_amount
    );
    assert_eq!(
        tok(&env, &t.token).balance(&t.royalty_recipient),
        royalty_before + royalty
    );
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before - price);

    // Verify NFT transferred to buyer
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.buyer);

    // Listing now inactive
    let listing = t.client.get_listing(&id).expect("listing");
    assert!(!listing.active);
}

#[test]
fn test_buy_inactive_listing_fails() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128);
    t.client.buy(&t.buyer, &id);
    let res = t.client.try_buy(&t.buyer, &id);
    assert!(res.is_err());
}

#[test]
fn test_cancel_listing() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128);
    t.client.cancel(&t.seller, &id);
    let listing = t.client.get_listing(&id).expect("listing");
    assert!(!listing.active);
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128);
    t.client.cancel(&t.seller, &id);
    let res = t.client.try_cancel(&t.seller, &id);
    assert!(res.is_err());
}

#[test]
fn test_non_seller_cannot_cancel() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128);
    let other = Address::generate(&env);
    let res = t.client.try_cancel(&other, &id);
    assert!(res.is_err());
}

#[test]
fn test_invalid_royalty_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let marketplace = env.register_contract(None, MarketplaceContract);
    let res = MarketplaceContractClient::new(&env, &marketplace)
        .try_initialize(&admin, &token, &10_001u32, &admin);
    assert!(res.is_err());
}

#[test]
fn test_nft_royalty_override_honored_on_buy() {
    let env = Env::default();
    env.mock_all_auths();
    let t = setup(&env);

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let marketplace_royalty_bps: u32 = env.as_contract(&t.marketplace, || {
        env.storage()
            .instance()
            .get(&DataKey::RoyaltyBps)
            .unwrap_or(0)
    });
    let marketplace_royalty = (price * marketplace_royalty_bps as i128) / 10_000;

    let before_marketplace_royalty = tok(&env, &t.token).balance(&t.royalty_recipient);
    let before_seller = tok(&env, &t.token).balance(&t.seller);

    t.client.buy(&t.buyer, &id);

    let after_marketplace_royalty = tok(&env, &t.token).balance(&t.royalty_recipient);
    assert_eq!(after_marketplace_royalty, before_marketplace_royalty + marketplace_royalty,
               "marketplace royalty should be used when NFT contract returns no royalty");
    assert_eq!(tok(&env, &t.token).balance(&t.seller),
               before_seller + price - marketplace_royalty,
               "seller should receive price minus marketplace royalty");
}

// ---------------------------------------------------------------------------
// Atomic 3-way sale split (creator royalty / platform fee / seller)
// ---------------------------------------------------------------------------

/// Configure a `fee_bps` platform fee and return its recipient.
fn enable_platform_fee(t: &TestEnv, fee_bps: u32) -> Address {
    let platform = Address::generate(&t.env);
    t.client.set_platform_fee(&t.admin, &fee_bps, &platform);
    platform
}

/// Balances of (seller, creator, platform, buyer, marketplace) for `creator`/`platform`.
fn balances(t: &TestEnv, creator: &Address, platform: &Address) -> [i128; 5] {
    let tc = tok(&t.env, &t.token);
    [
        tc.balance(&t.seller),
        tc.balance(creator),
        tc.balance(platform),
        tc.balance(&t.buyer),
        tc.balance(&t.marketplace),
    ]
}

#[test]
fn test_three_way_split_exact_amounts() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let split = t.client.get_sale_split(&id);
    assert_eq!(split.sale_price, price);
    assert_eq!(split.creator_royalty, 25); // 2.5 % marketplace default
    assert_eq!(split.creator, t.royalty_recipient);
    assert_eq!(split.platform_fee, 50); // 5 %
    assert_eq!(split.platform, Some(platform.clone()));
    assert_eq!(split.seller_amount, 925);

    let before = balances(&t, &t.royalty_recipient, &platform);
    t.client.buy(&t.buyer, &id);
    let after = balances(&t, &t.royalty_recipient, &platform);

    assert_eq!(after[0] - before[0], 925, "seller");
    assert_eq!(after[1] - before[1], 25, "creator");
    assert_eq!(after[2] - before[2], 50, "platform");
    assert_eq!(before[3] - after[3], price, "buyer pays exactly the price");
    assert_eq!(after[4], 0, "nothing stranded in the marketplace");
}

#[test]
fn test_three_way_split_seller_receives_exact_remainder_on_rounding() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);

    // 2.5 % of 999 = 24.975 → 24; 5 % of 999 = 49.95 → 49; seller gets 999 - 24 - 49 = 926.
    let price = 999i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let before = balances(&t, &t.royalty_recipient, &platform);
    t.client.buy(&t.buyer, &id);
    let after = balances(&t, &t.royalty_recipient, &platform);

    let seller = after[0] - before[0];
    let creator = after[1] - before[1];
    let fee = after[2] - before[2];
    assert_eq!((seller, creator, fee), (926, 24, 49));
    assert_eq!(seller + creator + fee, price, "no rounding drift");
    assert_eq!(after[4], 0);
}

#[test]
fn test_three_way_split_uses_nft_royalty() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);
    let creator = Address::generate(&env);
    MockNftClient::new(&env, &t.nft).set_royalty(&creator, &1_000); // 10 %

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let marketplace_royalty_before = tok(&env, &t.token).balance(&t.royalty_recipient);
    let before = balances(&t, &creator, &platform);
    t.client.buy(&t.buyer, &id);
    let after = balances(&t, &creator, &platform);

    assert_eq!(after[0] - before[0], 850, "seller");
    assert_eq!(after[1] - before[1], 100, "creator (NFT royalty)");
    assert_eq!(after[2] - before[2], 50, "platform");
    assert_eq!(
        tok(&env, &t.token).balance(&t.royalty_recipient),
        marketplace_royalty_before,
        "marketplace default royalty is not charged when the NFT sets one"
    );
}

#[test]
fn test_three_way_split_zero_nft_royalty() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);
    let creator = Address::generate(&env);
    MockNftClient::new(&env, &t.nft).set_royalty(&creator, &0);

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);

    let before = balances(&t, &creator, &platform);
    t.client.buy(&t.buyer, &id);
    let after = balances(&t, &creator, &platform);

    assert_eq!(after[0] - before[0], 950, "seller gets everything but the fee");
    assert_eq!(after[1] - before[1], 0, "zero royalty pays nothing");
    assert_eq!(after[2] - before[2], 50, "platform");
}

#[test]
fn test_split_without_platform_fee() {
    let env = Env::default();
    let t = setup(&env);

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    let split = t.client.get_sale_split(&id);
    assert_eq!(split.platform, None);
    assert_eq!(split.platform_fee, 0);
    assert_eq!(split.creator_royalty, 25);
    assert_eq!(split.seller_amount, 975);
}

#[test]
fn test_payout_exceeding_price_rejected_atomically() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);
    let creator = Address::generate(&env);
    MockNftClient::new(&env, &t.nft).set_royalty(&creator, &9_600); // 96 % + 5 % > 100 %

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);

    let before = balances(&t, &creator, &platform);
    let res = t.client.try_buy(&t.buyer, &id);
    assert!(matches!(res, Err(Ok(MarketplaceError::PayoutExceedsPrice))));

    // Nothing moved and the listing is still open.
    assert_eq!(balances(&t, &creator, &platform), before);
    assert!(t.client.get_listing(&id).expect("listing").active);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.seller);
}

#[test]
fn test_accept_offer_uses_three_way_split() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    t.client.make_offer(&t.buyer, &id, &800i128);

    let before = balances(&t, &t.royalty_recipient, &platform);
    t.client.accept_offer(&t.seller, &id, &t.buyer);
    let after = balances(&t, &t.royalty_recipient, &platform);

    assert_eq!(after[0] - before[0], 740, "seller");
    assert_eq!(after[1] - before[1], 20, "creator");
    assert_eq!(after[2] - before[2], 40, "platform");
    assert_eq!(after[4], 0, "escrow fully disbursed");
}

#[test]
fn test_set_platform_fee_validation() {
    let env = Env::default();
    let t = setup(&env);
    let platform = Address::generate(&env);

    let res = t.client.try_set_platform_fee(&t.admin, &10_001u32, &platform);
    assert!(matches!(res, Err(Ok(MarketplaceError::InvalidPlatformFee))));

    let res = t.client.try_set_platform_fee(&t.seller, &500u32, &platform);
    assert!(matches!(res, Err(Ok(MarketplaceError::NotAuthorized))));

    t.client.set_platform_fee(&t.admin, &500u32, &platform);
    assert_eq!(t.client.get_platform_fee(), (500u32, Some(platform)));
}

// ---------------------------------------------------------------------------
// Dutch auction linear price decay
// ---------------------------------------------------------------------------

fn dutch(
    starting_price: i128,
    reserve_price: i128,
    start_time: u64,
    duration: u64,
) -> DutchAuction {
    DutchAuction {
        starting_price,
        reserve_price,
        start_time,
        duration,
    }
}

#[test]
fn test_dutch_price_curve() {
    let a = dutch(1_000, 200, 1_000, 100);
    let price = |now| dutch_auction_price(&a, now).unwrap();

    assert_eq!(price(0), 1_000, "before start");
    assert_eq!(price(1_000), 1_000, "at start");
    assert_eq!(price(1_025), 800, "quarter");
    assert_eq!(price(1_050), 600, "midpoint");
    assert_eq!(price(1_075), 400, "three quarters");
    assert_eq!(price(1_099), 208, "just before end");
    assert_eq!(price(1_100), 200, "at duration");
    assert_eq!(price(50_000), 200, "long after duration");
}

#[test]
fn test_dutch_price_rounds_toward_reserve_and_never_below() {
    let a = dutch(1_000, 1, 0, 3);
    assert_eq!(dutch_auction_price(&a, 1).unwrap(), 667); // 1000 - 999/3
    assert_eq!(dutch_auction_price(&a, 2).unwrap(), 334); // 1000 - 1998/3
    assert_eq!(dutch_auction_price(&a, 3).unwrap(), 1);

    // Monotonically non-increasing and bounded by [reserve, starting].
    let b = dutch(10_000, 2_500, 0, 37);
    let mut prev = i128::MAX;
    for now in 0..60u64 {
        let p = dutch_auction_price(&b, now).unwrap();
        assert!(p <= prev && (2_500..=10_000).contains(&p));
        prev = p;
    }
}

#[test]
fn test_dutch_listing_price_across_timestamps() {
    let env = Env::default();
    let t = setup(&env);
    env.ledger().with_mut(|l| l.timestamp = 10_000);

    let id = t
        .client
        .list_dutch(&t.seller, &t.nft, &1u32, &1_000i128, &200i128, &100u64);
    assert_eq!(
        t.client.get_dutch_auction(&id),
        Some(dutch(1_000, 200, 10_000, 100))
    );

    for (now, expected) in [
        (10_000u64, 1_000i128),
        (10_010, 920),
        (10_050, 600),
        (10_090, 280),
        (10_100, 200),
        (20_000, 200),
    ] {
        env.ledger().with_mut(|l| l.timestamp = now);
        assert_eq!(t.client.get_current_price(&id), expected, "at t={now}");
    }
}

#[test]
fn test_buy_dutch_at_midpoint_charges_decayed_price_with_split() {
    let env = Env::default();
    let t = setup(&env);
    let platform = enable_platform_fee(&t, 500);
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let id = t
        .client
        .list_dutch(&t.seller, &t.nft, &1u32, &1_000i128, &200i128, &100u64);
    env.ledger().with_mut(|l| l.timestamp = 1_050);

    let before = balances(&t, &t.royalty_recipient, &platform);
    t.client.buy(&t.buyer, &id);
    let after = balances(&t, &t.royalty_recipient, &platform);

    // Price 600: royalty 15 (2.5 %), fee 30 (5 %), seller 555.
    assert_eq!(before[3] - after[3], 600, "buyer pays the decayed price");
    assert_eq!(after[0] - before[0], 555);
    assert_eq!(after[1] - before[1], 15);
    assert_eq!(after[2] - before[2], 30);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.buyer);
}

#[test]
fn test_buy_dutch_after_duration_charges_reserve() {
    let env = Env::default();
    let t = setup(&env);

    let id = t
        .client
        .list_dutch(&t.seller, &t.nft, &1u32, &1_000i128, &200i128, &100u64);
    env.ledger().with_mut(|l| l.timestamp += 1_000);

    let buyer_before = tok(&env, &t.token).balance(&t.buyer);
    t.client.buy(&t.buyer, &id);
    assert_eq!(buyer_before - tok(&env, &t.token).balance(&t.buyer), 200);
}

#[test]
fn test_list_dutch_rejects_invalid_params() {
    let env = Env::default();
    let t = setup(&env);
    for (start, reserve, duration) in [
        (1_000i128, 0i128, 100u64), // reserve must be positive
        (1_000, 1_000, 100),        // starting must exceed reserve
        (500, 1_000, 100),
        (1_000, 200, 0), // zero duration
    ] {
        let res = t
            .client
            .try_list_dutch(&t.seller, &t.nft, &1u32, &start, &reserve, &duration);
        assert!(matches!(res, Err(Ok(MarketplaceError::InvalidDutchAuction))));
    }
}

#[test]
fn test_dutch_offer_must_be_below_current_price() {
    let env = Env::default();
    let t = setup(&env);

    let id = t
        .client
        .list_dutch(&t.seller, &t.nft, &1u32, &1_000i128, &200i128, &100u64);
    env.ledger().with_mut(|l| l.timestamp += 50); // price now 600

    let res = t.client.try_make_offer(&t.buyer, &id, &700i128);
    assert!(matches!(res, Err(Ok(MarketplaceError::InvalidOfferAmount))));
    t.client.make_offer(&t.buyer, &id, &550i128);
    assert_eq!(t.client.get_offer(&id, &t.buyer), Some(550));
}
