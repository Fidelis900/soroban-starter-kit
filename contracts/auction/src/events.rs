use soroban_sdk::{Address, Env, Symbol};

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

/// Emitted when a late bid pushes the deadline back (anti-sniping). Each
/// auction is its own contract instance, so the contract address is the
/// auction ID.
pub fn auction_extended(env: &Env, auction_id: &Address, new_deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "AuctionExtended"), auction_id.clone()),
        new_deadline,
    );
}

pub fn cancelled(env: &Env, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), seller.clone()), ());
}
