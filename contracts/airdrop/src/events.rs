use soroban_sdk::{Address, Bytes, Env, Symbol, symbol_short};

pub fn root_set(env: &Env, root: &Bytes) {
    env.events()
        .publish((symbol_short!("root_set"),), root.clone());
}

pub fn paused(env: &Env, paused: bool) {
    env.events().publish((symbol_short!("paused"),), paused);
}

pub fn decay_config_set(env: &Env, treasury: &Address, config: &crate::DecayConfig) {
    env.events().publish(
        (Symbol::new(env, "decay_config_set"),),
        (treasury.clone(), config.clone()),
    );
}

pub fn epoch_root_updated(env: &Env, epoch: u32, root: &Bytes, deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "epoch_root_updated"), epoch),
        (root.clone(), deadline),
    );
}

pub fn decay_remainder(env: &Env, treasury: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "decay_remainder"),),
        (treasury.clone(), token.clone(), amount),
    );
}

/// Emitted on every successful claim.
///
/// The tier id is included both as a topic (so indexers can filter claims per
/// tier) and in the data payload.
pub fn claimed(env: &Env, recipient: &Address, token: &Address, amount: i128, tier: u32) {
    env.events().publish(
        (symbol_short!("claimed"), tier),
        (recipient.clone(), token.clone(), amount, tier),
    );
}

pub fn locked(env: &Env, recipient: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("locked"),),
        (recipient.clone(), token.clone(), amount),
    );
}

pub fn released(env: &Env, recipient: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("released"),),
        (recipient.clone(), token.clone(), amount),
    );
}

/// Emitted when the admin configures or updates the per-address rate limit.
pub fn rate_limit_set(env: &Env, window_ledgers: u32, max_claims: u32) {
    env.events().publish(
        (Symbol::new(env, "rate_limit_set"),),
        (window_ledgers, max_claims),
    );
}

/// Emitted when the admin grants or revokes KYC attestation for an address.
pub fn kyc_attestation_set(env: &Env, address: &Address, verified: bool) {
    env.events().publish(
        (Symbol::new(env, "kyc_attestation_set"), address.clone()),
        verified,
    );
}
