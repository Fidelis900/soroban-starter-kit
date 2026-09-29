use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, creator: &Address, goal: i128, deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "initialized"), creator.clone()),
        (goal, deadline),
    );
}

pub fn pledged(env: &Env, pledger: &Address, amount: i128, total: i128) {
    env.events().publish(
        (Symbol::new(env, "pledged"), pledger.clone()),
        (amount, total),
    );
}

pub fn withdrawn(env: &Env, pledger: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "withdrawn"), pledger.clone()), amount);
}

pub fn claimed(env: &Env, creator: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "claimed"), creator.clone()), amount);
}

pub fn refunded(env: &Env, pledger: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "refunded"), pledger.clone()), amount);
}

pub fn deadline_extended(env: &Env, creator: &Address, new_deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "deadline_extended"), creator.clone()),
        new_deadline,
    );
}

pub fn creator_claim_expired(env: &Env, claim_deadline: u32) {
    env.events().publish(
        (Symbol::new(env, "creator_claim_expired"),),
        claim_deadline,
    );
}

pub fn milestone_released(env: &Env, milestone_id: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "milestone_released"), milestone_id),
        amount,
    );
}

pub fn milestone_voted(env: &Env, milestone_id: u32, voter: &Address, approve: bool) {
    env.events().publish(
        (Symbol::new(env, "milestone_voted"), milestone_id, voter.clone()),
        approve,
    );
}
