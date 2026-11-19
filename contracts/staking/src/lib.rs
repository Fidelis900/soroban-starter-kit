#![no_std]
#![deny(missing_docs)]
//! Staking rewards contract template.
//!
//! Users stake tokens to earn rewards that accrue over time from a reward pool
//! funded by the admin; rewards can be claimed independently of withdrawals.
//!
//! ## Continuous linear emission (#1135)
//!
//! Instead of discrete lump-sum injections, rewards can be emitted linearly
//! over time.  The admin sets a `reward_rate` (tokens per ledger) and a
//! `period_end` ledger via `set_reward_rate`.  The global reward-per-token
//! accumulator is advanced by `elapsed_ledgers * rate * REWARD_SCALE /
//! total_staked` on every state-changing call, so rewards accrue smoothly and
//! stakers cannot time deposits/withdrawals to capture lump sums.
//!
//! ## Lockup boosts (#1336)
//!
//! `stake_with_lockup` locks a staker's position for a number of ledgers in
//! exchange for a boost multiplier ([`LOCKUP_TIERS`]).  Rewards are shared in
//! proportion to *weight* (`stake * boost`), while `TotalStaked` keeps tracking
//! raw tokens.  `unstake` is rejected while the lock is active.

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use errors::StakingError;
pub use storage::{BOOST_DENOMINATOR, DataKey, LOCKUP_TIERS, REWARD_SCALE, UnbondRequest};

use soroban_common::math::mul_div;
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};

fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Returns the current global reward-per-token accumulator.
fn reward_per_token(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::RewardPerTokenStored)
        .unwrap_or(0i128)
}

/// Returns the configured emission rate (tokens per ledger).
fn reward_rate(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::RewardRate)
        .unwrap_or(0i128)
}

/// Returns the ledger at which the current emission period ends.
fn period_end(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PeriodEnd)
        .unwrap_or(0u32)
}

/// Returns the ledger at which the accumulator was last advanced.
fn last_update_ledger(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::LastUpdateLedger)
        .unwrap_or(0u32)
}

/// Projects the reward-per-token accumulator forward to the current ledger
/// **without mutating storage**.
///
/// This is the pure, in-memory counterpart to [`accrue`].  It applies the
/// exact same clamping (`[last_update, period_end]`) and fixed-point scaling
/// so that read-only queries such as [`pending_rewards`] observe the same
/// value that a subsequent state-changing call would persist, while leaving
/// storage untouched for Soroban RPC simulation (#1335).
fn projected_reward_per_token(env: &Env, total_staked: i128) -> Result<i128, StakingError> {
    let now = env.ledger().sequence();
    let last = last_update_ledger(env);
    let end = period_end(env);
    let rate = reward_rate(env);

    let effective_now = if now < end { now } else { end };
    let effective_last = if last < end { last } else { end };
    let elapsed = effective_now.saturating_sub(effective_last);

    let rpt = reward_per_token(env);
    if elapsed == 0 || rate <= 0 || total_staked <= 0 {
        return Ok(rpt);
    }

    // `elapsed * rate` may overflow; surface it as an error rather than
    // silently wrapping, then scale by `REWARD_SCALE / total_staked`.
    let numerator = (elapsed as i128)
        .checked_mul(rate)
        .ok_or(StakingError::Overflow)?;
    let delta = mul_div(numerator, REWARD_SCALE, total_staked).map_err(|_| StakingError::Overflow)?;
    rpt.checked_add(delta).ok_or(StakingError::Overflow)
}

/// Advances and persists the reward-per-token accumulator for the elapsed
/// ledgers, and records the emitted amount so reserve checks (#1337) know the
/// outstanding obligation.  `total_weight` is the boosted stake total.
fn accrue(env: &Env, total_weight: i128) -> Result<(), StakingError> {
    let now = env.ledger().sequence();
    let projected = projected_reward_per_token(env, total_weight)?;
    if projected != reward_per_token(env) {
        let end = period_end(env);
        let last = last_update_ledger(env);
        let elapsed = now.min(end).saturating_sub(last.min(end));
        let emitted = (elapsed as i128)
            .checked_mul(reward_rate(env))
            .ok_or(StakingError::Overflow)?;
        let total = get_i128(env, &DataKey::TotalEmitted)
            .checked_add(emitted)
            .ok_or(StakingError::Overflow)?;
        env.storage().instance().set(&DataKey::TotalEmitted, &total);
    }
    env.storage()
        .instance()
        .set(&DataKey::RewardPerTokenStored, &projected);
    env.storage()
        .instance()
        .set(&DataKey::LastUpdateLedger, &now);
    Ok(())
}

fn get_i128(env: &Env, key: &DataKey) -> i128 {
    env.storage().instance().get(key).unwrap_or(0i128)
}

fn get_p_i128(env: &Env, key: &DataKey) -> i128 {
    env.storage().persistent().get(key).unwrap_or(0i128)
}

fn get_admin(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)
}

fn get_stake_token(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::StakeToken)
        .ok_or(StakingError::NotInitialized)
}

fn get_reward_token(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::RewardToken)
        .ok_or(StakingError::NotInitialized)
}

fn get_total_staked_internal(env: &Env) -> Result<i128, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .ok_or(StakingError::NotInitialized)
}

/// Total boosted weight across all stakers (the reward-share denominator).
fn total_weight(env: &Env) -> i128 {
    get_i128(env, &DataKey::TotalWeight)
}

/// Pure reward calculation — `weight` is the staker's boosted stake.
pub(crate) fn calculate_earned(
    weight: i128,
    rpt: i128,
    paid: i128,
    accrued: i128,
) -> Option<i128> {
    let delta = rpt.checked_sub(paid)?;
    let earned = mul_div(weight, delta, REWARD_SCALE).ok()?;
    accrued.checked_add(earned)
}

fn earned_at_reward_per_token(env: &Env, staker: &Address, rpt: i128) -> i128 {
    let weight = get_p_i128(env, &DataKey::Weight(staker.clone()));
    let paid = get_p_i128(env, &DataKey::RewardPerTokenPaid(staker.clone()));
    let accrued = get_p_i128(env, &DataKey::Rewards(staker.clone()));
    calculate_earned(weight, rpt, paid, accrued).unwrap_or(0i128)
}

fn earned(env: &Env, staker: &Address) -> i128 {
    earned_at_reward_per_token(env, staker, reward_per_token(env))
}

/// Snapshots the staker's earned rewards and updates their paid-up-to pointer.
fn update_reward(env: &Env, staker: &Address) {
    let e = earned(env, staker);
    let rpt = reward_per_token(env);
    env.storage()
        .persistent()
        .set(&DataKey::Rewards(staker.clone()), &e);
    env.storage()
        .persistent()
        .set(&DataKey::RewardPerTokenPaid(staker.clone()), &rpt);
}

fn stake_of(env: &Env, staker: &Address) -> i128 {
    get_p_i128(env, &DataKey::Stake(staker.clone()))
}

fn locked_until(env: &Env, staker: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::LockedUntil(staker.clone()))
        .unwrap_or(0u32)
}

/// Active boost in basis points; resets to 1x once the lock has elapsed.
fn active_boost(env: &Env, staker: &Address) -> i128 {
    if env.ledger().sequence() >= locked_until(env, staker) {
        return BOOST_DENOMINATOR;
    }
    env.storage()
        .persistent()
        .get(&DataKey::Boost(staker.clone()))
        .unwrap_or(BOOST_DENOMINATOR)
}

/// Sets a staker's raw stake and re-derives weight, `TotalStaked` and
/// `TotalWeight`.  Callers must have run `accrue` + `update_reward` first.
fn set_stake(env: &Env, staker: &Address, new_stake: i128) -> Result<(), StakingError> {
    let old_stake = stake_of(env, staker);
    let old_weight = get_p_i128(env, &DataKey::Weight(staker.clone()));
    let new_weight = mul_div(new_stake, active_boost(env, staker), BOOST_DENOMINATOR)
        .map_err(|_| StakingError::Overflow)?;
    let total_staked = get_total_staked_internal(env)?
        .checked_add(new_stake - old_stake)
        .ok_or(StakingError::Overflow)?;
    let weight_total = total_weight(env)
        .checked_add(new_weight - old_weight)
        .ok_or(StakingError::Overflow)?;
    env.storage()
        .persistent()
        .set(&DataKey::Stake(staker.clone()), &new_stake);
    env.storage()
        .persistent()
        .set(&DataKey::Weight(staker.clone()), &new_weight);
    env.storage().instance().set(&DataKey::TotalStaked, &total_staked);
    env.storage().instance().set(&DataKey::TotalWeight, &weight_total);
    Ok(())
}

/// Shared deposit path for `stake`, `stake_with_lockup`, `stake_with_permit`.
fn do_stake(
    env: &Env,
    staker: &Address,
    amount: i128,
    lockup_ledgers: u32,
) -> Result<(), StakingError> {
    if amount <= 0 {
        return Err(StakingError::InvalidAmount);
    }
    let boost = if lockup_ledgers == 0 {
        0
    } else {
        LOCKUP_TIERS
            .iter()
            .filter(|(min, _)| lockup_ledgers >= *min)
            .map(|(_, b)| *b)
            .max()
            .ok_or(StakingError::InvalidLockup)?
    };
    get_total_staked_internal(env)?;
    accrue(env, total_weight(env))?;
    update_reward(env, staker);

    if boost > 0 {
        // Extend (never shorten) an existing lock and keep the best boost.
        let now = env.ledger().sequence();
        let unlock = now
            .checked_add(lockup_ledgers)
            .ok_or(StakingError::Overflow)?
            .max(locked_until(env, staker));
        let best = boost.max(active_boost(env, staker));
        env.storage()
            .persistent()
            .set(&DataKey::LockedUntil(staker.clone()), &unlock);
        env.storage()
            .persistent()
            .set(&DataKey::Boost(staker.clone()), &best);
    }

    let stake_token = get_stake_token(env)?;
    token::Client::new(env, &stake_token).transfer(
        staker,
        &env.current_contract_address(),
        &amount,
    );
    let new_stake = stake_of(env, staker)
        .checked_add(amount)
        .ok_or(StakingError::Overflow)?;
    set_stake(env, staker, new_stake)?;
    bump(env);
    Ok(())
}

/// Staking contract with continuous emission, lockup boosts, permit staking
/// and cancellable unbonding.
pub use contract::*;

mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct StakingContract;

    #[contractimpl]
    impl StakingContract {
        /// Initialize the staking contract.
        ///
        /// # Errors
        /// - [`StakingError::AlreadyInitialized`] if called more than once.
        pub fn initialize(
            env: Env,
            admin: Address,
            stake_token: Address,
            reward_token: Address,
            unbonding_period: u32,
            slash_destination: Address,
        ) -> Result<(), StakingError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(StakingError::AlreadyInitialized);
            }
            admin.require_auth();
            let s = env.storage().instance();
            s.set(&DataKey::Admin, &admin);
            s.set(&DataKey::StakeToken, &stake_token);
            s.set(&DataKey::RewardToken, &reward_token);
            s.set(&DataKey::TotalStaked, &0i128);
            s.set(&DataKey::TotalWeight, &0i128);
            s.set(&DataKey::TotalRewards, &0i128);
            s.set(&DataKey::TotalEmitted, &0i128);
            s.set(&DataKey::TotalClaimed, &0i128);
            s.set(&DataKey::UnbondingPeriod, &unbonding_period);
            s.set(&DataKey::SlashDestination, &slash_destination);
            s.set(&DataKey::RewardPerTokenStored, &0i128);
            s.set(&DataKey::UndistributedRewards, &0i128);
            s.set(&DataKey::RewardRate, &0i128);
            s.set(&DataKey::PeriodEnd, &0u32);
            s.set(&DataKey::LastUpdateLedger, &env.ledger().sequence());
            bump(&env);
            Ok(())
        }

        /// Configure continuous linear reward emission (#1135).
        ///
        /// Before activating, verifies the contract's reward-token balance
        /// covers `unclaimed + (period_end - now) * reward_rate` (#1337).
        /// If the reward token is also the stake token, staked principal is
        /// excluded from the available balance.
        ///
        /// # Errors
        /// - [`StakingError::InvalidRewardRate`] / [`StakingError::InvalidPeriodEnd`]
        /// - [`StakingError::InsufficientRewardReserves`] if underfunded.
        pub fn set_reward_rate(
            env: Env,
            reward_rate: i128,
            period_end: u32,
        ) -> Result<(), StakingError> {
            let admin = get_admin(&env)?;
            admin.require_auth();
            if reward_rate < 0 {
                return Err(StakingError::InvalidRewardRate);
            }
            let now = env.ledger().sequence();
            if period_end <= now {
                return Err(StakingError::InvalidPeriodEnd);
            }
            get_total_staked_internal(&env)?;
            accrue(&env, total_weight(&env))?;

            let reward_token = get_reward_token(&env)?;
            let mut balance = token::Client::new(&env, &reward_token)
                .balance(&env.current_contract_address());
            if reward_token == get_stake_token(&env)? {
                balance -= get_total_staked_internal(&env)?;
            }
            let unclaimed =
                get_i128(&env, &DataKey::TotalEmitted) - get_i128(&env, &DataKey::TotalClaimed);
            let future = ((period_end - now) as i128)
                .checked_mul(reward_rate)
                .ok_or(StakingError::Overflow)?;
            let required = unclaimed.checked_add(future).ok_or(StakingError::Overflow)?;
            if balance < required {
                return Err(StakingError::InsufficientRewardReserves);
            }

            env.storage().instance().set(&DataKey::RewardRate, &reward_rate);
            env.storage().instance().set(&DataKey::PeriodEnd, &period_end);
            bump(&env);
            Ok(())
        }

        /// Read-only pending rewards for `staker` including linear accrual.
        pub fn pending_rewards(env: Env, staker: Address) -> i128 {
            let projected = projected_reward_per_token(&env, total_weight(&env))
                .unwrap_or_else(|_| reward_per_token(&env));
            earned_at_reward_per_token(&env, &staker, projected)
        }

        /// Stake `amount` tokens with no lockup.
        pub fn stake(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            staker.require_auth();
            do_stake(&env, &staker, amount, 0)
        }

        /// Stake `amount` and lock the position for `lockup_ledgers` in
        /// exchange for a boost multiplier (#1336).
        ///
        /// # Errors
        /// - [`StakingError::InvalidLockup`] if below the smallest tier.
        pub fn stake_with_lockup(
            env: Env,
            staker: Address,
            amount: i128,
            lockup_ledgers: u32,
        ) -> Result<(), StakingError> {
            staker.require_auth();
            if lockup_ledgers == 0 {
                return Err(StakingError::InvalidLockup);
            }
            do_stake(&env, &staker, amount, lockup_ledgers)
        }

        /// Single-transaction stake (#1338).  On Soroban the staker's
        /// authorization entry for this invocation is the permit: it signs over
        /// `(amount, deadline)`, so no separate `approve` transaction is
        /// needed.  Fails with [`StakingError::PermitExpired`] after `deadline`.
        pub fn stake_with_permit(
            env: Env,
            staker: Address,
            amount: i128,
            deadline: u32,
        ) -> Result<(), StakingError> {
            if env.ledger().sequence() > deadline {
                return Err(StakingError::PermitExpired);
            }
            staker.require_auth();
            do_stake(&env, &staker, amount, 0)
        }

        /// Claim accrued rewards.
        pub fn claim_rewards(env: Env, staker: Address) -> Result<i128, StakingError> {
            staker.require_auth();
            get_total_staked_internal(&env)?;
            accrue(&env, total_weight(&env))?;
            update_reward(&env, &staker);
            let reward = get_p_i128(&env, &DataKey::Rewards(staker.clone()));
            if reward > 0 {
                env.storage()
                    .persistent()
                    .set(&DataKey::Rewards(staker.clone()), &0i128);
                let claimed = get_i128(&env, &DataKey::TotalClaimed) + reward;
                env.storage().instance().set(&DataKey::TotalClaimed, &claimed);
                let reward_token = get_reward_token(&env)?;
                token::Client::new(&env, &reward_token).transfer(
                    &env.current_contract_address(),
                    &staker,
                    &reward,
                );
            }
            bump(&env);
            Ok(reward)
        }

        /// Queue an unbonding withdrawal.  Rejected while a lockup is active.
        pub fn unstake(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            staker.require_auth();
            get_total_staked_internal(&env)?;
            if env.ledger().sequence() < locked_until(&env, &staker) {
                return Err(StakingError::StillLocked);
            }
            accrue(&env, total_weight(&env))?;
            update_reward(&env, &staker);
            let prev = stake_of(&env, &staker);
            if amount <= 0 || amount > prev {
                return Err(StakingError::InsufficientStake);
            }
            set_stake(&env, &staker, prev - amount)?;
            let unbonding_period: u32 = env
                .storage()
                .instance()
                .get(&DataKey::UnbondingPeriod)
                .unwrap_or(0u32);
            if unbonding_period == 0 {
                let stake_token = get_stake_token(&env)?;
                token::Client::new(&env, &stake_token).transfer(
                    &env.current_contract_address(),
                    &staker,
                    &amount,
                );
            } else {
                let key = DataKey::UnbondRequest(staker.clone());
                let existing: i128 = env
                    .storage()
                    .persistent()
                    .get::<DataKey, UnbondRequest>(&key)
                    .map(|r| r.amount)
                    .unwrap_or(0);
                env.storage().persistent().set(
                    &key,
                    &UnbondRequest {
                        amount: existing + amount,
                        available_at: env.ledger().sequence() + unbonding_period,
                    },
                );
            }
            bump(&env);
            Ok(())
        }

        /// Cancel a pending unbonding request, returning the tokens directly
        /// to the active stake with no delay (#1339).  Rewards resume accruing
        /// on the restored amount immediately.
        ///
        /// # Errors
        /// - [`StakingError::NoUnbondRequest`] if nothing is queued.
        pub fn cancel_unbonding(env: Env, staker: Address) -> Result<(), StakingError> {
            staker.require_auth();
            let key = DataKey::UnbondRequest(staker.clone());
            let req: UnbondRequest = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(StakingError::NoUnbondRequest)?;
            accrue(&env, total_weight(&env))?;
            update_reward(&env, &staker);
            let new_stake = stake_of(&env, &staker)
                .checked_add(req.amount)
                .ok_or(StakingError::Overflow)?;
            set_stake(&env, &staker, new_stake)?;
            env.storage().persistent().remove(&key);
            bump(&env);
            Ok(())
        }

        /// Withdraw tokens after the unbonding period has elapsed.
        pub fn withdraw(env: Env, staker: Address) -> Result<i128, StakingError> {
            staker.require_auth();
            let req: UnbondRequest = env
                .storage()
                .persistent()
                .get(&DataKey::UnbondRequest(staker.clone()))
                .ok_or(StakingError::NoUnbondRequest)?;
            if env.ledger().sequence() < req.available_at {
                return Err(StakingError::UnbondingNotComplete);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::UnbondRequest(staker.clone()));
            let stake_token = get_stake_token(&env)?;
            token::Client::new(&env, &stake_token).transfer(
                &env.current_contract_address(),
                &staker,
                &req.amount,
            );
            bump(&env);
            Ok(req.amount)
        }

        /// Admin-only: slash up to `amount`, taking from active stake first and
        /// then from any pending unbond request.
        pub fn slash(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            let admin = get_admin(&env)?;
            admin.require_auth();
            get_total_staked_internal(&env)?;
            accrue(&env, total_weight(&env))?;
            update_reward(&env, &staker);

            let current = stake_of(&env, &staker);
            let from_stake = amount.min(current).max(0);
            let mut remaining = amount.max(0) - from_stake;
            if from_stake > 0 {
                set_stake(&env, &staker, current - from_stake)?;
            }
            let mut from_unbond = 0i128;
            if remaining > 0 {
                let key = DataKey::UnbondRequest(staker.clone());
                if let Some(mut req) = env.storage().persistent().get::<DataKey, UnbondRequest>(&key) {
                    from_unbond = remaining.min(req.amount);
                    req.amount -= from_unbond;
                    remaining -= from_unbond;
                    env.storage().persistent().set(&key, &req);
                }
            }
            let _ = remaining;
            let slashed = from_stake + from_unbond;
            if slashed > 0 {
                let dest: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::SlashDestination)
                    .ok_or(StakingError::NotInitialized)?;
                let stake_token = get_stake_token(&env)?;
                token::Client::new(&env, &stake_token).transfer(
                    &env.current_contract_address(),
                    &dest,
                    &slashed,
                );
            }
            bump(&env);
            Ok(())
        }

        /// Returns the total amount currently staked (raw tokens).
        pub fn total_staked(env: Env) -> i128 {
            get_total_staked_internal(&env).unwrap_or(0i128)
        }

        /// Returns the staker's raw active stake.
        pub fn stake_of(env: Env, staker: Address) -> i128 {
            stake_of(&env, &staker)
        }

        /// Returns the staker's boosted weight.
        pub fn weight_of(env: Env, staker: Address) -> i128 {
            get_p_i128(&env, &DataKey::Weight(staker))
        }

        /// Returns the ledger until which the staker is locked (0 = none).
        pub fn locked_until(env: Env, staker: Address) -> u32 {
            locked_until(&env, &staker)
        }

        /// Returns the pending unbond request, if any.
        pub fn unbond_request(env: Env, staker: Address) -> Option<UnbondRequest> {
            env.storage().persistent().get(&DataKey::UnbondRequest(staker))
        }

        /// Returns the current global reward-per-token accumulator.
        pub fn reward_per_token_stored(env: Env) -> i128 {
            reward_per_token(&env)
        }
    }
}
