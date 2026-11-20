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

/// Emitted when the dynamic reward rate is recalculated.
///
/// `total_staked` is the staked TVL used in the calculation; `reward_rate` is the
/// resulting per-ledger emission rate (or APY-scaled rate) applied going forward.
pub fn reward_rate_updated(env: &Env, total_staked: i128, reward_rate: i128) {
    env.events().publish(
        (Symbol::new(env, "reward_rate_updated"),),
        (total_staked, reward_rate),
    );
}

/// Emitted when slashed tokens are credited to the insurance pool.
///
/// `amount` is the amount credited; `new_pool_total` is the insurance pool balance
/// after the credit.
pub fn insurance_pool_credited(env: &Env, amount: i128, new_pool_total: i128) {
    env.events().publish(
        (Symbol::new(env, "insurance_pool_credited"),),
        (amount, new_pool_total),
    );
}
