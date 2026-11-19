use soroban_sdk::{Address, Bytes, Env, Symbol};

pub fn root_set(env: &Env, round_id: u32, root: &Bytes) {
    env.events().publish(
        (Symbol::new(env, "root_set"), round_id),
        root.clone(),
    );
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

pub fn decay_remainder(env: &Env, treasury: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "decay_remainder"),),
        (treasury.clone(), token.clone(), amount),
    );
}

pub fn claimed(env: &Env, recipient: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("claimed"),),
        (recipient.clone(), token.clone(), amount),
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
pub fn claimed(env: &Env, round_id: u32, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "claimed"), round_id),
        (recipient.clone(), amount),
    );
}

pub fn claimed_for(env: &Env, round_id: u32, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "claimed_for"), round_id),
        (recipient.clone(), amount),
    );
}

pub fn batch_claimed(env: &Env, round_id: u32, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "batch_claimed"), round_id),
        (recipient.clone(), amount),
    );
}

pub fn unclaimed_swept(env: &Env, recipient: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "UnclaimedSwept"),),
        (recipient.clone(), amount),
    );
}
