use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, token: &Address) {
    env.events()
        .publish((Symbol::new(env, "initialized"),), (admin.clone(), token.clone()));
}

/// Emitted when the curve transitions Active → Graduated.
/// `supply_cap` is the supply threshold that triggered graduation;
/// `reserve` is the reserve balance locked at that moment.
pub fn graduated(env: &Env, supply_cap: i128, reserve: i128) {
    env.events()
        .publish((Symbol::new(env, "graduated"),), (supply_cap, reserve));
}

/// Emitted when the admin calls `migrate_to_amm` to transfer reserve + supply
/// to an external AMM pool. `amm_pool` is the target pool address.
pub fn migrated_to_amm(env: &Env, amm_pool: &Address, reserve: i128, supply: i128) {
    env.events().publish(
        (Symbol::new(env, "migrated_to_amm"), amm_pool.clone()),
        (reserve, supply),
    );
}

/// Detailed purchase telemetry: (trader, tokens_in, tokens_out, new_spot_price, fee_paid).
pub fn bought(
    env: &Env,
    buyer: &Address,
    tokens_in: i128,
    tokens_out: i128,
    new_spot_price: i128,
    fee_paid: i128,
) {
    env.events().publish(
        (Symbol::new(env, "bought"), buyer.clone()),
        (tokens_in, tokens_out, new_spot_price, fee_paid),
    );
}

/// Detailed sale telemetry: (trader, tokens_in, tokens_out, new_spot_price, fee_paid).
pub fn sold(
    env: &Env,
    seller: &Address,
    tokens_in: i128,
    tokens_out: i128,
    new_spot_price: i128,
    fee_paid: i128,
) {
    env.events().publish(
        (Symbol::new(env, "sold"), seller.clone()),
        (tokens_in, tokens_out, new_spot_price, fee_paid),
    );
}
