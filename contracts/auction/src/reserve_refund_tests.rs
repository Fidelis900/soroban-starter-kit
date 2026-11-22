//! Tests for reserve price floor settlement (issue #1482) and instant outbid
//! refunds (issue #1483).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

use soroban_nft_template::{NftContract, NftContractClient};
use soroban_sdk::{
    Address, Env, IntoVal, String, Symbol,
    testutils::{Address as _, Events as _, IssuerFlags, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

use crate::{AuctionContract, AuctionContractClient, AuctionError};

const FUNDS: i128 = 100_000;

struct Setup<'a> {
    env: Env,
    client: AuctionContractClient<'a>,
    seller: Address,
    b1: Address,
    b2: Address,
    token: Address,
    sac: soroban_sdk::testutils::StellarAssetContract,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let b1 = Address::generate(&env);
    let b2 = Address::generate(&env);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token = sac.address();
    let admin = StellarAssetClient::new(&env, &token);
    admin.mint(&b1, &FUNDS);
    admin.mint(&b2, &FUNDS);

    let addr = env.register(AuctionContract, ());
    let client = AuctionContractClient::new(&env, &addr);

    Setup {
        env,
        client,
        seller,
        b1,
        b2,
        token,
        sac,
    }
}

/// Start an English auction with no anti-sniping, cancellation grace or
/// buy-it-now, returning the deadline.
fn start(s: &Setup, reserve_price: Option<i128>, nft: Option<(Address, u32)>) -> u32 {
    let deadline = s.env.ledger().sequence() + 100;
    let (nft_contract, token_id) = match nft {
        Some((c, id)) => (Some(c), Some(id)),
        None => (None, None),
    };
    s.client.start(
        &s.seller,
        &s.token,
        &1_000,
        &100,
        &deadline,
        &reserve_price,
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &nft_contract,
        &token_id,
    );
    deadline
}

fn setup_nft<'a>(env: &'a Env, owner: &Address) -> (NftContractClient<'a>, Address, u32) {
    let admin = Address::generate(env);
    let addr = env.register(NftContract, ());
    let nft = NftContractClient::new(env, &addr);
    nft.initialize(
        &admin,
        &String::from_str(env, "Auction Lots"),
        &String::from_str(env, "LOT"),
        &10,
        &None,
        &None,
        &false,
    );
    let token_id = 1u32;
    nft.mint(
        owner,
        &token_id,
        &String::from_str(env, "ipfs://lot-1"),
        &None,
        &None,
    );
    (nft, addr, token_id)
}

fn balance(s: &Setup, who: &Address) -> i128 {
    TokenClient::new(&s.env, &s.token).balance(who)
}

fn end_auction(s: &Setup, deadline: u32) {
    s.env
        .ledger()
        .with_mut(|l| l.sequence_number = deadline + 1);
    s.client.end();
}

// ---------------------------------------------------------------------------
// Issue #1482: reserve price floor on settlement
// ---------------------------------------------------------------------------

#[test]
fn test_below_reserve_refunds_bidder_and_returns_nft_to_seller() {
    let s = setup();
    let (nft, nft_addr, token_id) = setup_nft(&s.env, &s.seller);
    let deadline = start(&s, Some(5_000), Some((nft_addr, token_id)));
    assert_eq!(nft.owner_of(&token_id), s.client.address);

    s.client.bid(&s.b1, &1_500);
    assert_eq!(balance(&s, &s.b1), FUNDS - 1_500);

    end_auction(&s, deadline);

    let info = s.client.get_info();
    assert!(info.settled);
    // The asset is not transferred to the below-reserve bidder ...
    assert_eq!(nft.owner_of(&token_id), s.seller);
    // ... the bidder is refunded in full ...
    assert_eq!(balance(&s, &s.b1), FUNDS);
    assert_eq!(s.client.get_pending(&s.b1), 0);
    // ... and the seller receives no proceeds.
    assert_eq!(balance(&s, &s.seller), 0);
    assert_eq!(s.client.get_seller_pending(&s.seller), 0);
    assert_eq!(balance(&s, &s.client.address), 0);

    assert!(s.env.events().all().contains(&(
        s.client.address.clone(),
        (Symbol::new(&s.env, "ended_reserve_not_met"), s.b1.clone()).into_val(&s.env),
        (1_500i128, 5_000i128).into_val(&s.env),
    )));

    // Settlement cannot be replayed.
    assert_eq!(s.client.try_end(), Err(Ok(AuctionError::AlreadyEnded)));
}

#[test]
fn test_reserve_met_settles_to_winner() {
    let s = setup();
    let (nft, nft_addr, token_id) = setup_nft(&s.env, &s.seller);
    let deadline = start(&s, Some(5_000), Some((nft_addr, token_id)));

    s.client.bid(&s.b1, &5_000);
    end_auction(&s, deadline);

    assert_eq!(nft.owner_of(&token_id), s.b1);
    assert_eq!(balance(&s, &s.seller), 5_000);
    assert_eq!(balance(&s, &s.b1), FUNDS - 5_000);
}

#[test]
fn test_below_reserve_refund_failure_is_credited_not_stalled() {
    let s = setup();
    let deadline = start(&s, Some(5_000), None);
    s.client.bid(&s.b1, &1_500);

    // The bidder can no longer receive tokens: the push refund will fail.
    s.sac.issuer().set_flag(IssuerFlags::RevocableFlag);
    StellarAssetClient::new(&s.env, &s.token).set_authorized(&s.b1, &false);

    end_auction(&s, deadline);

    // Settlement still completes; the refund is isolated as a pending credit.
    assert!(s.client.get_info().settled);
    assert_eq!(s.client.get_pending(&s.b1), 1_500);
    assert_eq!(balance(&s, &s.seller), 0);

    // Once re-authorized, the bidder recovers the funds via `withdraw`.
    StellarAssetClient::new(&s.env, &s.token).set_authorized(&s.b1, &true);
    s.client.withdraw(&s.b1);
    assert_eq!(balance(&s, &s.b1), FUNDS);
    assert_eq!(s.client.get_pending(&s.b1), 0);
}

#[test]
fn test_negative_reserve_price_rejected() {
    let s = setup();
    let deadline = s.env.ledger().sequence() + 100;
    let res = s.client.try_start(
        &s.seller,
        &s.token,
        &1_000,
        &100,
        &deadline,
        &Some(-1i128),
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
        &None,
    );
    assert_eq!(res, Err(Ok(AuctionError::InvalidAmount)));
}

// ---------------------------------------------------------------------------
// Issue #1483: instant refunds for outbid bidders
// ---------------------------------------------------------------------------

#[test]
fn test_outbid_bidder_refunded_immediately() {
    let s = setup();
    start(&s, None, None);

    s.client.bid(&s.b1, &1_000);
    assert_eq!(balance(&s, &s.b1), FUNDS - 1_000);

    s.client.bid(&s.b2, &1_200);

    // b1's funds are back in their wallet within the same transaction:
    // nothing is left pending and no `withdraw` call is needed.
    assert_eq!(balance(&s, &s.b1), FUNDS);
    assert_eq!(s.client.get_pending(&s.b1), 0);
    assert_eq!(
        s.client.try_withdraw(&s.b1),
        Err(Ok(AuctionError::NothingToWithdraw))
    );

    // Only the current highest bid stays in escrow.
    assert_eq!(balance(&s, &s.client.address), 1_200);
    let info = s.client.get_info();
    assert_eq!(info.highest_bid, 1_200);
    assert_eq!(info.highest_bidder, Some(s.b2.clone()));

    let events = s.env.events().all();
    assert!(events.contains(&(
        s.client.address.clone(),
        (Symbol::new(&s.env, "outbid"), s.b1.clone()).into_val(&s.env),
        (1_000i128, 1_200i128).into_val(&s.env),
    )));
    assert!(events.contains(&(
        s.client.address.clone(),
        (Symbol::new(&s.env, "refund_sent"), s.b1.clone()).into_val(&s.env),
        1_000i128.into_val(&s.env),
    )));
}

#[test]
fn test_repeated_outbids_refund_each_previous_bidder() {
    let s = setup();
    let deadline = start(&s, None, None);

    s.client.bid(&s.b1, &1_000);
    s.client.bid(&s.b2, &1_500);
    s.client.bid(&s.b1, &2_000);

    assert_eq!(balance(&s, &s.b2), FUNDS);
    assert_eq!(balance(&s, &s.b1), FUNDS - 2_000);
    assert_eq!(s.client.get_pending(&s.b1), 0);
    assert_eq!(s.client.get_pending(&s.b2), 0);
    assert_eq!(balance(&s, &s.client.address), 2_000);

    end_auction(&s, deadline);
    assert_eq!(balance(&s, &s.seller), 2_000);
    assert_eq!(balance(&s, &s.client.address), 0);
}

#[test]
fn test_failed_outbid_refund_does_not_block_new_bid() {
    let s = setup();
    let deadline = start(&s, None, None);

    s.client.bid(&s.b1, &1_000);

    // b1 becomes unable to receive tokens before being outbid.
    s.sac.issuer().set_flag(IssuerFlags::RevocableFlag);
    StellarAssetClient::new(&s.env, &s.token).set_authorized(&s.b1, &false);

    // The higher bid is still accepted; b1's refund is isolated as pending.
    s.client.bid(&s.b2, &1_200);
    let info = s.client.get_info();
    assert_eq!(info.highest_bid, 1_200);
    assert_eq!(info.highest_bidder, Some(s.b2.clone()));
    assert_eq!(s.client.get_pending(&s.b1), 1_000);
    assert_eq!(balance(&s, &s.client.address), 2_200);
    assert!(s.env.events().all().contains(&(
        s.client.address.clone(),
        (Symbol::new(&s.env, "refund_failed"), s.b1.clone()).into_val(&s.env),
        1_000i128.into_val(&s.env),
    )));

    // The auction keeps progressing and settles normally.
    end_auction(&s, deadline);
    assert_eq!(balance(&s, &s.seller), 1_200);

    // b1 later recovers the isolated refund.
    StellarAssetClient::new(&s.env, &s.token).set_authorized(&s.b1, &true);
    s.client.withdraw(&s.b1);
    assert_eq!(balance(&s, &s.b1), FUNDS);
    assert_eq!(balance(&s, &s.client.address), 0);
}

#[test]
fn test_self_raise_with_credit_only_pays_delta() {
    let s = setup();
    start(&s, None, None);

    s.client.bid(&s.b1, &1_000);
    // Raising your own lead with credit reuses the escrowed bid.
    s.client.bid_with_credit(&s.b1, &1_500);

    assert_eq!(balance(&s, &s.b1), FUNDS - 1_500);
    assert_eq!(s.client.get_pending(&s.b1), 0);
    assert_eq!(balance(&s, &s.client.address), 1_500);
    assert_eq!(s.client.get_info().highest_bid, 1_500);
}
