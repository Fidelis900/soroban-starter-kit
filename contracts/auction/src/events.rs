// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Env, Symbol, contracttype};

/// Payload of the `cancelled_with_compensation` event, emitted when the seller
/// cancels inside the grace window after at least one bid has been placed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionCancelledWithCompensation {
    /// The seller who cancelled the auction.
    pub seller: Address,
    /// The highest bidder at the time of cancellation.
    pub top_bidder: Address,
    /// Compensation fee credited to `top_bidder` on top of their bid refund.
    pub compensation_amount: i128,
}

pub fn started(env: &Env, seller: &Address, start_price: i128, deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "started"), seller.clone()),
        (start_price, deadline),
    );
}

pub fn bid_placed(env: &Env, bidder: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "bid_placed"), bidder.clone()), amount);
}

pub fn outbid(
    env: &Env,
    outbid_bidder: &Address,
    outbid_amount: i128,
    new_highest_bid: i128,
) {
    env.events().publish(
        (Symbol::new(env, "outbid"), outbid_bidder.clone()),
        (outbid_amount, new_highest_bid),
    );
}

pub fn refund_queued(env: &Env, bidder: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "refund_queued"), bidder.clone()), amount);
}

pub fn ended(env: &Env, winner: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "ended"), winner.clone()), amount);
}

pub fn ended_no_bids(env: &Env) {
    env.events()
        .publish((Symbol::new(env, "ended_no_bids"),), ());
}

pub fn ended_reserve_not_met(
    env: &Env,
    highest_bidder: &Address,
    highest_bid: i128,
    reserve_price: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "ended_reserve_not_met"),
            highest_bidder.clone(),
        ),
        (highest_bid, reserve_price),
    );
}

pub fn withdrawn(env: &Env, bidder: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "withdrawn"), bidder.clone()), amount);
}

pub fn deadline_extended(env: &Env, new_deadline: u32) {
    env.events()
        .publish((Symbol::new(env, "deadline_extended"),), new_deadline);
}

pub fn max_deadline_reached(env: &Env, max_deadline: u32) {
    env.events()
        .publish((Symbol::new(env, "max_deadline_reached"),), max_deadline);
}

pub fn cancelled(env: &Env, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), seller.clone()), ());
}

pub fn cancelled_with_compensation(
    env: &Env,
    seller: &Address,
    top_bidder: &Address,
    compensation_amount: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "cancelled_with_compensation"),
            seller.clone(),
        ),
        AuctionCancelledWithCompensation {
            seller: seller.clone(),
            top_bidder: top_bidder.clone(),
            compensation_amount,
        },
    );
}
pub fn credit_applied(env: &Env, bidder: &Address, credit_used: i128, transferred: i128) {
    env.events().publish(
        (Symbol::new(env, "credit_applied"), bidder.clone()),
        (credit_used, transferred),
    );
}

pub fn dutch_started(
    env: &Env,
    seller: &Address,
    start_price: i128,
    floor_price: i128,
    start_ledger: u32,
    duration_ledgers: u32,
) {
    env.events().publish(
        (Symbol::new(env, "dutch_started"), seller.clone()),
        (start_price, floor_price, start_ledger, duration_ledgers),
    );
}

pub fn dutch_bought(env: &Env, buyer: &Address, price: i128) {
    env.events()
        .publish((Symbol::new(env, "dutch_bought"), buyer.clone()), price);
}

pub fn nft_escrowed(env: &Env, nft_contract: &Address, token_id: u32) {
    env.events().publish(
        (Symbol::new(env, "nft_escrowed"), nft_contract.clone()),
        token_id,
    );
}

pub fn nft_released(env: &Env, to: &Address, token_id: u32) {
    env.events()
        .publish((Symbol::new(env, "nft_released"), to.clone()), token_id);
}
