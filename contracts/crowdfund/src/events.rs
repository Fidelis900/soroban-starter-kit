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

pub fn campaign_cancelled(env: &Env, creator: &Address) {
    env.events().publish(
        (Symbol::new(env, "campaign_cancelled"), creator.clone()),
        (),
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

pub fn milestone_approved(env: &Env, milestone_id: u32, approvals: i128, quorum: i128) {
    env.events().publish(
        (Symbol::new(env, "milestone_approved"), milestone_id),
        (approvals, quorum),
    );
}

pub fn project_update(env: &Env, creator: &Address, update_hash: &soroban_sdk::BytesN<32>, uri: &soroban_sdk::String) {
    env.events().publish(
        (Symbol::new(env, "project_update"), creator.clone()),
        (update_hash.clone(), uri.clone()),
    );
}
