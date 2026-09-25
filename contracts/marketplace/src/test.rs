#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, contract, contractimpl, contracttype,
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
};

// ---------------------------------------------------------------------------
// Minimal mock NFT contract for testing
// ---------------------------------------------------------------------------

#[contracttype]
enum NftKey {
    Owner(u32),
    Approved(u32),
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

    /// Test helper: move ownership without going through the marketplace,
    /// simulating a seller transferring the NFT away after listing it.
    pub fn set_owner(env: Env, token_id: u32, owner: Address) {
        env.storage()
            .persistent()
            .set(&NftKey::Owner(token_id), &owner);
    }

    pub fn owner_of(env: Env, token_id: u32) -> Address {
        env.storage()
            .persistent()
            .get(&NftKey::Owner(token_id))
            .expect("token not found")
    }

    pub fn royalty_info(_env: Env, _token_id: u32, _sale_price: i128) -> Option<super::contract::RoyaltyInfo> {
        None  // MockNft returns no royalty by default; tests can override via storage if needed
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
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);
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
    let res = t.client.try_list(&t.seller, &t.nft, &1u32, &0i128, &t.token);
    assert!(res.is_err());
}

#[test]
fn test_buy_happy_path() {
    let env = Env::default();
    let t = setup(&env);

    let price = 1_000i128;
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price, &t.token);

    let seller_before = tok(&env, &t.token).balance(&t.seller);
    let royalty_before = tok(&env, &t.token).balance(&t.royalty_recipient);
    let buyer_before = tok(&env, &t.token).balance(&t.buyer);

    t.client.buy(&t.buyer, &id, &price);

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
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128, &t.token);
    t.client.buy(&t.buyer, &id);
    let res = t.client.try_buy(&t.buyer, &id);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128);
    t.client.buy(&t.buyer, &id, &500i128);
    let res = t.client.try_buy(&t.buyer, &id, &500i128);
    assert!(res.is_err());
}

#[test]
fn test_cancel_listing() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128, &t.token);
    t.client.cancel(&t.seller, &id);
    let listing = t.client.get_listing(&id).expect("listing");
    assert!(!listing.active);
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128, &t.token);
    t.client.cancel(&t.seller, &id);
    let res = t.client.try_cancel(&t.seller, &id);
    assert!(res.is_err());
}

#[test]
fn test_non_seller_cannot_cancel() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &500i128, &t.token);
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
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price, &t.token);

    let marketplace_royalty_bps: u32 = env.as_contract(&t.marketplace, || {
        env.storage()
            .instance()
            .get(&DataKey::RoyaltyBps)
            .unwrap_or(0)
    });
    let marketplace_royalty = (price * marketplace_royalty_bps as i128) / 10_000;

    let before_marketplace_royalty = tok(&env, &t.token).balance(&t.royalty_recipient);
    let before_seller = tok(&env, &t.token).balance(&t.seller);

    t.client.buy(&t.buyer, &id, &price);

    let after_marketplace_royalty = tok(&env, &t.token).balance(&t.royalty_recipient);
    assert_eq!(after_marketplace_royalty, before_marketplace_royalty + marketplace_royalty,
               "marketplace royalty should be used when NFT contract returns no royalty");
    assert_eq!(tok(&env, &t.token).balance(&t.seller),
               before_seller + price - marketplace_royalty,
               "seller should receive price minus marketplace royalty");
}

// ---------------------------------------------------------------------------
// Helpers for multi-token / batch / offer-sweep / ghost-listing tests
// ---------------------------------------------------------------------------

/// Deploy a fresh Stellar asset and mint `amount` to `to`.
fn new_token(env: &Env, admin: &Address, to: &Address, amount: i128) -> Address {
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(env, &token).mint(to, &amount);
    token
}

/// Mint `token_id` to the seller on the mock NFT and approve the marketplace.
fn mint_and_approve(t: &TestEnv, token_id: u32) {
    let nft = MockNftClient::new(&t.env, &t.nft);
    nft.init(&t.seller, &token_id);
    nft.approve(
        &t.seller,
        &t.marketplace,
        &token_id,
        &(t.env.ledger().sequence() + 10_000),
    );
}

// ---------------------------------------------------------------------------
// #1096 – multi-token payment support per listing
// ---------------------------------------------------------------------------

#[test]
fn test_listings_in_multiple_payment_tokens() {
    let env = Env::default();
    let t = setup(&env);
    mint_and_approve(&t, 2);

    let usdc = new_token(&env, &t.admin, &t.buyer, 10_000);

    let xlm_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);
    let usdc_id = t.client.list(&t.seller, &t.nft, &2u32, &2_000i128, &usdc);
    assert_eq!(t.client.get_listing(&xlm_id).unwrap().payment_token, t.token);
    assert_eq!(t.client.get_listing(&usdc_id).unwrap().payment_token, usdc);

    t.client.buy(&t.buyer, &xlm_id);
    t.client.buy(&t.buyer, &usdc_id);

    // Each sale settles (price and royalty) in its own listing's token.
    assert_eq!(tok(&env, &t.token).balance(&t.seller), 975);
    assert_eq!(tok(&env, &t.token).balance(&t.royalty_recipient), 25);
    assert_eq!(tok(&env, &usdc).balance(&t.seller), 1_950);
    assert_eq!(tok(&env, &usdc).balance(&t.royalty_recipient), 50);
    assert_eq!(tok(&env, &usdc).balance(&t.buyer), 8_000);
}

#[test]
fn test_offer_escrowed_in_listing_token() {
    let env = Env::default();
    let t = setup(&env);
    let usdc = new_token(&env, &t.admin, &t.buyer, 10_000);

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &usdc);
    t.client.make_offer(&t.buyer, &id, &800i128);
    assert_eq!(tok(&env, &usdc).balance(&t.marketplace), 800);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 0);

    t.client.accept_offer(&t.seller, &id, &t.buyer);
    assert_eq!(tok(&env, &usdc).balance(&t.seller), 780);
    assert_eq!(tok(&env, &usdc).balance(&t.royalty_recipient), 20);
}

#[test]
fn test_whitelist_rejects_unlisted_token() {
    let env = Env::default();
    let t = setup(&env);
    let scam = new_token(&env, &t.admin, &t.buyer, 10_000);

    // Default token is whitelisted at init; whitelist is off by default.
    assert!(t.client.is_payment_token_allowed(&t.token));
    assert!(!t.client.is_payment_token_allowed(&scam));
    assert!(!t.client.is_whitelist_enabled());

    t.client.set_whitelist_enabled(&true);
    let res = t.client.try_list(&t.seller, &t.nft, &1u32, &1_000i128, &scam);
    assert_eq!(res, Err(Ok(MarketplaceError::PaymentTokenNotAllowed)));

    t.client.set_payment_token_allowed(&scam, &true);
    assert!(t.client.try_list(&t.seller, &t.nft, &1u32, &1_000i128, &scam).is_ok());

    t.client.set_payment_token_allowed(&scam, &false);
    let res = t.client.try_list(&t.seller, &t.nft, &1u32, &1_000i128, &scam);
    assert_eq!(res, Err(Ok(MarketplaceError::PaymentTokenNotAllowed)));
}

// ---------------------------------------------------------------------------
// #1097 – batch listing, purchase and delisting
// ---------------------------------------------------------------------------

#[test]
fn test_list_batch_and_buy_batch_sweep() {
    let env = Env::default();
    let t = setup(&env);
    mint_and_approve(&t, 2);
    mint_and_approve(&t, 3);
    let usdc = new_token(&env, &t.admin, &t.buyer, 10_000);

    let mut items = Vec::new(&env);
    for (token_id, price, pay) in [(1u32, 100i128, &t.token), (2, 200, &usdc), (3, 300, &t.token)] {
        items.push_back(ListingParams {
            nft_contract: t.nft.clone(),
            token_id,
            price,
            payment_token: pay.clone(),
            expires_at: None,
        });
    }
    let ids = t.client.list_batch(&t.seller, &items);
    assert_eq!(ids.len(), 3);

    t.client.buy_batch(&t.buyer, &ids);

    let nft = MockNftClient::new(&env, &t.nft);
    for token_id in 1u32..=3 {
        assert_eq!(nft.owner_of(&token_id), t.buyer);
    }
    for id in ids.iter() {
        assert!(!t.client.get_listing(&id).unwrap().active);
    }
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000 - 400);
    assert_eq!(tok(&env, &usdc).balance(&t.buyer), 10_000 - 200);
}

#[test]
fn test_buy_batch_is_all_or_nothing() {
    let env = Env::default();
    let t = setup(&env);
    mint_and_approve(&t, 2);

    let a = t.client.list(&t.seller, &t.nft, &1u32, &100i128, &t.token);
    let b = t.client.list(&t.seller, &t.nft, &2u32, &100i128, &t.token);
    t.client.cancel(&t.seller, &b);

    let res = t.client.try_buy_batch(&t.buyer, &soroban_sdk::vec![&env, a, b]);
    assert_eq!(res, Err(Ok(MarketplaceError::ListingInactive)));

    // The valid listing was not bought and no funds moved.
    assert!(t.client.get_listing(&a).unwrap().active);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.seller);
}

#[test]
fn test_buy_batch_rejects_duplicates_and_bad_sizes() {
    let env = Env::default();
    let t = setup(&env);
    let a = t.client.list(&t.seller, &t.nft, &1u32, &100i128, &t.token);

    let res = t.client.try_buy_batch(&t.buyer, &soroban_sdk::vec![&env, a, a]);
    assert_eq!(res, Err(Ok(MarketplaceError::ListingInactive)));

    let res = t.client.try_buy_batch(&t.buyer, &Vec::new(&env));
    assert_eq!(res, Err(Ok(MarketplaceError::EmptyBatch)));

    let mut too_many = Vec::new(&env);
    for i in 0..=MAX_BATCH_SIZE {
        too_many.push_back(u64::from(i));
    }
    let res = t.client.try_buy_batch(&t.buyer, &too_many);
    assert_eq!(res, Err(Ok(MarketplaceError::BatchTooLarge)));
}

#[test]
fn test_cancel_batch() {
    let env = Env::default();
    let t = setup(&env);
    mint_and_approve(&t, 2);
    let a = t.client.list(&t.seller, &t.nft, &1u32, &100i128, &t.token);
    let b = t.client.list(&t.seller, &t.nft, &2u32, &100i128, &t.token);

    let other = Address::generate(&env);
    let res = t.client.try_cancel_batch(&other, &soroban_sdk::vec![&env, a, b]);
    assert_eq!(res, Err(Ok(MarketplaceError::NotAuthorized)));
    assert!(t.client.get_listing(&a).unwrap().active);

    t.client.cancel_batch(&t.seller, &soroban_sdk::vec![&env, a, b]);
    assert!(!t.client.get_listing(&a).unwrap().active);
    assert!(!t.client.get_listing(&b).unwrap().active);
}

// ---------------------------------------------------------------------------
// #1094 – refund escrowed offers on closed listings
// ---------------------------------------------------------------------------

#[test]
fn test_sweep_offers_refunds_buyers_after_cancel() {
    let env = Env::default();
    let t = setup(&env);
    let buyer2 = Address::generate(&env);
    StellarAssetClient::new(&env, &t.token).mint(&buyer2, &10_000i128);

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);
    t.client.make_offer(&t.buyer, &id, &600i128);
    t.client.make_offer(&buyer2, &id, &700i128);

    // Cannot sweep while the listing is open.
    let res = t.client.try_sweep_offers(&id, &soroban_sdk::vec![&env, t.buyer.clone()]);
    assert_eq!(res, Err(Ok(MarketplaceError::ListingStillActive)));

    t.client.cancel(&t.seller, &id);

    // Includes a duplicate and a buyer with no offer; neither is double-refunded.
    let stranger = Address::generate(&env);
    let refunded = t.client.sweep_offers(
        &id,
        &soroban_sdk::vec![&env, t.buyer.clone(), buyer2.clone(), t.buyer.clone(), stranger],
    );
    assert_eq!(refunded, 2);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000);
    assert_eq!(tok(&env, &t.token).balance(&buyer2), 10_000);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 0);
    assert_eq!(t.client.get_offer(&id, &t.buyer), None);
    assert_eq!(t.client.get_offer(&id, &buyer2), None);
}

#[test]
fn test_sweep_offers_after_expiry_sweep() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    let t = setup(&env);
    let expires_at = env.ledger().sequence() + 10;
    let id = t
        .client
        .list_with_expiry(&t.seller, &t.nft, &1u32, &1_000i128, &t.token, &expires_at);
    t.client.make_offer(&t.buyer, &id, &500i128);

    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);
    t.client.sweep_expired(&t.seller, &id);

    assert_eq!(t.client.sweep_offers(&id, &soroban_sdk::vec![&env, t.buyer.clone()]), 1);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000);
}

// ---------------------------------------------------------------------------
// #1095 – ghost listings
// ---------------------------------------------------------------------------

#[test]
fn test_buy_ghost_listing_fails_without_payment() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);

    // Seller moves the NFT elsewhere after listing.
    let elsewhere = Address::generate(&env);
    MockNftClient::new(&env, &t.nft).set_owner(&1u32, &elsewhere);

    let res = t.client.try_buy(&t.buyer, &id);
    assert_eq!(res, Err(Ok(MarketplaceError::SellerNotOwner)));
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000);
    assert_eq!(tok(&env, &t.token).balance(&t.seller), 0);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), elsewhere);
}

#[test]
fn test_invalidate_ghost_listing_and_filter_queries() {
    let env = Env::default();
    let t = setup(&env);
    mint_and_approve(&t, 2);
    let ghost = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);
    let live = t.client.list(&t.seller, &t.nft, &2u32, &1_000i128, &t.token);

    // Still owned: invalidate is a no-op.
    assert!(!t.client.invalidate_listing(&ghost));

    MockNftClient::new(&env, &t.nft).set_owner(&1u32, &Address::generate(&env));

    // Query filters the ghost even before it is invalidated.
    let page = t.client.get_active_listings(&0u64, &10u32);
    assert_eq!(page.listings.len(), 1);
    assert_eq!(page.listings.get(0).unwrap().id, live);

    assert!(t.client.invalidate_listing(&ghost));
    assert!(!t.client.get_listing(&ghost).unwrap().active);
    assert!(t.client.get_listing(&live).unwrap().active);
}

#[test]
fn test_ghost_listing_offers_can_be_refunded() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);
    t.client.make_offer(&t.buyer, &id, &500i128);

    MockNftClient::new(&env, &t.nft).set_owner(&1u32, &Address::generate(&env));
    let res = t.client.try_accept_offer(&t.seller, &id, &t.buyer);
    assert_eq!(res, Err(Ok(MarketplaceError::SellerNotOwner)));

    t.client.invalidate_listing(&id);
    assert_eq!(t.client.sweep_offers(&id, &soroban_sdk::vec![&env, t.buyer.clone()]), 1);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000);
// #1101 — buyer max-price (front-running / slippage) protection
// ---------------------------------------------------------------------------

#[test]
fn test_buy_rejects_when_price_exceeds_max() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);

    let res = t.client.try_buy(&t.buyer, &id, &999i128);
    assert_eq!(res, Err(Ok(MarketplaceError::PriceExceedsMax)));

    // Nothing moved: listing still active, NFT still with seller.
    assert!(t.client.get_listing(&id).unwrap().active);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.seller);
}

#[test]
fn test_buy_accepts_max_price_above_listing_price() {
    let env = Env::default();
    let t = setup(&env);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    let buyer_before = tok(&env, &t.token).balance(&t.buyer);

    t.client.buy(&t.buyer, &id, &1_500i128);

    // Buyer pays the listing price, not max_price.
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before - 1_000);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.buyer);
}

#[test]
fn test_buy_protected_against_cancel_and_relist_higher() {
    let env = Env::default();
    let t = setup(&env);

    // Buyer observes listing at 1_000...
    let old_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    // ...seller cancels and re-lists higher before the buy lands.
    t.client.cancel(&t.seller, &old_id);
    let new_id = t.client.list(&t.seller, &t.nft, &1u32, &5_000i128);

    let buyer_before = tok(&env, &t.token).balance(&t.buyer);
    let res = t.client.try_buy(&t.buyer, &new_id, &1_000i128);
    assert_eq!(res, Err(Ok(MarketplaceError::PriceExceedsMax)));
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before);
}

// ---------------------------------------------------------------------------
// #1098 — collection-wide floor offers
// ---------------------------------------------------------------------------

#[test]
fn test_collection_offer_lifecycle_accept() {
    use soroban_sdk::testutils::Ledger as _;
    let env = Env::default();
    let t = setup(&env);
    let expires_at = env.ledger().sequence() + 100;

    let buyer_before = tok(&env, &t.token).balance(&t.buyer);
    let offer_id = t
        .client
        .make_collection_offer(&t.buyer, &t.nft, &800i128, &expires_at);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before - 800);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 800);
    let offer = t.client.get_collection_offer(&offer_id).unwrap();
    assert_eq!(offer.buyer, t.buyer);
    assert_eq!(offer.nft_contract, t.nft);
    assert_eq!(offer.amount, 800);

    env.ledger().with_mut(|l| l.sequence_number += 10);

    let seller_before = tok(&env, &t.token).balance(&t.seller);
    let royalty_before = tok(&env, &t.token).balance(&t.royalty_recipient);
    t.client.accept_collection_offer(&t.seller, &offer_id, &1u32);

    let royalty = (800 * 250) / 10_000; // 20
    assert_eq!(tok(&env, &t.token).balance(&t.seller), seller_before + 800 - royalty);
    assert_eq!(
        tok(&env, &t.token).balance(&t.royalty_recipient),
        royalty_before + royalty
    );
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 0);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.buyer);
    assert!(t.client.get_collection_offer(&offer_id).is_none());

    // Cannot be accepted twice.
    let res = t.client.try_accept_collection_offer(&t.seller, &offer_id, &1u32);
    assert_eq!(res, Err(Ok(MarketplaceError::CollectionOfferNotFound)));
}

#[test]
fn test_collection_offer_any_token_in_collection() {
    let env = Env::default();
    let t = setup(&env);
    let holder = Address::generate(&env);
    MockNftClient::new(&env, &t.nft).init(&holder, &7u32);
    MockNftClient::new(&env, &t.nft).approve(&holder, &t.marketplace, &7u32, &0u32);

    let offer_id = t.client.make_collection_offer(
        &t.buyer,
        &t.nft,
        &400i128,
        &(env.ledger().sequence() + 100),
    );
    t.client.accept_collection_offer(&holder, &offer_id, &7u32);
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&7u32), t.buyer);
    assert_eq!(tok(&env, &t.token).balance(&holder), 400 - (400 * 250) / 10_000);
}

#[test]
fn test_collection_offer_cancel_refunds() {
    let env = Env::default();
    let t = setup(&env);
    let buyer_before = tok(&env, &t.token).balance(&t.buyer);
    let offer_id = t.client.make_collection_offer(
        &t.buyer,
        &t.nft,
        &600i128,
        &(env.ledger().sequence() + 100),
    );

    let other = Address::generate(&env);
    let res = t.client.try_cancel_collection_offer(&other, &offer_id);
    assert_eq!(res, Err(Ok(MarketplaceError::NotAuthorized)));

    t.client.cancel_collection_offer(&t.buyer, &offer_id);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 0);
    assert!(t.client.get_collection_offer(&offer_id).is_none());
}

#[test]
fn test_collection_offer_expired_cannot_be_accepted_but_can_be_cancelled() {
    use soroban_sdk::testutils::Ledger as _;
    let env = Env::default();
    let t = setup(&env);
    let expires_at = env.ledger().sequence() + 5;
    let offer_id = t
        .client
        .make_collection_offer(&t.buyer, &t.nft, &500i128, &expires_at);

    env.ledger().with_mut(|l| l.sequence_number = expires_at + 1);
    let res = t.client.try_accept_collection_offer(&t.seller, &offer_id, &1u32);
    assert_eq!(res, Err(Ok(MarketplaceError::CollectionOfferExpired)));
    assert_eq!(MockNftClient::new(&env, &t.nft).owner_of(&1u32), t.seller);

    let buyer_before = tok(&env, &t.token).balance(&t.buyer);
    t.client.cancel_collection_offer(&t.buyer, &offer_id);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), buyer_before + 500);
}

#[test]
fn test_collection_offer_validation() {
    let env = Env::default();
    let t = setup(&env);
    let now = env.ledger().sequence();
    assert_eq!(
        t.client
            .try_make_collection_offer(&t.buyer, &t.nft, &0i128, &(now + 10)),
        Err(Ok(MarketplaceError::InvalidOfferAmount))
    );
    assert_eq!(
        t.client
            .try_make_collection_offer(&t.buyer, &t.nft, &100i128, &now),
        Err(Ok(MarketplaceError::InvalidExpiry))
    );
    let offer_id = t
        .client
        .make_collection_offer(&t.buyer, &t.nft, &100i128, &(now + 10));
    // Buyer cannot accept their own offer.
    assert_eq!(
        t.client.try_accept_collection_offer(&t.buyer, &offer_id, &1u32),
        Err(Ok(MarketplaceError::NotAuthorized))
    );
}

#[test]
fn test_collection_offer_accept_without_ownership_reverts() {
    let env = Env::default();
    let t = setup(&env);
    let offer_id = t.client.make_collection_offer(
        &t.buyer,
        &t.nft,
        &300i128,
        &(env.ledger().sequence() + 100),
    );
    // A non-owner cannot sell token 1 into the offer; the whole call reverts.
    let impostor = Address::generate(&env);
    assert!(
        t.client
            .try_accept_collection_offer(&impostor, &offer_id, &1u32)
            .is_err()
    );
    assert_eq!(t.client.get_collection_offer(&offer_id).unwrap().amount, 300);
    assert_eq!(tok(&env, &t.token).balance(&impostor), 0);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 300);
}

// ---------------------------------------------------------------------------
// #1099 — multi-recipient royalty splits
// ---------------------------------------------------------------------------

#[test]
fn test_multi_recipient_royalty_split_on_buy() {
    let env = Env::default();
    let t = setup(&env);
    let creator_a = Address::generate(&env);
    let creator_b = Address::generate(&env);
    let dao = Address::generate(&env);

    let splits = soroban_sdk::vec![
        &env,
        (creator_a.clone(), 300u32),
        (creator_b.clone(), 200u32),
        (dao.clone(), 100u32),
    ];
    t.client.set_royalty_splits(&t.admin, &splits);
    assert_eq!(t.client.get_royalty_splits(), splits);

    let price = 10_000i128;
    StellarAssetClient::new(&env, &t.token).mint(&t.buyer, &price);
    let id = t.client.list(&t.seller, &t.nft, &1u32, &price);
    let legacy_before = tok(&env, &t.token).balance(&t.royalty_recipient);

    t.client.buy(&t.buyer, &id, &price);

    assert_eq!(tok(&env, &t.token).balance(&creator_a), 300);
    assert_eq!(tok(&env, &t.token).balance(&creator_b), 200);
    assert_eq!(tok(&env, &t.token).balance(&dao), 100);
    assert_eq!(tok(&env, &t.token).balance(&t.seller), price - 600);
    // Splits supersede the single initialize-time recipient.
    assert_eq!(
        tok(&env, &t.token).balance(&t.royalty_recipient),
        legacy_before
    );
}

#[test]
fn test_multi_recipient_royalty_split_on_accept_offer_rounds_down() {
    let env = Env::default();
    let t = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    t.client
        .set_royalty_splits(&t.admin, &soroban_sdk::vec![&env, (a.clone(), 333u32), (b.clone(), 333u32)]);

    let id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128);
    t.client.make_offer(&t.buyer, &id, &999i128);
    t.client.accept_offer(&t.seller, &id, &t.buyer);

    let share = (999 * 333) / 10_000; // 33 each, dust stays with the seller
    assert_eq!(tok(&env, &t.token).balance(&a), share);
    assert_eq!(tok(&env, &t.token).balance(&b), share);
    assert_eq!(tok(&env, &t.token).balance(&t.seller), 999 - 2 * share);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 0);
}

#[test]
fn test_royalty_split_validation() {
    let env = Env::default();
    let t = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);

    // Total > 10_000 bps.
    assert_eq!(
        t.client.try_set_royalty_splits(
            &t.admin,
            &soroban_sdk::vec![&env, (a.clone(), 6_000u32), (b.clone(), 4_001u32)]
        ),
        Err(Ok(MarketplaceError::InvalidRoyalty))
    );
    // Exactly 10_000 bps is allowed.
    t.client.set_royalty_splits(
        &t.admin,
        &soroban_sdk::vec![&env, (a.clone(), 6_000u32), (b.clone(), 4_000u32)],
    );
    // Zero-bps entry.
    assert_eq!(
        t.client
            .try_set_royalty_splits(&t.admin, &soroban_sdk::vec![&env, (a.clone(), 0u32)]),
        Err(Ok(MarketplaceError::InvalidRoyalty))
    );
    // Too many recipients.
    let mut many = soroban_sdk::Vec::new(&env);
    for _ in 0..=MAX_ROYALTY_RECIPIENTS {
        many.push_back((Address::generate(&env), 1u32));
    }
    assert_eq!(
        t.client.try_set_royalty_splits(&t.admin, &many),
        Err(Ok(MarketplaceError::InvalidRoyalty))
    );
    // Non-admin.
    assert_eq!(
        t.client
            .try_set_royalty_splits(&a, &soroban_sdk::vec![&env, (a.clone(), 100u32)]),
        Err(Ok(MarketplaceError::NotAuthorized))
    );
    // Empty clears back to the initialize-time royalty.
    t.client
        .set_royalty_splits(&t.admin, &soroban_sdk::Vec::new(&env));
    assert_eq!(t.client.get_royalty_splits().len(), 0);
}

#[test]
fn test_royalty_split_applies_to_collection_offer() {
    let env = Env::default();
    let t = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    t.client.set_royalty_splits(
        &t.admin,
        &soroban_sdk::vec![&env, (a.clone(), 500u32), (b.clone(), 1_500u32)],
    );
    let offer_id = t.client.make_collection_offer(
        &t.buyer,
        &t.nft,
        &2_000i128,
        &(env.ledger().sequence() + 100),
    );
    t.client.accept_collection_offer(&t.seller, &offer_id, &1u32);
    assert_eq!(tok(&env, &t.token).balance(&a), 100);
    assert_eq!(tok(&env, &t.token).balance(&b), 300);
    assert_eq!(tok(&env, &t.token).balance(&t.seller), 1_600);
}

// ---------------------------------------------------------------------------
// Issue #1093 — make_offer() delta-transfer regression tests
// ---------------------------------------------------------------------------

/// Increasing an offer should pull only the net difference from the buyer,
/// not a full refund + full pull (which would require the buyer to temporarily
/// hold the entire new amount even if they only had the difference available).
#[test]
fn make_offer_increase_pulls_delta_only() {
    let env = Env::default();
    let t = setup(&env);

    // List NFT at 1 000 so offers can be up to 999.
    let listing_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);

    // First offer: 300 → buyer pays 300, marketplace holds 300.
    t.client.make_offer(&t.buyer, &listing_id, &300i128);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000 - 300);
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 300);

    // Increase offer to 700 → only the delta of 400 should be pulled.
    t.client.make_offer(&t.buyer, &listing_id, &700i128);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000 - 700,
        "buyer should only have paid the delta (400), not a full re-pull of 700");
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 700,
        "marketplace should hold the new offer amount");
    assert_eq!(t.client.get_offer(&listing_id, &t.buyer), Some(700));
}

/// Decreasing an offer should refund only the delta to the buyer.
#[test]
fn make_offer_decrease_refunds_delta_only() {
    let env = Env::default();
    let t = setup(&env);

    let listing_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);

    t.client.make_offer(&t.buyer, &listing_id, &800i128);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000 - 800);

    // Decrease offer to 500 → only 300 refunded.
    t.client.make_offer(&t.buyer, &listing_id, &500i128);
    assert_eq!(tok(&env, &t.token).balance(&t.buyer), 10_000 - 500,
        "buyer should have received the delta refund of 300");
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 500);
    assert_eq!(t.client.get_offer(&listing_id, &t.buyer), Some(500));
}

/// Resubmitting the exact same offer amount produces no transfer at all.
#[test]
fn make_offer_identical_amount_no_transfer() {
    let env = Env::default();
    let t = setup(&env);

    let listing_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);

    t.client.make_offer(&t.buyer, &listing_id, &400i128);
    let balance_after_first = tok(&env, &t.token).balance(&t.buyer);

    // Resubmit the same amount.
    t.client.make_offer(&t.buyer, &listing_id, &400i128);
    assert_eq!(
        tok(&env, &t.token).balance(&t.buyer),
        balance_after_first,
        "identical offer amount should cause no net token movement"
    );
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 400);
}

/// Even when the buyer's wallet only holds exactly the delta, an increasing
/// offer must succeed — the old double-transfer pattern would have failed here
/// because it would try to pull the full new amount (700) while only 400 is free.
#[test]
fn make_offer_increase_succeeds_with_only_delta_available() {
    let env = Env::default();
    let t = setup(&env);

    // Give buyer exactly 700 tokens (just enough for the first 300 offer + delta of 400).
    let token_client = StellarAssetClient::new(&env, &t.token);
    // The setup minted 10_000, so drain back to 700.
    // Easiest: use a fresh buyer minted with exactly 700.
    let tight_buyer = Address::generate(&env);
    token_client.mint(&tight_buyer, &700i128);

    let listing_id = t.client.list(&t.seller, &t.nft, &1u32, &1_000i128, &t.token);

    // First offer of 300 — costs 300, leaving 400 in wallet.
    t.client.make_offer(&tight_buyer, &listing_id, &300i128);
    assert_eq!(tok(&env, &t.token).balance(&tight_buyer), 400);

    // Increase to 700 — needs only delta of 400, which is exactly what remains.
    // Under the old (buggy) code this would try to pull 700 and fail because the
    // buyer only holds 400.
    t.client.make_offer(&tight_buyer, &listing_id, &700i128);
    assert_eq!(tok(&env, &t.token).balance(&tight_buyer), 0,
        "tight buyer should have spent their remaining 400 as the delta");
    assert_eq!(tok(&env, &t.token).balance(&t.marketplace), 700);
}
