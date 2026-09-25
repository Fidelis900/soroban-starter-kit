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

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;

pub use errors::StakingError;
pub use storage::{DataKey, REWARD_SCALE, UnbondRequest};

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

/// Advances the reward-per-token accumulator for the elapsed ledgers.
///
/// `ΔRPT = Δledgers * rate * REWARD_SCALE / total_staked`.  Only the ledgers
/// within the active emission window `[last_update, period_end]` count, so
/// emission stops exactly at `period_end` and never over-distributes.
fn accrue(env: &Env, total_staked: i128) {
    let now = env.ledger().sequence();
    let last = last_update_ledger(env);
    let end = period_end(env);
    let rate = reward_rate(env);

    let effective_now = if now < end { now } else { end };
    let effective_last = if last < end { last } else { end };
    let elapsed = effective_now.saturating_sub(effective_last);

    if elapsed > 0 && rate > 0 && total_staked > 0 {
        let rpt = reward_per_token(env);
        let delta = (elapsed as i128) * rate * REWARD_SCALE / total_staked;
        env.storage()
            .instance()
            .set(&DataKey::RewardPerTokenStored, &(rpt + delta));
    }
    env.storage()
        .instance()
        .set(&DataKey::LastUpdateLedger, &now);
}

/// Helper to get admin address or return NotInitialized error.
fn get_admin(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(StakingError::NotInitialized)
}

/// Helper to get stake token address or return NotInitialized error.
fn get_stake_token(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::StakeToken)
        .ok_or(StakingError::NotInitialized)
}

/// Helper to get reward token address or return NotInitialized error.
fn get_reward_token(env: &Env) -> Result<Address, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::RewardToken)
        .ok_or(StakingError::NotInitialized)
}

/// Helper to get total staked or return NotInitialized error.
fn get_total_staked_internal(env: &Env) -> Result<i128, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::TotalStaked)
        .ok_or(StakingError::NotInitialized)
}

/// Helper to get total rewards or return NotInitialized error.
fn get_total_rewards_internal(env: &Env) -> Result<i128, StakingError> {
    env.storage()
        .instance()
        .get(&DataKey::TotalRewards)
        .ok_or(StakingError::NotInitialized)
}

/// Pure reward calculation — isolated from storage for testability.
#[allow(clippy::arithmetic_side_effects)] // overflow checked via REWARD_SCALE invariant
pub(crate) fn calculate_earned(stake: i128, rpt: i128, paid: i128, accrued: i128) -> i128 {
    accrued + stake * (rpt - paid) / REWARD_SCALE
}

/// Computes how many reward tokens `staker` has earned since their last update.
fn earned(env: &Env, staker: &Address) -> i128 {
    let stake: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::Stake(staker.clone()))
        .unwrap_or(0i128);
    let rpt = reward_per_token(env);
    let paid: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::RewardPerTokenPaid(staker.clone()))
        .unwrap_or(0i128);
    let accrued: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::Rewards(staker.clone()))
        .unwrap_or(0i128);
    calculate_earned(stake, rpt, paid, accrued)
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

/// Simple proportional token staking contract.
///
/// Flow:
/// 1. Admin calls `initialize` — sets the stake and reward token addresses,
///    unbonding period, and slash destination.
/// 2. Admin calls `set_reward_rate` to configure continuous linear emission.
/// 3. Users call `stake` to deposit stake tokens.
/// 4. Users call `claim_rewards` to collect accrued rewards.
/// 5. Users call `unstake` to queue a withdrawal (starts unbonding timer).
/// 6. After the unbonding period, users call `withdraw` to receive tokens.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct StakingContract;

    #[contractimpl]
    impl StakingContract {
        /// Initialize the staking contract.
        ///
        /// - `unbonding_period` — ledgers between `unstake` and `withdraw`.
        ///   Pass `0` for immediate withdrawals (legacy behaviour).
        /// - `slash_destination` — address that receives slashed tokens.
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

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage()
                .instance()
                .set(&DataKey::StakeToken, &stake_token);
            env.storage()
                .instance()
                .set(&DataKey::RewardToken, &reward_token);
            env.storage().instance().set(&DataKey::TotalStaked, &0i128);
            env.storage().instance().set(&DataKey::TotalRewards, &0i128);
            env.storage()
                .instance()
                .set(&DataKey::UnbondingPeriod, &unbonding_period);
            env.storage()
                .instance()
                .set(&DataKey::SlashDestination, &slash_destination);
            env.storage()
                .instance()
                .set(&DataKey::RewardPerTokenStored, &0i128);
            env.storage()
                .instance()
                .set(&DataKey::UndistributedRewards, &0i128);
            env.storage().instance().set(&DataKey::RewardRate, &0i128);
            env.storage().instance().set(&DataKey::PeriodEnd, &0u32);
            env.storage()
                .instance()
                .set(&DataKey::LastUpdateLedger, &env.ledger().sequence());
            env.storage()
                .instance()
                .set(&DataKey::SlashDestination, &slash_destination);
            bump(&env);
            Ok(())
        }

        /// Configure continuous linear reward emission (#1135).
        ///
        /// Sets `reward_rate` (tokens per ledger) and the ledger at which
        /// emission stops (`period_end`).  The accumulator is advanced for the
        /// elapsed ledgers under the previous schedule before the new rate
        /// takes effect, so no rewards are lost or double-counted.
        ///
        /// # Errors
        /// - [`StakingError::NotInitialized`] if the contract is not set up.
        pub fn set_reward_rate(
            env: Env,
            reward_rate: i128,
            period_end: u32,
        ) -> Result<(), StakingError> {
            let admin = get_admin(&env)?;
            admin.require_auth();
            let total_staked = get_total_staked_internal(&env)?;
            accrue(&env, total_staked);
            env.storage()
                .instance()
                .set(&DataKey::RewardRate, &reward_rate);
            env.storage()
                .instance()
                .set(&DataKey::PeriodEnd, &period_end);
            bump(&env);
            Ok(())
        }

        /// Returns the pending rewards for `staker` including linear accrual.
        pub fn pending_rewards(env: Env, staker: Address) -> i128 {
            let total_staked = get_total_staked_internal(&env).unwrap_or(0i128);
            accrue(&env, total_staked);
            earned(&env, &staker)
        }

        /// Stake `amount` tokens, accruing rewards first so the staker's
        /// entry point cannot capture or miss a lump sum.
        pub fn stake(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            staker.require_auth();
            let total_staked = get_total_staked_internal(&env)?;
            accrue(&env, total_staked);
            update_reward(&env, &staker);
            let stake_token = get_stake_token(&env)?;
            token::Client::new(&env, &stake_token).transfer(
                &staker,
                &env.current_contract_address(),
                &amount,
            );
            let prev: i128 = env
        /// Slash up to `amount` from `staker`'s balance, routing the slashed
        /// tokens to the configured `slash_destination`.
        ///
        /// The slash is applied first against the staker's active stake and,
        /// if that is insufficient, against any pending unbond request.  This
        /// closes the unbonding bypass where a staker could move funds into an
        /// unbond request to escape slashing.
        ///
        /// # Errors
        /// - [`StakingError::NotInitialized`] if the contract is not set up.
        pub fn slash(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            let admin = get_admin(&env)?;
            admin.require_auth();
            let stake_token = get_stake_token(&env)?;

            let current: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Stake(staker.clone()))
                .unwrap_or(0i128);
            env.storage()
                .persistent()
                .set(&DataKey::Stake(staker.clone()), &(prev + amount));
            env.storage()
                .instance()
                .set(&DataKey::TotalStaked, &(total_staked + amount));
            bump(&env);
            Ok(())
        }

        /// Claim accrued rewards, advancing the accumulator first.
        pub fn claim_rewards(env: Env, staker: Address) -> Result<i128, StakingError> {
            staker.require_auth();
            let total_staked = get_total_staked_internal(&env)?;
            accrue(&env, total_staked);
            update_reward(&env, &staker);
            let reward: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Rewards(staker.clone()))
                .unwrap_or(0i128);
            if reward > 0 {
                let reward_token = get_reward_token(&env)?;
                token::Client::new(&env, &reward_token).transfer(
                    &env.current_contract_address(),
                    &staker,
                    &reward,
                );
                env.storage()
                    .persistent()
                    .set(&DataKey::Rewards(staker.clone()), &0i128);
            let mut remaining = if amount > current { current } else { amount };

            // Reduce active stake first.
            let stake_slashed = if remaining > current { current } else { remaining };
            if stake_slashed > 0 {
                env.storage()
                    .persistent()
                    .set(&DataKey::Stake(staker.clone()), &(current - stake_slashed));
                remaining -= stake_slashed;
            }
            bump(&env);
            Ok(reward)
        }

        /// Queue an unbonding withdrawal, accruing rewards first.
        pub fn unstake(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            staker.require_auth();
            let total_staked = get_total_staked_internal(&env)?;
            accrue(&env, total_staked);
            update_reward(&env, &staker);
            let prev: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Stake(staker.clone()))
                .unwrap_or(0i128);
            if amount > prev {
                return Err(StakingError::InsufficientStake);
            }
            env.storage()
                .persistent()
                .set(&DataKey::Stake(staker.clone()), &(prev - amount));
            env.storage()
                .instance()
                .set(&DataKey::TotalStaked, &(total_staked - amount));
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
                let ready_at = env.ledger().sequence() + unbonding_period;
                env.storage().persistent().set(
                    &DataKey::UnbondRequest(staker.clone()),
                    &UnbondRequest { amount, ready_at },
                );
            }
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
            if env.ledger().sequence() < req.ready_at {
                return Err(StakingError::UnbondingNotComplete);
            }
            let stake_token = get_stake_token(&env)?;
            token::Client::new(&env, &stake_token).transfer(
                &env.current_contract_address(),
                &staker,
                &req.amount,
            );
            env.storage()
                .persistent()
                .remove(&DataKey::UnbondRequest(staker.clone()));
            bump(&env);
            Ok(req.amount)
        }

        /// Admin-only: slash up to the staker's full balance.
        pub fn slash(env: Env, staker: Address, amount: i128) -> Result<(), StakingError> {
            let admin = get_admin(&env)?;
            admin.require_auth();
            let total_staked = get_total_staked_internal(&env)?;
            accrue(&env, total_staked);
            update_reward(&env, &staker);
            let prev: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Stake(staker.clone()))
                .unwrap_or(0i128);
            if amount > prev {
                return Err(StakingError::InsufficientStake);
            }
            env.storage()
                .persistent()
                .set(&DataKey::Stake(staker.clone()), &(prev - amount));
            env.storage()
                .instance()
                .set(&DataKey::TotalStaked, &(total_staked - amount));
            let dest: Address = env
                .storage()
                .instance()
                .get(&DataKey::SlashDestination)
                .ok_or(StakingError::NotInitialized)?;
            let stake_token = get_stake_token(&env)?;
            token::Client::new(&env, &stake_token).transfer(
                &env.current_contract_address(),
                &dest,
                &amount,
            );
            bump(&env);
            Ok(())
        }

        /// Returns the total amount currently staked.
        pub fn total_staked(env: Env) -> i128 {
            get_total_staked_internal(&env).unwrap_or(0i128)
        }

        /// Returns the current global reward-per-token accumulator.
        pub fn reward_per_token_stored(env: Env) -> i128 {
            reward_per_token(&env)
            // If the stake was insufficient, slash any pending unbond request.
            if remaining > 0 {
                if let Some(mut request) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, UnbondRequest>(&DataKey::UnbondRequest(staker.clone()))
                {
                    let unbond_slashed = if remaining > request.amount {
                        request.amount
                    } else {
                        remaining
                    };
                    if unbond_slashed > 0 {
                        request.amount -= unbond_slashed;
                        env.storage()
                            .persistent()
                            .set(&DataKey::UnbondRequest(staker.clone()), &request);
                        remaining -= unbond_slashed;
                    }
                }
            }

            let slashed = if amount > current { current } else { amount } - remaining;
            if slashed > 0 {
                let destination: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::SlashDestination)
                    .ok_or(StakingError::NotInitialized)?;
                token::Client::new(&env, &stake_token).transfer(
                    &env.current_contract_address(),
                    &destination,
                    &slashed,
                );
            }

            bump(&env);
            Ok(())
        }
    }
}
