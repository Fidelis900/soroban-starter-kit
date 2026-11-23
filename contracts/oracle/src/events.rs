use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, staleness_threshold: u32) {
    env.events().publish(
        (Symbol::new(env, "initialized"), admin.clone()),
        staleness_threshold,
    );
}

pub fn price_updated(env: &Env, admin: &Address, price: i128, ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "price_updated"), admin.clone()),
        (price, ledger),
    );
}

pub fn price_submitted(env: &Env, publisher: &Address, price: i128, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "price_submitted"), publisher.clone()),
        (price, timestamp),
    );
}

/// Emitted when a price update lands within the last 20% of the heartbeat
/// window, or a `get_price` read observes a price close to expiring.
pub fn heartbeat_warning(env: &Env, ledgers_remaining: u32, heartbeat_ledgers: u32) {
    env.events().publish(
        (Symbol::new(env, "heartbeat_warning"),),
        (ledgers_remaining, heartbeat_ledgers),
    );
}

/// Emitted when a price update arrives after the previous price had already
/// gone stale, marking the stale -> fresh transition.
pub fn heartbeat_recovered(env: &Env, stale_for_ledgers: u32, heartbeat_ledgers: u32) {
    env.events().publish(
        (Symbol::new(env, "heartbeat_recovered"),),
        (stale_for_ledgers, heartbeat_ledgers),
pub fn asset_price_updated(
    env: &Env,
    admin: &Address,
    asset: &Symbol,
    price: i128,
    decimals: u32,
    ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "asset_price_updated"), admin.clone()),
        (asset.clone(), price, decimals, ledger),
    );
}
