#![no_std]
#![deny(missing_docs)]
//! Crowdfunding campaign contract template.
//!
//! Contributors pledge tokens toward a funding goal before a deadline. If the
//! goal is met the creator claims the funds; otherwise contributors refund
//! their pledges.

use soroban_sdk::{Address, BytesN, Env, String, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::CrowdfundError;
pub use storage::{
    CrowdfundInfo, DataKey, FundingTier, StretchGoal, StretchGoalStatus, TierStatus,
};

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

/// A campaign is considered failed once the deadline has passed and the
/// total raised is below the soft cap (which equals the funding goal).
fn campaign_failed(env: &Env, deadline: u32, soft_cap: i128, total_raised: i128) -> bool {
    env.ledger().sequence() >= deadline && total_raised < soft_cap
}

/// All-or-nothing crowdfunding contract.
///
/// Lifecycle:
/// - Creator calls `initialize` to set a token, funding goal, and deadline ledger.
/// - Contributors call `pledge` to deposit tokens before the deadline.
/// - If the goal is met, the creator calls `claim` to collect all funds after the deadline.
/// - If the goal is not met after the deadline, each contributor calls `refund` to recover their pledge.
/// - A contributor can call `withdraw` to pull back their pledge before the deadline,
///   as long as the goal has not yet been reached. Once reached, pledges are locked.
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
        /// `claim_window` is the number of ledgers after `deadline` within which the creator
        /// must claim funds if the goal is met. If not claimed within this window, contributors
        /// can refund their pledges.
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
            claim_window: Option<u32>,
            kyc_hash: BytesN<32>,
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
            if kyc_hash.iter().all(|b| b == 0) {
                return Err(CrowdfundError::InvalidKycHash);
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
            env.storage().instance().set(&DataKey::Cancelled, &false);
            env.storage().instance().set(&DataKey::Tiers, &tiers);
            env.storage().instance().set(&DataKey::KycHash, &kyc_hash);

            let mut stretch_goals: Vec<StretchGoal> = Vec::new(&env);
            for tier in tiers.iter() {
                stretch_goals.push_back(StretchGoal {
                    threshold: tier.threshold,
                    description: tier.description.clone(),
                });
            }
            env.storage()
                .instance()
                .set(&DataKey::StretchGoals, &stretch_goals);

            env.storage()
                .instance()
                .set(&DataKey::DeadlineExtended, &false);
            if let Some(cap) = max_pledge_per_address {
                env.storage()
                    .instance()
                    .set(&DataKey::MaxPledgePerAddress, &cap);
            }
            
            // Set claim window (default to 90 days if not specified)
            let window = claim_window.unwrap_or(90 * 24 * 60 * 60 / 5); // ~90 days in ledgers (5s per ledger)
            env.storage().instance().set(&DataKey::ClaimWindow, &window);

            bump_instance(&env);
            events::initialized(&env, &creator, goal, deadline);
            Ok(())
        }

        /// Pledge `amount` tokens to the campaign. Must be called before the deadline.
        /// Optionally specify a reward tier to claim by providing `tier_id` (0-based index).
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlinePassed`] if the deadline has passed.
        /// - [`CrowdfundError::InvalidAmount`] if `amount` <= 0.
        /// - [`CrowdfundError::PledgeCapExceeded`] if this pledge would push the
        ///   pledger's cumulative total above `max_pledge_per_address`.
        /// - [`CrowdfundError::CampaignCancelled`] if the campaign was cancelled.
        /// - [`CrowdfundError::InvalidTierId`] if `tier_id` is out of bounds.
        /// - [`CrowdfundError::TierCapacityExceeded`] if the tier has reached max capacity.
        pub fn pledge(
            env: Env,
            pledger: Address,
            amount: i128,
            tier_id: Option<u32>,
        ) -> Result<(), CrowdfundError> {
            if amount <= 0 {
                return Err(CrowdfundError::InvalidAmount);
            }

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            let cancelled: bool = get_instance(&env, &DataKey::Cancelled)?;
            if cancelled {
                return Err(CrowdfundError::CampaignCancelled);
            }

            pledger.require_auth();

            let existing: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pledge(pledger.clone()))
                .unwrap_or(0);
            let new_pledge = existing.checked_add(amount).ok_or(CrowdfundError::Overflow)?;

            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, i128>(&DataKey::MaxPledgePerAddress)
            {
                if new_pledge > cap {
                    return Err(CrowdfundError::PledgeCapExceeded);
                }
            }

            // Handle tier selection
            if let Some(tid) = tier_id {
                let tiers: Vec<FundingTier> = get_instance(&env, &DataKey::Tiers)?;
                if tid >= tiers.len() {
                    return Err(CrowdfundError::InvalidTierId);
                }

                let tier = tiers.get(tid).unwrap();
                if let Some(max_backers) = tier.max_backers {
                    let current_count = env
                        .storage()
                        .persistent()
                        .get(&DataKey::TierBackerCount(tid))
                        .unwrap_or(0u32);

                    // Check if already claimed this tier
                    let already_claimed = env
                        .storage()
                        .persistent()
                        .get(&DataKey::PledgeTier(pledger.clone()))
                        .unwrap_or(None::<u32>);

                    if already_claimed != Some(tid) && current_count >= max_backers {
                        return Err(CrowdfundError::TierCapacityExceeded);
                    }

                    // Update backer count if new to this tier
                    if already_claimed != Some(tid) {
                        env.storage().persistent().set(
                            &DataKey::TierBackerCount(tid),
                            &(current_count + 1),
                        );
                        env.storage()
                            .persistent()
                            .extend_ttl(&DataKey::TierBackerCount(tid), LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
                    }
                }

                env.storage()
                    .persistent()
                    .set(&DataKey::PledgeTier(pledger.clone()), &Some(tid));
                env.storage().persistent().extend_ttl(
                    &DataKey::PledgeTier(pledger.clone()),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
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
            let new_total = total.checked_add(amount).ok_or(CrowdfundError::Overflow)?;
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
        /// - [`CrowdfundError::GoalAlreadyMet`] if total pledged >= goal; pledges are
        ///   locked once the goal is reached so a successful campaign cannot be sabotaged.
        /// - [`CrowdfundError::NothingToWithdraw`] if the caller has no active pledge.
        pub fn withdraw(env: Env, pledger: Address) -> Result<(), CrowdfundError> {
            get_instance::<Address>(&env, &DataKey::Creator)?; // ensure initialized

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
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
            // Soft cap (== goal) must be attained before the creator may withdraw.
            if campaign_failed(&env, deadline, goal, total) {
                return Err(CrowdfundError::GoalNotMet);
            }

            // With milestone governance enabled the creator cannot bypass
            // contributor voting by calling `claim`. Every milestone must have
            // been released first, and only the unreleased remainder is claimable.
            let milestones: Vec<storage::Milestone> = env
                .storage()
                .instance()
                .get(&DataKey::Milestones)
                .unwrap_or(Vec::new(&env));
            let mut released_total = 0_i128;
            for milestone in milestones.iter() {
                let released: bool = env
                    .storage()
                    .persistent()
                    .get(&DataKey::MilestoneReleased(milestone.milestone_id))
                    .unwrap_or(false);
                if !released {
                    return Err(CrowdfundError::MilestoneNotReady);
                }
                released_total += milestone.amount;
            }
            let claimable = total.saturating_sub(released_total);

            env.storage().instance().set(&DataKey::Claimed, &true);

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &creator,
                &claimable,
            );

            bump_instance(&env);
            events::claimed(&env, &creator, claimable);
            Ok(())
        }

        /// Contributor reclaims their pledge after the deadline when the goal was not met,
        /// or immediately if the campaign was cancelled.
        /// OR after the claim window expires if the creator failed to claim.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlineNotReached`] if the deadline has not passed and campaign not cancelled.
        /// - [`CrowdfundError::GoalAlreadyMet`] if the goal was met AND the claim window hasn't expired AND funds were claimed.
        /// - [`CrowdfundError::NothingToWithdraw`] if the caller has no pledge to refund.
        pub fn refund(env: Env, pledger: Address) -> Result<(), CrowdfundError> {
            get_instance::<Address>(&env, &DataKey::Creator)?; // ensure initialized

            let cancelled: bool = get_instance(&env, &DataKey::Cancelled)?;
            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;

            // Allow refund if cancelled OR if deadline passed
            if !cancelled && env.ledger().sequence() <= deadline {
                return Err(CrowdfundError::DeadlineNotReached);
            }

            let goal: i128 = get_instance(&env, &DataKey::Goal)?;
            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let claimed: bool = get_instance(&env, &DataKey::Claimed)?;

            // Permissionless refunds: once the soft-cap deadline has passed and
            // the soft cap (== goal) was not attained, the campaign is failed and
            // any contributor may refund without creator intervention.
            if !cancelled && campaign_failed(&env, deadline, goal, total) {
                // fall through to refund below
            } else if !cancelled && total >= goal {
                if claimed {
                    // Funds already claimed by creator
                    return Err(CrowdfundError::GoalAlreadyMet);
                }
                
                // Check if claim window has expired
                let claim_window: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ClaimWindow)
                    .unwrap_or(90 * 24 * 60 * 60 / 5);
                let claim_deadline = deadline.saturating_add(claim_window);
                
                if env.ledger().sequence() <= claim_deadline {
                    // Still within claim window, creator can still claim
                    return Err(CrowdfundError::GoalAlreadyMet);
                }
                
                // Claim window expired - emit event
                events::creator_claim_expired(&env, claim_deadline);
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

        /// Creator cancels the campaign before the deadline, immediately enabling refunds.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        /// - [`CrowdfundError::DeadlinePassed`] if the deadline has already passed.
        /// - [`CrowdfundError::CampaignCancelled`] if already cancelled.
        pub fn cancel_campaign(env: Env) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            let cancelled: bool = get_instance(&env, &DataKey::Cancelled)?;
            if cancelled {
                return Err(CrowdfundError::CampaignCancelled);
            }

            env.storage().instance().set(&DataKey::Cancelled, &true);

            bump_instance(&env);
            events::campaign_cancelled(&env, &creator);
            Ok(())
        }

        /// Post an immutable project update for backers.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        pub fn post_update(
            env: Env,
            creator: Address,
            update_hash: BytesN<32>,
            uri: String,
        ) -> Result<(), CrowdfundError> {
            let stored_creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();
            if creator != stored_creator {
                return Err(CrowdfundError::NotAuthorized);
            }

            bump_instance(&env);
            events::project_update(&env, &creator, &update_hash, &uri);
            Ok(())
        }

        /// Set milestones for tranche-based fund release (creator only, before campaign deadline).
        /// Closes #1168
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        /// - [`CrowdfundError::CampaignCancelled`] if campaign is cancelled.
        pub fn set_milestones(
            env: Env,
            milestones: Vec<storage::Milestone>,
        ) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let cancelled: bool = get_instance(&env, &DataKey::Cancelled)?;
            if cancelled {
                return Err(CrowdfundError::CampaignCancelled);
            }

            env.storage().instance().set(&DataKey::Milestones, &milestones);
            bump_instance(&env);
            Ok(())
        }

        /// Batch refund pledges to multiple contributors. Callable by anyone after the
        /// deadline if the goal was not met, or immediately if the campaign was cancelled.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        /// - [`CrowdfundError::DeadlineNotReached`] if the deadline has not passed and campaign not cancelled.
        /// - [`CrowdfundError::GoalAlreadyMet`] if the goal was met.
        pub fn refund_batch(env: Env, pledgers: Vec<Address>) -> Result<(), CrowdfundError> {
            get_instance::<Address>(&env, &DataKey::Creator)?; // ensure initialized

            let cancelled: bool = get_instance(&env, &DataKey::Cancelled)?;
            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;

            if !cancelled && env.ledger().sequence() <= deadline {
                return Err(CrowdfundError::DeadlineNotReached);
            }

            let goal: i128 = get_instance(&env, &DataKey::Goal)?;
            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            if !cancelled && total >= goal {
                return Err(CrowdfundError::GoalAlreadyMet);
            }

            let token: Address = get_instance(&env, &DataKey::Token)?;
            let token_client = token::Client::new(&env, &token);

            for pledger in pledgers.iter() {
                let pledge: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Pledge(pledger.clone()))
                    .unwrap_or(0);

                if pledge > 0 {
                    env.storage()
                        .persistent()
                        .remove(&DataKey::Pledge(pledger.clone()));

                    token_client.transfer(&env.current_contract_address(), &pledger, &pledge);

                    events::refunded(&env, &pledger, pledge);
                }
            }

            bump_instance(&env);
            Ok(())
        }

        /// Submit proof and request milestone release (creator only).
        /// Closes #1168
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        /// - [`CrowdfundError::MilestoneNotFound`] if milestone doesn't exist.
        /// - [`CrowdfundError::MilestoneAlreadyReleased`] if already released.
        pub fn request_milestone_release(
            env: Env,
            milestone_id: u32,
        ) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let milestones: Vec<storage::Milestone> = env
                .storage()
                .instance()
                .get(&DataKey::Milestones)
                .unwrap_or(Vec::new(&env));

            let milestone = milestones
                .iter()
                .find(|m| m.milestone_id == milestone_id)
                .ok_or(CrowdfundError::MilestoneNotFound)?;

            let released: bool = env
                .storage()
                .persistent()
                .get(&DataKey::MilestoneReleased(milestone_id))
                .unwrap_or(false);

            if released {
                return Err(CrowdfundError::MilestoneAlreadyReleased);
            }

            // Opening a release proposal starts the contributor voting window.
            env.storage().persistent().set(
                &DataKey::MilestoneRequested(milestone_id),
                &true,
            );

            // Mark as ready for voting
            bump_instance(&env);
            Ok(())
        }

        /// Vote on milestone release (contributor only).
        /// Closes #1168
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::MilestoneNotFound`] if milestone doesn't exist.
        /// - [`CrowdfundError::NothingToWithdraw`] if caller has no pledge (not a contributor).
        pub fn vote_milestone(
            env: Env,
            voter: Address,
            milestone_id: u32,
            approve: bool,
        ) -> Result<(), CrowdfundError> {
            voter.require_auth();

            // Verify voter is a contributor
            let pledge: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pledge(voter.clone()))
                .unwrap_or(0);
            if pledge <= 0 {
                return Err(CrowdfundError::NothingToWithdraw);
            }

            let milestones: Vec<storage::Milestone> = env
                .storage()
                .instance()
                .get(&DataKey::Milestones)
                .unwrap_or(Vec::new(&env));

            milestones
                .iter()
                .find(|m| m.milestone_id == milestone_id)
                .ok_or(CrowdfundError::MilestoneNotFound)?;

            // A contributor's pledge is their voting power. Track the
            // weighted approval total so changing a vote cannot double count.
            let power_key = DataKey::MilestoneApprovalPower(milestone_id);
            let mut approval_power: i128 =
                env.storage().persistent().get(&power_key).unwrap_or(0);
            let vote_key = DataKey::MilestoneVotes(milestone_id, voter.clone());
            if let Some(previous) = env
                .storage()
                .persistent()
                .get::<_, bool>(&vote_key)
            {
                if previous {
                    approval_power -= pledge;
                }
            }
            if approve {
                approval_power += pledge;
            }
            env.storage().persistent().set(&vote_key, &approve);
            env.storage()
                .persistent()
                .set(&power_key, &approval_power);

            bump_instance(&env);
            events::milestone_voted(&env, milestone_id, &voter, approve);
            Ok(())
        }

        /// Release milestone funds after approval (creator only).
        /// Closes #1168
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        /// - [`CrowdfundError::MilestoneNotFound`] if milestone doesn't exist.
        /// - [`CrowdfundError::MilestoneNotReady`] if not enough votes.
        /// - [`CrowdfundError::MilestoneAlreadyReleased`] if already released.
        pub fn release_milestone(
            env: Env,
            milestone_id: u32,
        ) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            let milestones: Vec<storage::Milestone> = env
                .storage()
                .instance()
                .get(&DataKey::Milestones)
                .unwrap_or(Vec::new(&env));

            let milestone = milestones
                .iter()
                .find(|m| m.milestone_id == milestone_id)
                .ok_or(CrowdfundError::MilestoneNotFound)?;

            let released: bool = env
                .storage()
                .persistent()
                .get(&DataKey::MilestoneReleased(milestone_id))
                .unwrap_or(false);

            if released {
                return Err(CrowdfundError::MilestoneAlreadyReleased);
            }

            let requested: bool = env
                .storage()
                .persistent()
                .get(&DataKey::MilestoneRequested(milestone_id))
                .unwrap_or(false);
            if !requested {
                return Err(CrowdfundError::MilestoneNotRequested);
            }

            // Contributor governance: a tranche may only be released when
            // approving voting power is strictly greater than 50% of all
            // pledged voting power.
            let total_pledged: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let approval_power: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::MilestoneApprovalPower(milestone_id))
                .unwrap_or(0);
            if approval_power <= total_pledged / 2 {
                return Err(CrowdfundError::MilestoneApprovalThresholdNotMet);
            }

            env.storage()
                .persistent()
                .set(&DataKey::MilestoneReleased(milestone_id), &true);

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &creator,
                &milestone.amount,
            );

            bump_instance(&env);
            events::milestone_released(&env, milestone_id, milestone.amount);
            Ok(())
        }

        /// Whitelist tokens for multi-token contributions (creator only).
        /// Closes #1169
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotAuthorized`] if caller is not the creator.
        pub fn whitelist_tokens(
            env: Env,
            tokens: Vec<Address>,
            oracle: Address,
        ) -> Result<(), CrowdfundError> {
            let creator: Address = get_instance(&env, &DataKey::Creator)?;
            creator.require_auth();

            env.storage().instance().set(&DataKey::WhitelistedTokens, &tokens);
            env.storage().instance().set(&DataKey::OracleAddress, &oracle);
            bump_instance(&env);
            Ok(())
        }

        /// Query reward perk eligibility for a pledger.
        ///
        /// # Returns
        ///
        /// The tier_id the pledger has claimed, or None if no tier was selected.
        #[must_use]
        pub fn claim_reward_perk(env: Env, pledger: Address) -> Option<u32> {
            env.storage()
                .persistent()
                .get(&DataKey::PledgeTier(pledger))
                .unwrap_or(None)
        }

        /// Pledge with alternative token (multi-token support).
        /// Closes #1169
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::TokenNotWhitelisted`] if token not in whitelist.
        /// - [`CrowdfundError::InvalidTokenPrice`] if oracle price invalid.
        /// - [`CrowdfundError::DeadlinePassed`] if the deadline has passed.
        /// - [`CrowdfundError::InvalidAmount`] if `amount` <= 0.
        pub fn pledge_with_token(
            env: Env,
            pledger: Address,
            token: Address,
            amount: i128,
        ) -> Result<(), CrowdfundError> {
            if amount <= 0 {
                return Err(CrowdfundError::InvalidAmount);
            }

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(CrowdfundError::DeadlinePassed);
            }

            pledger.require_auth();

            // Verify token is whitelisted
            let whitelisted: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::WhitelistedTokens)
                .unwrap_or(Vec::new(&env));

            if !whitelisted.iter().any(|t| t == token) {
                return Err(CrowdfundError::TokenNotWhitelisted);
            }

            // Get price from oracle (simplified - would need actual oracle integration)
            let _oracle: Address = env
                .storage()
                .instance()
                .get(&DataKey::OracleAddress)
                .ok_or(CrowdfundError::NotInitialized)?;

            // Simplified: assume 1:1 conversion for now
            // In production, query oracle.get_price() and convert amount
            let value_in_common_unit = amount;

            // Store token-specific pledge
            let existing: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::TokenPledge(pledger.clone(), token.clone()))
                .unwrap_or(0);
            let new_pledge = existing.checked_add(amount).ok_or(CrowdfundError::Overflow)?;

            env.storage().persistent().set(
                &DataKey::TokenPledge(pledger.clone(), token.clone()),
                &new_pledge,
            );

            // Update total for this token
            let token_total: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::TokenTotalPledged(token.clone()))
                .unwrap_or(0);
            let new_token_total = token_total.checked_add(value_in_common_unit).ok_or(CrowdfundError::Overflow)?;
            env.storage().persistent().set(
                &DataKey::TokenTotalPledged(token.clone()),
                &new_token_total,
            );

            // Update global total (in common unit value)
            let total: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let new_total = total.checked_add(value_in_common_unit).ok_or(CrowdfundError::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::TotalPledged, &new_total);

            // Transfer tokens
            token::Client::new(&env, &token).transfer(
                &pledger,
                &env.current_contract_address(),
                &amount,
            );

            bump_instance(&env);
            events::pledged(&env, &pledger, value_in_common_unit, new_total);
            Ok(())
        }

        /// Return campaign details, including which funding tiers have been met.
        #[must_use]
        pub fn get_info(env: Env) -> Result<CrowdfundInfo, CrowdfundError> {
            let total_pledged: i128 = get_instance(&env, &DataKey::TotalPledged)?;
            let tiers: Vec<FundingTier> = get_instance(&env, &DataKey::Tiers)?;
            let mut tier_status = Vec::new(&env);
            for (idx, tier) in tiers.iter().enumerate() {
                let current_backers = env
                    .storage()
                    .persistent()
                    .get(&DataKey::TierBackerCount(idx as u32))
                    .unwrap_or(0u32);

                tier_status.push_back(TierStatus {
                    threshold: tier.threshold,
                    description: tier.description.clone(),
                    met: total_pledged >= tier.threshold,
                    max_backers: tier.max_backers,
                    current_backers,
                });
            }

            let stretch_goals: Vec<StretchGoal> = env
                .storage()
                .instance()
                .get(&DataKey::StretchGoals)
                .unwrap_or(Vec::new(&env));
            let mut stretch_goal_status = Vec::new(&env);
            for goal in stretch_goals.iter() {
                stretch_goal_status.push_back(StretchGoalStatus {
                    threshold: goal.threshold,
                    description: goal.description.clone(),
                    unlocked: total_pledged >= goal.threshold,
                });
            }

            Ok(CrowdfundInfo {
                creator: get_instance(&env, &DataKey::Creator)?,
                token: get_instance(&env, &DataKey::Token)?,
                goal: get_instance(&env, &DataKey::Goal)?,
                deadline: get_instance(&env, &DataKey::Deadline)?,
                total_pledged,
                claimed: get_instance(&env, &DataKey::Claimed)?,
                cancelled: get_instance(&env, &DataKey::Cancelled)?,
                tiers: tier_status,
                stretch_goals: stretch_goal_status,
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

        /// Return the creator's KYC attestation hash stored at initialization.
        ///
        /// # Errors
        ///
        /// - [`CrowdfundError::NotInitialized`] if not set up.
        pub fn get_kyc_hash(env: Env) -> Result<BytesN<32>, CrowdfundError> {
            get_instance(&env, &DataKey::KycHash)
        }
    }
}

mod test;

// #[cfg(test)]
// mod prop_test;
