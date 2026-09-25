use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address, stake_token: &Address, reward_token: &Address) {
    env.events().publish(
        (Symbol::new(env, "initialized"), admin.clone()),
        (stake_token.clone(), reward_token.clone()),
    );
}

pub fn staked(env: &Env, staker: &Address, amount: i128, new_total: i128) {
    env.events().publish(
        (Symbol::new(env, "staked"), staker.clone()),
        (amount, new_total),
    );
}

pub fn unstaked(env: &Env, staker: &Address, amount: i128, remaining: i128) {
    env.events().publish(
        (Symbol::new(env, "unstaked"), staker.clone()),
        (amount, remaining),
    );
}

pub fn rewards_claimed(env: &Env, staker: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "claimed_rewards"), staker.clone()),
        amount,
    );
}

pub fn rewards_added(env: &Env, admin: &Address, amount: i128, new_total: i128) {
    env.events().publish(
        (Symbol::new(env, "added_rewards"), admin.clone()),
        (amount, new_total),
    );
}

/// Emitted when a staker auto-compounds claimed rewards back into their stake.
///
/// `reward` is the amount of rewards restaked; `new_stake` is the staker's
/// resulting stake balance after compounding.
pub fn compounded(env: &Env, staker: &Address, reward: i128, new_stake: i128) {
    env.events().publish(
        (Symbol::new(env, "compounded"), staker.clone()),
        (reward, new_stake),
    );
}

/// Emitted when admin slashes a staker's balance.
///
/// `amount` is the slashed token amount; `destination` is where the tokens went.
pub fn slashed(env: &Env, admin: &Address, staker: &Address, amount: i128, destination: &Address) {
    env.events().publish(
        (Symbol::new(env, "slashed"), admin.clone(), staker.clone()),
        (amount, destination.clone()),
    );
}

/// Emitted when a staker queues an unbond request.
///
/// `amount` is the amount queued; `available_at` is the ledger sequence after which
/// `withdraw` becomes valid.
pub fn unbond_requested(env: &Env, staker: &Address, amount: i128, available_at: u32) {
    env.events().publish(
        (Symbol::new(env, "unbond_requested"), staker.clone()),
        (amount, available_at),
    );
}

/// Emitted when a staker successfully withdraws their unbonded tokens.
pub fn withdrawn(env: &Env, staker: &Address, amount: i128) {
    env.events()
        .publish((Symbol::new(env, "withdrawn"), staker.clone()), amount);
}
