use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, fee_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "initialized"), admin.clone()), fee_bps);
}

pub fn fee_updated(env: &Env, admin: &Address, fee_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "fee_updated"), admin.clone()), fee_bps);
}

pub fn swap_proposed(
    env: &Env,
    party_a: &Address,
    swap_id: u32,
    token_a: &Address,
    amount_a: i128,
    token_b: &Address,
    amount_b: i128,
    expires_at: u32,
    allowed_counterparty: &Option<Address>,
) {
    env.events().publish(
        (Symbol::new(env, "proposed"), party_a.clone()),
        (
            swap_id,
            token_a.clone(),
            amount_a,
            token_b.clone(),
            amount_b,
            expires_at,
            allowed_counterparty.clone(),
        ),
    );
}

pub fn swap_accepted(env: &Env, party_b: &Address, swap_id: u32, fill_amount_a: i128) {
    env.events().publish(
        (Symbol::new(env, "accepted"), party_b.clone()),
        (swap_id, fill_amount_a),
    );
}

pub fn swap_cancelled(env: &Env, party_a: &Address, swap_id: u32) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), party_a.clone()), swap_id);
}

pub fn basket_swap_proposed(env: &Env, party_a: &Address, swap_id: u32, expires_at: u32) {
    env.events().publish(
        (Symbol::new(env, "basket_proposed"), party_a.clone()),
        (swap_id, expires_at),
    );
}

pub fn basket_swap_accepted(env: &Env, party_b: &Address, swap_id: u32) {
    env.events().publish(
        (Symbol::new(env, "basket_accepted"), party_b.clone()),
        swap_id,
    );
}