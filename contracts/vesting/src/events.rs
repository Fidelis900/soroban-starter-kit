use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, beneficiary: &Address, amount: i128, cliff_ledger: u32, end_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "initialized"), beneficiary.clone()),
        (amount, cliff_ledger, end_ledger),
    );
}

pub fn claimed(env: &Env, beneficiary: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "claimed"), beneficiary.clone()), amount);
}

pub fn revoked(env: &Env, beneficiary: &Address, ledger: u32, returned: i128) {
    env.events().publish(
        (Symbol::new(env, "revoked"), beneficiary.clone()),
        (ledger, returned),
    );
}

pub fn admin_released(env: &Env, beneficiary: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "admin_released"), beneficiary.clone()),
        amount,
    );
}

pub fn rotation_proposed(
    env: &Env,
    current: &Address,
    new_beneficiary: &Address,
    proposer: &Address,
    eta_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "rotation_proposed"), current.clone()),
        (new_beneficiary.clone(), proposer.clone(), eta_ledger),
    );
}

pub fn rotation_cancelled(env: &Env, current: &Address, by: &Address) {
    env.events().publish(
        (Symbol::new(env, "rotation_cancelled"), current.clone()),
        by.clone(),
    );
}

pub fn rotation_executed(env: &Env, old: &Address, new_beneficiary: &Address) {
    env.events().publish(
        (Symbol::new(env, "rotation_executed"), old.clone()),
        new_beneficiary.clone(),
    );
}
