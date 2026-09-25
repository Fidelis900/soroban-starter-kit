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
