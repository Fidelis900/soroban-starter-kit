use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(
    env: &Env,
    beneficiary: &Address,
    token: &Address,
    amount: i128,
    cliff_ledger: u32,
    end_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "initialized"), beneficiary.clone(), token.clone()),
        (amount, cliff_ledger, end_ledger),
    );
}

pub fn claimed(env: &Env, beneficiary: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "claimed"), beneficiary.clone(), token.clone()),
        amount,
    );
}

pub fn revoked(
    env: &Env,
    beneficiary: &Address,
    token: &Address,
    admin: &Address,
    returned: i128,
) {
    env.events().publish(
        (Symbol::new(env, "revoked"), beneficiary.clone(), token.clone()),
        (admin.clone(), returned),
    );
}

pub fn admin_released(env: &Env, admin: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "admin_released"), admin.clone(), token.clone()),
        amount,
    );
}

pub fn admin_released_after_cliff(env: &Env, admin: &Address, token: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "admin_released_after_cliff"), admin.clone(), token.clone()),
        amount,
    );
}
