use soroban_sdk::{Address, Bytes, Env, symbol_short};

pub fn root_set(env: &Env, root: &Bytes) {
    env.events()
        .publish((symbol_short!("root_set"),), root.clone());
}

/// Emitted on every successful claim. The tier id is included both as a topic
/// (so indexers can filter claims per tier) and in the data payload.
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
