use soroban_sdk::{Address, Env, Symbol};

pub fn created(env: &Env, beneficiary: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "created"), beneficiary.clone()), amount);
}

pub fn claimed(env: &Env, beneficiary: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "claimed"), beneficiary.clone()), amount);
}

pub fn revoked(env: &Env, beneficiary: &Address, paid: i128, clawback: i128) {
    env.events().publish(
        (Symbol::new(env, "revoked"), beneficiary.clone()),
        (paid, clawback),
    );
}

pub fn milestone_approved(env: &Env, beneficiary: &Address, milestone_id: u32) {
    env.events().publish(
        (Symbol::new(env, "milestone"), beneficiary.clone()),
        milestone_id,
    );
}
