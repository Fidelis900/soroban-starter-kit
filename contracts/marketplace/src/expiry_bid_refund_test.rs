//! Tests for the listing expiration guard (issue #1479) and escrowed bid
//! cancellation refunds (issue #1477).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, IntoVal, Symbol, contract, contractimpl, contracttype,
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

use crate::{
    BidCancelled, ContractError, MarketplaceContract, MarketplaceContractClient, RoyaltyInfo,
};

const BUYER_FUNDS: i128 = 10_000;
const PRICE: i128 = 1_000;
const TOKEN_ID: u32 = 1;

// ---------------------------------------------------------------------------
// Minimal NFT mock: owner tracking + marketplace-driven transfer_from.
// ---------------------------------------------------------------------------

#[contracttype]
enum MockKey {
    Owner(u32),
}

#[contract]
pub struct ExpiryMockNft;

#[contractimpl]
impl ExpiryMockNft {
    pub fn set_owner(env: Env, token_id: u32, owner: Address) {
        env.storage()
            .persistent()
            .set(&MockKey::Owner(token_id), &owner);
    }

    pub fn owner_of(env: Env, token_id: u32) -> Address {
        env.storage()
            .persistent()
            .get(&MockKey::Owner(token_id))
            .expect("token not found")
    }

    pub fn transfer_from(env: Env, _spender: Address, from: Address, to: Address, token_id: u32) {
        let owner: Address = env
            .storage()
            .persistent()
            .get(&MockKey::Owner(token_id))
            .expect("token not found");
        assert_eq!(owner, from);
        env.storage()
            .persistent()
            .set(&MockKey::Owner(token_id), &to);
    }

    pub fn royalty_info(_env: Env, _token_id: u32, _sale_price: i128) -> Option<RoyaltyInfo> {
        None
    }
}

struct Setup<'a> {
    env: Env,
    client: MarketplaceContractClient<'a>,
    token: Address,
    nft: Address,
    seller: Address,
    buyer: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&buyer, &BUYER_FUNDS);

    let nft = env.register(ExpiryMockNft, ());
    ExpiryMockNftClient::new(&env, &nft).set_owner(&TOKEN_ID, &seller);

    let addr = env.register(MarketplaceContract, ());
    let client = MarketplaceContractClient::new(&env, &addr);
    // No marketplace royalty, so balances in the assertions are exact.
    client.initialize(&admin, &token, &0u32, &royalty_recipient);

    env.ledger().with_mut(|l| l.timestamp = 1_000);

    Setup {
        env,
        client,
        token,
        nft,
        seller,
        buyer,
    }
}

fn balance(s: &Setup, who: &Address) -> i128 {
    TokenClient::new(&s.env, &s.token).balance(who)
}

fn set_timestamp(s: &Setup, timestamp: u64) {
    s.env.ledger().with_mut(|l| l.timestamp = timestamp);
}

fn list_expiring_at(s: &Setup, expiration: u64) -> u64 {
    s.client.list_with_expiration(
        &s.seller,
        &s.nft,
        &TOKEN_ID,
        &PRICE,
        &s.token,
        &expiration,
    )
}

// ---------------------------------------------------------------------------
// Issue #1479: listing expiration guard in buy_listing()
// ---------------------------------------------------------------------------

#[test]
fn test_buy_listing_after_expiration_is_rejected() {
    let s = setup();
    let id = list_expiring_at(&s, 2_000);
    assert_eq!(s.client.get_listing(&id).unwrap().expiration, Some(2_000));

    // One second past the expiration.
    set_timestamp(&s, 2_001);
    assert_eq!(
        s.client.try_buy_listing(&s.buyer, &id, &PRICE),
        Err(Ok(ContractError::ListingExpired))
    );
    // `buy` enforces the same guard.
    assert_eq!(
        s.client.try_buy(&s.buyer, &id, &PRICE),
        Err(Ok(ContractError::ListingExpired))
    );

    // Nothing moved and the listing is still open (only sweepable by the seller).
    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS);
    assert_eq!(balance(&s, &s.seller), 0);
    assert_eq!(
        ExpiryMockNftClient::new(&s.env, &s.nft).owner_of(&TOKEN_ID),
        s.seller
    );
    assert!(s.client.get_listing(&id).unwrap().active);

    s.client.sweep_expired(&s.seller, &id);
    assert!(!s.client.get_listing(&id).unwrap().active);
}

#[test]
fn test_buy_listing_at_exact_expiration_succeeds() {
    let s = setup();
    let id = list_expiring_at(&s, 2_000);

    // `timestamp <= expiration` is still purchasable.
    set_timestamp(&s, 2_000);
    s.client.buy_listing(&s.buyer, &id, &PRICE);

    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS - PRICE);
    assert_eq!(balance(&s, &s.seller), PRICE);
    assert_eq!(
        ExpiryMockNftClient::new(&s.env, &s.nft).owner_of(&TOKEN_ID),
        s.buyer
    );
    assert!(!s.client.get_listing(&id).unwrap().active);
}

#[test]
fn test_listing_without_expiration_never_expires() {
    let s = setup();
    let id = s
        .client
        .list(&s.seller, &s.nft, &TOKEN_ID, &PRICE, &s.token);
    assert_eq!(s.client.get_listing(&id).unwrap().expiration, None);

    set_timestamp(&s, u64::MAX);
    s.client.buy_listing(&s.buyer, &id, &PRICE);
    assert_eq!(balance(&s, &s.seller), PRICE);
}

#[test]
fn test_list_with_past_expiration_is_rejected() {
    let s = setup();
    assert_eq!(
        s.client.try_list_with_expiration(
            &s.seller,
            &s.nft,
            &TOKEN_ID,
            &PRICE,
            &s.token,
            &1_000u64,
        ),
        Err(Ok(ContractError::InvalidExpiry))
    );
}

// ---------------------------------------------------------------------------
// Issue #1477: escrowed bid cancellation refunds
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_bid_refunds_full_escrow_and_emits_bid_cancelled() {
    let s = setup();
    let id = s
        .client
        .list(&s.seller, &s.nft, &TOKEN_ID, &PRICE, &s.token);
    let bid = 750i128;
    let expiry = s.env.ledger().sequence() + 100;

    s.client.make_offer(&s.buyer, &id, &bid, &expiry);
    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS - bid);
    assert_eq!(balance(&s, &s.client.address), bid);

    s.client.cancel_offer(&s.buyer, &id);

    // The full escrow is back with the bidder and nothing is left in the contract.
    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS);
    assert_eq!(balance(&s, &s.client.address), 0);
    assert_eq!(s.client.get_offer(&id, &s.buyer), None);

    assert!(s.env.events().all().contains(&(
        s.client.address.clone(),
        (Symbol::new(&s.env, "bid_cancelled"), s.buyer.clone()).into_val(&s.env),
        BidCancelled {
            listing_id: id,
            bidder: s.buyer.clone(),
            refunded_amount: bid,
        }
        .into_val(&s.env),
    )));

    // The bid cannot be cancelled (or refunded) twice.
    assert_eq!(
        s.client.try_cancel_offer(&s.buyer, &id),
        Err(Ok(ContractError::OfferNotFound))
    );
    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS);
}

#[test]
fn test_cancel_raised_bid_refunds_latest_escrow() {
    let s = setup();
    let id = s
        .client
        .list(&s.seller, &s.nft, &TOKEN_ID, &PRICE, &s.token);
    let expiry = s.env.ledger().sequence() + 100;

    // Raising a bid only escrows the delta; cancelling refunds the total.
    s.client.make_offer(&s.buyer, &id, &400, &expiry);
    s.client.make_offer(&s.buyer, &id, &900, &expiry);
    assert_eq!(balance(&s, &s.client.address), 900);

    s.client.cancel_offer(&s.buyer, &id);
    assert_eq!(balance(&s, &s.buyer), BUYER_FUNDS);
    assert_eq!(balance(&s, &s.client.address), 0);
}
