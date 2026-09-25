use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, token: &Address) {
    env.events()
        .publish((Symbol::new(env, "initialized"),), (admin.clone(), token.clone()));
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
