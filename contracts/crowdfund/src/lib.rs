#![no_std]
#![deny(missing_docs)]
//! Crowdfunding campaign contract template.
//!
//! Contributors pledge tokens toward a funding goal before a deadline. If the
//! goal is met the creator claims the funds; otherwise contributors refund
//! their pledges.

use soroban_sdk::{Address, Env, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::CrowdfundError;
pub use storage::{CrowdfundInfo, DataKey, FundingTier, TierStatus};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn get_instance<T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Result<T, CrowdfundError> {
    env.storage()
        .instance()
        .get(key)
        .ok_or(CrowdfundError::NotInitialized)
}

/// All-or-nothing crowdfunding contract.
///
/// Lifecycle:
/// - Creator calls `initialize` to set a token, funding goal, and deadline ledger.
/// - Contributors call `pledge` to deposit tokens before the deadline.
/// - If the goal is met, the creator calls `claim` to collect all funds after the deadline.
/// - If the goal is not met after the deadline, each contributor calls `refund` to recover their pledge.
/// - A contributor can call `withdraw` to pull back their pledge before the deadline.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct CrowdfundContract;

    #[contractimpl]
    impl CrowdfundContract {
        /// Initialize the campaign. Can only be called once.
        ///
        /// `tiers` are optional (stretch-goal) reward thresholds beyond `goal`; pass an
        /// empty vec if none are needed. `max_pledge_per_address`, if set, caps how much
        /// a single address may pledge in total across multiple calls to `pledge`.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::AlreadyInitialized`] if already set up.
        /// - [`CrowdfundError::InvalidGoal`] if `goal` <= 0.
        /// - [`CrowdfundError::InvalidDeadline`] if `deadline` <= current ledger.
        /// - [`CrowdfundError::InvalidTier`] if any tier threshold <= 0.
        /// - [`CrowdfundError::InvalidAmount`] if `max_pledge_per_address` is `Some` and <= 0.
        pub fn initialize(
            env: Env,
            creator: Address,
            token: Address,
            goal: i128,
            deadline: u32,
            tiers: Vec<FundingTier>,
            max_pledge_per_address: Option<i128>,
        ) -> Result<(), CrowdfundError> {
            if env.storage().instance().has(&DataKey::Creator) {
                return Err(CrowdfundError::AlreadyInitialized);
            }
            if goal <= 0 {
                return Err(CrowdfundError::InvalidGoal);
            }
            if deadline <= env.ledger().sequence() {
                return Err(CrowdfundError::InvalidDeadline);
            }
            for tier in tiers.iter() {
                if tier.threshold <= 0 {
                    return Err(CrowdfundError::InvalidTier);
                }
            }
            if let Some(cap) = max_pledge_per_address {
                if cap <= 0 {
                    return Err(CrowdfundError::InvalidAmount);
                }
            }

            creator.require_auth();

            env.storage().instance().set(&DataKey::Creator, &creator);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage().instance().set(&DataKey::Goal, &goal);
            env.storage().instance().set(&DataKey::Deadline, &deadline);
            env.storage()
                .instance()
                .set(&DataKey::TotalPledged, &0_i128);
            env.storage().instance().set(&DataKey::Claimed, &false);
            env.storage().instance().set(&DataKey::Tiers, &tiers);
            env.storage()
                .instance()
                .set(&DataKey::DeadlineExtended, &false);
            if let Some(cap) = max_pledge_per_address {
                env.storage()
                    .instance()
                    .set(&DataKey::MaxPledgePerAddress, &cap);
            }

            bump_instance(&env);
            events::initialized(&env, &creator, goal, deadline);
            Ok(())
        }

        /// Pledge `amount` tokens to the campaign. Must be called before the deadline.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlinePassed`] if the deadline has passed.
        /// - [`CrowdfundError::InvalidAmount`] if `amount` <= 0.
        /// - [`CrowdfundError::PledgeCapExceeded`] if this pledge would push the
        ///   pledger's cumulative total above `max_pledge_per_address`.
        pub fn pledge(env: Env, pledger: Address, amount: i128) -> Result<(), CrowdfundError> {
            if amount <= 0 {
                return Err(CrowdfundError::InvalidAmount);
            }

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            pledger.require_auth();

            let existing: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pledge(pledger.clone()))
                .unwrap_or(0);
            let new_pledge = existing + amount;

            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, i128>(&DataKey::MaxPledgePerAddress)
            {
                if new_pledge > cap {
                    return Err(CrowdfundError::PledgeCapExceeded);
                }
            }

            let token: Address = get_instance(&env, &DataKey::Token)?;

            // Persist local accounting before the external token transfer. A failed transfer
            // aborts the transaction and rolls this effect back atomically.
            env.storage()
                .persistent()
                .set(&DataKey::Pledge(pledger.clone()), &new_pledge);
            env.storage().persistent().extend_ttl(
                &DataKey::Pledge(pledger.clone()),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let new_total = total + amount;
            env.storage()
                .instance()
                .set(&DataKey::TotalPledged, &new_total);

            token::Client::new(&env, &token).transfer(
                &pledger,
                &env.current_contract_address(),
                &amount,
            );

            bump_instance(&env);
            events::pledged(&env, &pledger, amount, new_total);
            Ok(())
        }

        /// Withdraw the caller's pledge before the deadline. Goal must not have been reached.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlinePassed`] if the deadline has already passed.
        /// - [`CrowdfundError::NothingToWithdraw`] if the caller has no active pledge.
        pub fn withdraw(env: Env, pledger: Address) -> Result<(), CrowdfundError> {
            get_instance::<Address>(&env, &DataKey::Creator)?; // ensure initialized

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            pledger.require_auth();

            let pledge: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pledge(pledger.clone()))
                .unwrap_or(0);
            if pledge <= 0 {
                return Err(CrowdfundError::NothingToWithdraw);
            }

            env.storage()
                .persistent()
                .remove(&DataKey::Pledge(pledger.clone()));

            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            env.storage()
                .instance()
                .set(&DataKey::TotalPledged, &(total - pledge));

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &pledger,
                &pledge,
            );

            bump_instance(&env);
            events::withdrawn(&env, &pledger, pledge);
            Ok(())
        }

        /// Extend the campaign deadline once. Admin (creator) only, and only callable
        /// before the original deadline has passed.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlinePassed`] if the current deadline has already passed.
        /// - [`CrowdfundError::DeadlineAlreadyExtended`] if the deadline was already extended once.
        /// - [`CrowdfundError::InvalidDeadline`] if `new_deadline` does not extend the current deadline.
        pub fn extend_deadline(env: Env, new_deadline: u32) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            let extended: bool = get_instance(&env, &DataKey::DeadlineExtended)?;
            if extended {
                return Err(CrowdfundError::DeadlineAlreadyExtended);
            }

            if new_deadline <= deadline {
                return Err(CrowdfundError::InvalidDeadline);
            }

            env.storage()
                .instance()
                .set(&DataKey::Deadline, &new_deadline);
            env.storage()
                .instance()
                .set(&DataKey::DeadlineExtended, &true);

            bump_instance(&env);
            events::deadline_extended(&env, &creator, new_deadline);
            Ok(())
        }

        /// Creator claims all pledged funds after the deadline when the goal is met.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        /// - [`CrowdfundError::DeadlineNotReached`] if the deadline has not passed.
        /// - [`CrowdfundError::GoalNotMet`] if total pledged < goal.
        /// - [`CrowdfundError::AlreadyClaimed`] if funds were already claimed.
        pub fn claim(env: Env) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() <= deadline {
                return Err(CrowdfundError::DeadlineNotReached);
            }

            let claimed: bool = get_instance(&env, &DataKey::Claimed)?;
            if claimed {
                return Err(CrowdfundError::AlreadyClaimed);
            }

            let goal: i128 = get_instance(&env, &DataKey::Goal)?;
            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            if total < goal {
                return Err(CrowdfundError::GoalNotMet);
            }

            env.storage().instance().set(&DataKey::Claimed, &true);

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &creator,
                &total,
            );

            bump_instance(&env);
            events::claimed(&env, &creator, total);
            Ok(())
        }

        /// Contributor reclaims their pledge after the deadline when the goal was not met.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlineNotReached`] if the deadline has not passed.
        /// - [`CrowdfundError::GoalAlreadyMet`] if the goal was met (creator should claim instead).
        /// - [`CrowdfundError::NothingToWithdraw`] if the caller has no pledge to refund.
        pub fn refund(env: Env, pledger: Address) -> Result<(), CrowdfundError> {
            get_instance::<Address>(&env, &DataKey::Creator)?; // ensure initialized

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() <= deadline {
                return Err(CrowdfundError::DeadlineNotReached);
            }

            let goal: i128 = get_instance(&env, &DataKey::Goal)?;
            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            if total >= goal {
                return Err(CrowdfundError::GoalAlreadyMet);
            }

            pledger.require_auth();

            let pledge: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pledge(pledger.clone()))
                .unwrap_or(0);
            if pledge <= 0 {
                return Err(CrowdfundError::NothingToWithdraw);
            }

            env.storage()
                .persistent()
                .remove(&DataKey::Pledge(pledger.clone()));

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &pledger,
                &pledge,
            );

            bump_instance(&env);
            events::refunded(&env, &pledger, pledge);
            Ok(())
        }

        /// Return campaign details, including which funding tiers have been met.
        #[must_use]
        pub fn get_info(env: Env) -> Result<CrowdfundInfo, CrowdfundError> {
            let total_pledged: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let tiers: Vec<FundingTier> = get_instance(&env, &DataKey::Tiers)?;
            let mut tier_status = Vec::new(&env);
            for tier in tiers.iter() {
                tier_status.push_back(TierStatus {
                    threshold: tier.threshold,
                    description: tier.description.clone(),
                    met: total_pledged >= tier.threshold,
                });
            }

            Ok(CrowdfundInfo {
                creator: get_instance(&env, &DataKey::Creator)?,
                token: get_instance(&env, &DataKey::Token)?,
                goal: get_instance(&env, &DataKey::Goal)?,
                deadline: get_instance(&env, &DataKey::Deadline)?,
                total_pledged,
                claimed: get_instance(&env, &DataKey::Claimed)?,
                tiers: tier_status,
                max_pledge_per_address: env.storage().instance().get(&DataKey::MaxPledgePerAddress),
            })
        }

        /// Return a contributor's current pledge amount.
        #[must_use]
        pub fn get_pledge(env: Env, pledger: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::Pledge(pledger))
                .unwrap_or(0)
        }
    }
}

mod test;

#[cfg(test)]
mod prop_test;
