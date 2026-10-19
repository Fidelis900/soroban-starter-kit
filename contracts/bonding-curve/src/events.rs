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

/// Emitted when the admin updates the single-transaction purchase cap.
/// `bps` is the cap expressed in basis points of total max supply.
pub fn max_buy_bps_set(env: &Env, admin: &Address, bps: u32) {
    env.events().publish(
        (Symbol::new(env, "max_buy_bps_set"), admin.clone()),
        (bps,),
    );
}

/// Emitted when the volatility circuit breaker trips and buys are halted.
/// `reference_price` is the historical spot price used as the baseline,
/// `current_price` is the spot price that exceeded the surge threshold, and
/// `ledger` is the ledger sequence at which the breaker tripped.
pub fn price_surge_detected(
    env: &Env,
    reference_price: i128,
    current_price: i128,
    ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "price_surge_detected"),),
        (reference_price, current_price, ledger),
/// Detailed sale telemetry including the dynamic exit-fee breakdown:
/// (trader, tokens_in, tokens_out, new_spot_price, base_fee, exit_fee, reserve_routed).
/// `base_fee` is the standard fee, `exit_fee` is the additional duration-based
/// penalty, and `reserve_routed` is the portion of the exit fee credited to the
/// curve reserve backing.
pub fn sold_with_exit_fee(
    env: &Env,
    seller: &Address,
    tokens_in: i128,
    tokens_out: i128,
    new_spot_price: i128,
    base_fee: i128,
    exit_fee: i128,
    reserve_routed: i128,
) {
    env.events().publish(
        (Symbol::new(env, "sold_exit_fee"), seller.clone()),
        (
            tokens_in,
            tokens_out,
            new_spot_price,
            base_fee,
            exit_fee,
            reserve_routed,
        ),
    );
}

/// Emitted when the curve is configured with the sigmoid model.
/// `midpoint` is the supply inflection point and `steepness` controls curvature.
pub fn sigmoid_configured(env: &Env, midpoint: i128, steepness: u32) {
    env.events().publish(
        (Symbol::new(env, "sigmoid_cfg"),),
        (midpoint, steepness),
    );
}
