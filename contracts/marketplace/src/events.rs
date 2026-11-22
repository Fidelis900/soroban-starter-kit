use soroban_sdk::{Address, Env, Symbol};

use crate::storage::SaleSplit;

pub fn listed(env: &Env, listing_id: u64, seller: &Address, price: i128) {
    env.events().publish(
        (Symbol::new(env, "listed"), listing_id),
        (seller.clone(), price),
    );
}

pub fn sold(env: &Env, listing_id: u64, buyer: &Address, price: i128) {
    env.events()
        .publish((Symbol::new(env, "sold"), listing_id), (buyer.clone(), price));
}

pub fn cancelled(env: &Env, listing_id: u64, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), listing_id), seller.clone());
}

pub fn swept(env: &Env, listing_id: u64, seller: &Address) {
    env.events()
        .publish((Symbol::new(env, "swept"), listing_id), seller.clone());
}

pub fn offer_made(env: &Env, listing_id: u64, buyer: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "offered"), listing_id),
        (buyer.clone(), amount),
    );
}

pub fn offer_accepted(env: &Env, listing_id: u64, buyer: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "offer_accepted"), listing_id),
        (buyer.clone(), amount),
    );
}

pub fn dutch_listed(
    env: &Env,
    listing_id: u64,
    seller: &Address,
    starting_price: i128,
    reserve_price: i128,
    duration: u64,
) {
    env.events().publish(
        (Symbol::new(env, "dutch_listed"), listing_id),
        (seller.clone(), starting_price, reserve_price, duration),
    );
}

pub fn platform_fee_set(env: &Env, fee_bps: u32, recipient: &Address) {
    env.events().publish(
        (Symbol::new(env, "platform_fee_set"),),
        (fee_bps, recipient.clone()),
    );
}

pub fn sale_split(env: &Env, listing_id: u64, split: &SaleSplit) {
    env.events().publish(
        (Symbol::new(env, "sale_split"), listing_id),
        (split.seller_amount, split.creator_royalty, split.platform_fee),
    );
}

pub fn offer_cancelled(env: &Env, listing_id: u64, buyer: &Address) {
    env.events()
        .publish((Symbol::new(env, "offer_cancelled"), listing_id), buyer.clone());
}
