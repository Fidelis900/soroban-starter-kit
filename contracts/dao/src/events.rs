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

/// Emitted when a voter allocates weight to a specific choice of a
/// multi-choice proposal (issue #1317).
pub fn proposal_choice_voted(
    env: &Env,
    voter: &Address,
    proposal_id: u32,
    choice: Symbol,
    weight: i128,
) {
    env.events().publish(
        (Symbol::new(env, "choice_voted"), voter.clone()),
        (proposal_id, choice, weight),
    );
}

/// Emitted when a proposal's execution tier is recorded (issue #1318).
pub fn proposal_tier_set(env: &Env, proposal_id: u32, tier: u32) {
    env.events()
        .publish((Symbol::new(env, "tier_set"),), (proposal_id, tier));
}

/// Emitted when a proposal is rejected because an active proposal with the
/// identical payload hash already exists (issue #1319).
pub fn duplicate_proposal_rejected(env: &Env, proposer: &Address, hash: soroban_sdk::BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "dup_rejected"), proposer.clone()),
        hash,
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

/// Emitted when the emergency security council fast-tracks a proposal (issue #1320).
pub fn emergency_proposal_executed(env: &Env, council: &Address, proposal_id: u32) {
    env.events().publish(
        (Symbol::new(env, "emergency_executed"), council.clone()),
        proposal_id,
    );
}

/// Emitted when the original proposer cancels their proposal before voting opens (issue #1321).
pub fn proposal_cancelled_by_proposer(env: &Env, proposer: &Address, proposal_id: u32) {
    env.events().publish(
        (Symbol::new(env, "proposer_cancelled"), proposer.clone()),
        proposal_id,
    );
}

/// Emitted when the admin updates the voting delay (issue #1441).
pub fn voting_delay_set(env: &Env, admin: &Address, voting_delay: u32) {
    env.events().publish(
        (Symbol::new(env, "voting_delay_set"), admin.clone()),
        voting_delay,
    );
}

/// Emitted when the admin updates the dynamic quorum configuration (issue #1440).
/// Data: `(quorum_bps, number_of_excluded_holders)`.
pub fn dynamic_quorum_set(env: &Env, admin: &Address, quorum_bps: u32, excluded: u32) {
    env.events().publish(
        (Symbol::new(env, "dynamic_quorum_set"), admin.clone()),
        (quorum_bps, excluded),
    );
}

/// Emitted when a proposal's dynamic quorum threshold is snapshotted at
/// creation (issue #1440).
/// Data: `(circulating_supply, required_votes)`.
pub fn dynamic_quorum_snapshot(
    env: &Env,
    proposal_id: u32,
    circulating_supply: i128,
    required_votes: i128,
) {
    env.events().publish(
        (Symbol::new(env, "dynamic_quorum_snapshot"), proposal_id),
        (circulating_supply, required_votes),
    );
}

/// Emitted when a voter's token balance is snapshotted at proposal creation
/// ledger, so vote weight is evaluated against the historical balance
/// (issue #1442).
pub fn vote_snapshot_recorded(env: &Env, proposal_id: u32, voter: &Address, balance: i128) {
    env.events().publish(
        (Symbol::new(env, "vote_snapshot"), voter.clone()),
        (proposal_id, balance),
    );
}

/// Emitted when a passed proposal enters the execution timelock (issue #1443).
/// Data: `executable_at` is the ledger at which execution becomes allowed.
pub fn timelock_started(env: &Env, proposal_id: u32, executable_at: u32) {
    env.events().publish(
        (Symbol::new(env, "timelock_started"),),
        (proposal_id, executable_at),
    );
}

/// Emitted when the guardian multi-sig vetoes a pending proposal during the
/// timelock window (issue #1444).
pub fn guardian_vetoed(env: &Env, guardian: &Address, proposal_id: u32) {
    env.events().publish(
        (Symbol::new(env, "guardian_vetoed"), guardian.clone()),
        proposal_id,
    );
}
