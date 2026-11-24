use soroban_sdk::{Address, BytesN, Env, Symbol};

pub fn initialized(
    env: &Env,
    admin: &Address,
    beneficiary: &Address,
    release_ledger: u32,
    amount: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "initialized"),
            admin.clone(),
            beneficiary.clone(),
        ),
        (release_ledger, amount),
    );
}

pub fn released(env: &Env, beneficiary: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "released"), beneficiary.clone()), amount);
}

pub fn cancelled(env: &Env, admin: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), admin.clone()), amount);
}

pub fn beneficiary_reassigned(env: &Env, admin: &Address, old_beneficiary: &Address, new_beneficiary: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "beneficiary_reassigned"),
            admin.clone(),
            old_beneficiary.clone(),
            new_beneficiary.clone(),
        ),
        (),
    );
}

pub fn beneficiary_proposed(env: &Env, current_beneficiary: &Address, new_beneficiary: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "beneficiary_proposed"),
            current_beneficiary.clone(),
            new_beneficiary.clone(),
        ),
        (),
    );
}

pub fn beneficiary_accepted(env: &Env, old_beneficiary: &Address, new_beneficiary: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "beneficiary_accepted"),
            old_beneficiary.clone(),
            new_beneficiary.clone(),
        ),
        (),
    );
}

pub fn early_released(
    env: &Env,
    beneficiary: &Address,
    penalty_recipient: &Address,
    amount: i128,
    penalty: i128,
    net: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "early_released"),
            beneficiary.clone(),
            penalty_recipient.clone(),
        ),
        (amount, penalty, net),
    );
}

pub fn deposited(env: &Env, staking_contract: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "deposited"), staking_contract.clone()),
        amount,
    );
}

pub fn token_deposited(env: &Env, token: &Address, amount: i128, release_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "token_deposited"), token.clone()),
        (amount, release_ledger),
    );
}

pub fn partial_released(env: &Env, token: &Address, beneficiary: &Address, amount: i128) {
    env.events().publish(
        (
            Symbol::new(env, "partial_released"),
            token.clone(),
            beneficiary.clone(),
        ),
        amount,
    );
}

pub fn withdrawn(env: &Env, staking_contract: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "withdrawn"), staking_contract.clone()),
        amount,
    );
}

pub fn yield_distributed(
    env: &Env,
    beneficiary: &Address,
    principal: i128,
    yield_recipient: &Address,
    yield_amount: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "yield_distributed"),
            beneficiary.clone(),
            yield_recipient.clone(),
        ),
        (principal, yield_amount),
    );
}

pub fn tranche_rescheduled(env: &Env, tranche_id: u32, old_ledger: u32, new_ledger: u32) {
    env.events().publish(
        (Symbol::new(env, "tranche_rescheduled"), tranche_id),
        (old_ledger, new_ledger),
    );
}

pub fn tx_queued(env: &Env, tx_hash: &BytesN<32>, target: &Address, eta: u32) {
    env.events().publish(
        (Symbol::new(env, "tx_queued"), tx_hash.clone()),
        (target.clone(), eta),
    );
}

pub fn tx_executed(env: &Env, tx_hash: &BytesN<32>, target: &Address) {
    env.events().publish(
        (Symbol::new(env, "tx_executed"), tx_hash.clone()),
        target.clone(),
    );
}

pub fn tx_cancelled(env: &Env, tx_hash: &BytesN<32>, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "tx_cancelled"), tx_hash.clone()),
        admin.clone(),
    );
}

pub fn grace_period_set(env: &Env, admin: &Address, grace_period: u32) {
    env.events().publish(
        (Symbol::new(env, "grace_period_set"), admin.clone()),
        grace_period,
    );
}
