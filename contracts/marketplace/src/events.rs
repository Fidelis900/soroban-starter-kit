use soroban_sdk::{Address, Env, Symbol};

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

pub fn offer_cancelled(env: &Env, listing_id: u64, buyer: &Address) {
    env.events()
        .publish((Symbol::new(env, "offer_cancelled"), listing_id), buyer.clone());
}

pub fn offer_refunded(env: &Env, listing_id: u64, buyer: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "offer_refunded"), listing_id),
        (buyer.clone(), amount),
    );
}

pub fn listing_invalidated(env: &Env, listing_id: u64, seller: &Address) {
    env.events().publish(
        (Symbol::new(env, "listing_invalidated"), listing_id),
        seller.clone(),
    );
}

pub fn payment_token_allowed(env: &Env, token: &Address, allowed: bool) {
    env.events()
        .publish((Symbol::new(env, "payment_token_allowed"), token.clone()), allowed);
}

pub fn whitelist_enabled(env: &Env, enabled: bool) {
    env.events()
        .publish((Symbol::new(env, "whitelist_enabled"),), enabled);
pub fn collection_offer_made(
    env: &Env,
    offer_id: u64,
    buyer: &Address,
    nft_contract: &Address,
    amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "coll_offered"), offer_id),
        (buyer.clone(), nft_contract.clone(), amount),
    );
}

pub fn collection_offer_accepted(
    env: &Env,
    offer_id: u64,
    seller: &Address,
    token_id: u32,
    amount: i128,
) {
    env.events().publish(
        (Symbol::new(env, "coll_offer_accepted"), offer_id),
        (seller.clone(), token_id, amount),
    );
}

pub fn collection_offer_cancelled(env: &Env, offer_id: u64, buyer: &Address) {
    env.events()
        .publish((Symbol::new(env, "coll_offer_cancelled"), offer_id), buyer.clone());
}

pub fn royalty_splits_set(env: &Env, admin: &Address, total_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "royalty_splits"),), (admin.clone(), total_bps));
}
