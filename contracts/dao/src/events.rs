use soroban_sdk::{Address, Env, Symbol, Val};

pub fn initialized(env: &Env, admin: &Address, token: &Address, quorum: i128) {
    env.events().publish(
        (
            Symbol::new(env, "initialized"),
            admin.clone(),
            token.clone(),
        ),
        quorum,
    );
}

pub fn proposal_created(env: &Env, proposer: &Address, proposal_id: u32) {
    env.events()
        .publish((Symbol::new(env, "created"), proposer.clone()), proposal_id);
}

pub fn voted(env: &Env, voter: &Address, proposal_id: u32, support: bool, weight: i128) {
    env.events().publish(
        (Symbol::new(env, "voted"), voter.clone()),
        (proposal_id, support, weight),
    );
}

pub fn proposal_queued(env: &Env, proposal_id: u32, execution_eta: u32) {
    env.events()
        .publish((Symbol::new(env, "queued"),), (proposal_id, execution_eta));
}

pub fn proposal_executed(env: &Env, proposal_id: u32) {
    env.events()
        .publish((Symbol::new(env, "executed"),), proposal_id);
}

/// Emitted after a proposal's action payload is dispatched (issue #1108).
///
/// `return_val` is the `Val` returned by the invoked contract function.
pub fn proposal_action_executed(env: &Env, proposal_id: u32, return_val: Val) {
    env.events().publish(
        (Symbol::new(env, "action_executed"),),
        (proposal_id, return_val),
    );
}

pub fn proposal_cancelled(env: &Env, admin: &Address, proposal_id: u32) {
    env.events()
        .publish((Symbol::new(env, "cancelled"), admin.clone()), proposal_id);
}

/// Emitted when a proposal bond is slashed to the treasury (issue #1106).
pub fn bond_slashed(env: &Env, proposer: &Address, proposal_id: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "bond_slashed"), proposer.clone()),
/// Emitted when the original proposer self-cancels before any votes are cast.
pub fn proposal_proposer_cancelled(env: &Env, proposer: &Address, proposal_id: u32) {
    env.events().publish(
        (Symbol::new(env, "prop_cancelled"), proposer.clone()),
        proposal_id,
    );
}

/// Emitted when a queued proposal is vetoed by the admin / security council.
pub fn proposal_vetoed(env: &Env, admin: &Address, proposal_id: u32) {
    env.events()
        .publish((Symbol::new(env, "vetoed"), admin.clone()), proposal_id);
}

/// Emitted when a token holder locks tokens to vote.
pub fn tokens_locked(env: &Env, voter: &Address, proposal_id: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "tokens_locked"), voter.clone()),
        (proposal_id, amount),
    );
}

/// Emitted when a proposal bond is refunded to the proposer (issue #1106).
pub fn bond_refunded(env: &Env, proposer: &Address, proposal_id: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "bond_refunded"), proposer.clone()),
/// Emitted when a voter reclaims their locked tokens after the voting window closes.
pub fn tokens_unlocked(env: &Env, voter: &Address, proposal_id: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "tokens_unlocked"), voter.clone()),
        (proposal_id, amount),
    );
}

/// Emitted when the adaptive quorum EMA is updated (issue #1107).
pub fn quorum_updated(env: &Env, new_quorum_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "quorum_updated"),), new_quorum_bps);
/// Emitted when a token holder delegates their vote to another address.
pub fn delegated(env: &Env, delegator: &Address, delegatee: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "delegated"),
            delegator.clone(),
            delegatee.clone(),
        ),
        (),
    );
}

/// Emitted when a token holder removes their delegation.
pub fn undelegated(env: &Env, delegator: &Address) {
    env.events()
        .publish((Symbol::new(env, "undelegated"), delegator.clone()), ());
}
