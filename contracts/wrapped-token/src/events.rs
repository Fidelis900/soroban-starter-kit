use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, token: &Address) {
    env.events()
        .publish((Symbol::new(env, "initialized"),), (admin.clone(), token.clone()));
}

pub fn wrapped(env: &Env, user: &Address, amount: i128, total: i128) {
    env.events()
        .publish((Symbol::new(env, "wrapped"),), (user.clone(), amount, total));
}

pub fn unwrapped(env: &Env, user: &Address, amount: i128, total: i128) {
    env.events()
        .publish((Symbol::new(env, "unwrapped"),), (user.clone(), amount, total));
}

/// Emitted when the minimum wrap/unwrap amount configuration changes.
pub fn min_amount_updated(env: &Env, admin: &Address, min_amount: Option<i128>) {
    env.events().publish(
        (Symbol::new(env, "min_amount_updated"),),
        (admin.clone(), min_amount),
    );
}

/// Emitted when the contract is paused. Only used when the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
pub fn paused(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "paused"), admin.clone()), ());
}

/// Emitted when the contract is unpaused. Only used when the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
pub fn unpaused(env: &Env, admin: &Address) {
    env.events()
        .publish((Symbol::new(env, "unpaused"), admin.clone()), ());
}

/// Emitted when the `wrap` pause flag changes. Only used when the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
pub fn wrap_paused(env: &Env, admin: &Address, paused: bool) {
    env.events()
        .publish((Symbol::new(env, "wrap_paused"), admin.clone()), paused);
}

/// Emitted when the `unwrap` pause flag changes. Only used when the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
pub fn unwrap_paused(env: &Env, admin: &Address, paused: bool) {
    env.events()
        .publish((Symbol::new(env, "unwrap_paused"), admin.clone()), paused);
}

/// Emitted when a protocol fee is collected. `op` is `wrap` or `unwrap`.
pub fn fee_collected(env: &Env, op: Symbol, payer: &Address, recipient: &Address, fee: i128) {
    let topics = (Symbol::new(env, "fee_collected"), op);
    env.events()
        .publish(topics, (payer.clone(), recipient.clone(), fee));
}

/// Emitted when the admin updates the protocol fee configuration.
pub fn fees_updated(env: &Env, admin: &Address, wrap_fee_bps: u32, unwrap_fee_bps: u32) {
    env.events().publish(
        (Symbol::new(env, "fees_updated"), admin.clone()),
        (wrap_fee_bps, unwrap_fee_bps),
    );
}

/// Proof-of-reserve audit event (#1436): emitted when the wrapped supply exceeds
/// the underlying collateral held by the contract.
/// Data: `(total_wrapped, total_locked, deficit)`.
pub fn reserve_discrepancy(env: &Env, total_wrapped: i128, total_locked: i128, deficit: i128) {
    env.events().publish(
        (Symbol::new(env, "reserve_discrepancy"),),
        (total_wrapped, total_locked, deficit),
    );
}
