use soroban_sdk::{Address, Bytes, Env, Symbol};

pub fn root_set(env: &Env, round_id: u32, root: &Bytes) {
    env.events().publish(
        (Symbol::new(env, "root_set"), round_id),
        root.clone(),
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
