use soroban_sdk::{Address, Bytes, Env, symbol_short};

pub fn root_set(env: &Env, root: &Bytes) {
    env.events()
        .publish((symbol_short!("root_set"),), root.clone());
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
    );
}
