#![no_std]
#![deny(missing_docs)]
//! Token vesting contract: linear-with-cliff and milestone-based schedules,
//! batch creation, and exact-conservation revocation.

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use errors::VestingError;
pub use storage::{BeneficiarySchedule, DataKey, ScheduleParams};

use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance, extend_ttl_persistent,
};

/// `a * b / d` rounded down, erroring on overflow or a zero divisor.
fn mul_div(a: i128, b: i128, d: i128) -> Result<i128, ()> {
    a.checked_mul(b).and_then(|p| p.checked_div(d)).ok_or(())
}

const BPS: i128 = 10_000;

fn bump(env: &Env, key: &DataKey) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    extend_ttl_persistent(env, key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Tokens vested at `now` for a linear schedule: zero before the cliff, linear
/// from `start` to `end`, rounded down.
pub(crate) fn vested_linear(
    amount: i128,
    start: u32,
    cliff: u32,
    end: u32,
    now: u32,
) -> Result<i128, VestingError> {
    if now < cliff {
        return Ok(0);
    }
    if now >= end {
        return Ok(amount);
    }
    mul_div(amount, i128::from(now - start), i128::from(end - start))
        .map_err(|_| VestingError::ArithmeticError)
}

fn vested_now(s: &BeneficiarySchedule, now: u32) -> Result<i128, VestingError> {
    if s.milestones.is_empty() {
        return vested_linear(s.amount, s.start_ledger, s.cliff_ledger, s.end_ledger, now);
    }
    let mut bps: i128 = 0;
    for (id, w) in s.milestones.iter() {
        if s.approved.contains(id) {
            bps += i128::from(w);
        }
    }
    mul_div(s.amount, bps, BPS).map_err(|_| VestingError::ArithmeticError)
}

/// Enforce `now <= start <= cliff < end`.
fn validate_bounds(start: u32, cliff: u32, end: u32, now: u32) -> Result<(), VestingError> {
    if start < now || cliff < start || end <= cliff {
        return Err(VestingError::InvalidScheduleBounds);
    }
    Ok(())
}

fn load_admin(env: &Env) -> Result<Address, VestingError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(VestingError::NotInitialized)
}

fn load_token(env: &Env) -> Result<Address, VestingError> {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(VestingError::NotInitialized)
}

fn load_schedule(env: &Env, b: &Address) -> Result<BeneficiarySchedule, VestingError> {
    env.storage()
        .persistent()
        .get(&DataKey::Schedule(b.clone()))
        .ok_or(VestingError::ScheduleNotFound)
}

fn save_schedule(env: &Env, b: &Address, s: &BeneficiarySchedule) {
    let key = DataKey::Schedule(b.clone());
    env.storage().persistent().set(&key, s);
    bump(env, &key);
}

fn store_linear(env: &Env, p: &ScheduleParams) -> Result<(), VestingError> {
    if p.amount <= 0 {
        return Err(VestingError::InvalidAmount);
    }
    validate_bounds(p.start_ledger, p.cliff_ledger, p.end_ledger, env.ledger().sequence())?;
    if env.storage().persistent().has(&DataKey::Schedule(p.beneficiary.clone())) {
        return Err(VestingError::ScheduleAlreadyExists);
    }
    let s = BeneficiarySchedule {
        amount: p.amount,
        start_ledger: p.start_ledger,
        cliff_ledger: p.cliff_ledger,
        end_ledger: p.end_ledger,
        claimed: 0,
        revoked: false,
        is_revocable: p.is_revocable,
        milestones: Vec::new(env),
        approved: Vec::new(env),
        verifier: None,
    };
    save_schedule(env, &p.beneficiary, &s);
    events::created(env, &p.beneficiary, p.amount);
    Ok(())
}

pub use contract::*;

mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct VestingContract;

    #[contractimpl]
    impl VestingContract {
        /// Initialize with admin and token. Callable once.
        pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), VestingError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(VestingError::AlreadyInitialized);
            }
            admin.require_auth();
            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Token, &token);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            Ok(())
        }

        /// Create a linear schedule and fund it from the admin.
        ///
        /// # Errors
        /// - [`VestingError::InvalidScheduleBounds`] unless
        ///   `current <= start <= cliff < end`.
        pub fn create_schedule(
            env: Env,
            beneficiary: Address,
            start_ledger: u32,
            cliff_ledger: u32,
            end_ledger: u32,
            amount: i128,
            is_revocable: bool,
        ) -> Result<(), VestingError> {
            let admin = load_admin(&env)?;
            let token = load_token(&env)?;
            admin.require_auth();
            store_linear(
                &env,
                &ScheduleParams {
                    beneficiary,
                    start_ledger,
                    cliff_ledger,
                    end_ledger,
                    amount,
                    is_revocable,
                },
            )?;
            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );
            Ok(())
        }

        /// Create many linear schedules, funded by one aggregated transfer.
        /// All-or-nothing: any invalid entry reverts the whole batch.
        pub fn create_schedules_batch(
            env: Env,
            admin: Address,
            schedules: Vec<ScheduleParams>,
        ) -> Result<(), VestingError> {
            if admin != load_admin(&env)? {
                return Err(VestingError::NotAuthorized);
            }
            let token = load_token(&env)?;
            admin.require_auth();
            if schedules.is_empty() {
                return Err(VestingError::InvalidSchedule);
            }
            let mut total: i128 = 0;
            for p in schedules.iter() {
                store_linear(&env, &p)?;
                total = total
                    .checked_add(p.amount)
                    .ok_or(VestingError::ArithmeticError)?;
            }
            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &total,
            );
            Ok(())
        }

        /// Create a milestone-based schedule. `milestones` is a list of
        /// `(milestone_id, weight_bps)` whose weights sum to 10_000 and whose
        /// ids are unique. Tokens unlock as the `verifier` approves milestones.
        pub fn create_milestone_schedule(
            env: Env,
            beneficiary: Address,
            verifier: Address,
            milestones: Vec<(u32, u32)>,
            amount: i128,
            is_revocable: bool,
        ) -> Result<(), VestingError> {
            let admin = load_admin(&env)?;
            let token = load_token(&env)?;
            admin.require_auth();
            if amount <= 0 {
                return Err(VestingError::InvalidAmount);
            }
            if milestones.is_empty() {
                return Err(VestingError::InvalidSchedule);
            }
            let mut sum: i128 = 0;
            let mut seen: Vec<u32> = Vec::new(&env);
            for (id, w) in milestones.iter() {
                if w == 0 || seen.contains(id) {
                    return Err(VestingError::InvalidSchedule);
                }
                seen.push_back(id);
                sum += i128::from(w);
            }
            if sum != BPS {
                return Err(VestingError::InvalidSchedule);
            }
            if env.storage().persistent().has(&DataKey::Schedule(beneficiary.clone())) {
                return Err(VestingError::ScheduleAlreadyExists);
            }
            let s = BeneficiarySchedule {
                amount,
                start_ledger: 0,
                cliff_ledger: 0,
                end_ledger: 0,
                claimed: 0,
                revoked: false,
                is_revocable,
                milestones,
                approved: Vec::new(&env),
                verifier: Some(verifier),
            };
            save_schedule(&env, &beneficiary, &s);
            token::Client::new(&env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );
            events::created(&env, &beneficiary, amount);
            Ok(())
        }

        /// Approve a milestone (the schedule is identified by its beneficiary).
        /// Only the designated verifier may call.
        pub fn approve_milestone(
            env: Env,
            beneficiary: Address,
            milestone_id: u32,
        ) -> Result<(), VestingError> {
            let mut s = load_schedule(&env, &beneficiary)?;
            let verifier = s.verifier.clone().ok_or(VestingError::NotMilestoneSchedule)?;
            verifier.require_auth();
            if s.revoked {
                return Err(VestingError::AlreadyRevoked);
            }
            if !s.milestones.iter().any(|(id, _)| id == milestone_id) {
                return Err(VestingError::MilestoneNotFound);
            }
            if s.approved.contains(milestone_id) {
                return Err(VestingError::MilestoneAlreadyApproved);
            }
            s.approved.push_back(milestone_id);
            save_schedule(&env, &beneficiary, &s);
            events::milestone_approved(&env, &beneficiary, milestone_id);
            Ok(())
        }

        /// Claim vested, unclaimed tokens. Returns the amount paid.
        pub fn claim(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            let token = load_token(&env)?;
            beneficiary.require_auth();
            let mut s = load_schedule(&env, &beneficiary)?;
            let claimable = vested_now(&s, env.ledger().sequence())? - s.claimed;
            if claimable <= 0 {
                return Err(VestingError::NothingToClaim);
            }
            s.claimed += claimable;
            save_schedule(&env, &beneficiary, &s);
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &beneficiary,
                &claimable,
            );
            events::claimed(&env, &beneficiary, claimable);
            Ok(claimable)
        }

        /// Revoke a revocable schedule. Pays the beneficiary the vested but
        /// unclaimed portion and claws the rest back to the admin, so that
        /// `claimed_before + paid + clawback == total allocated` exactly.
        /// Returns `(paid_to_beneficiary, clawback_to_admin)`.
        pub fn revoke(env: Env, beneficiary: Address) -> Result<(i128, i128), VestingError> {
            let admin = load_admin(&env)?;
            let token = load_token(&env)?;
            admin.require_auth();
            let mut s = load_schedule(&env, &beneficiary)?;
            if !s.is_revocable {
                return Err(VestingError::ScheduleIrrevocable);
            }
            if s.revoked {
                return Err(VestingError::AlreadyRevoked);
            }
            let vested = vested_now(&s, env.ledger().sequence())?;
            let clawback = s.amount - vested;
            let pay = vested - s.claimed;

            s.amount = vested;
            s.claimed = vested;
            s.revoked = true;
            save_schedule(&env, &beneficiary, &s);

            let client = token::Client::new(&env, &token);
            let me = env.current_contract_address();
            if pay > 0 {
                client.transfer(&me, &beneficiary, &pay);
            }
            if clawback > 0 {
                client.transfer(&me, &admin, &clawback);
            }
            events::revoked(&env, &beneficiary, pay, clawback);
            Ok((pay, clawback))
        }

        /// Return a beneficiary's schedule, if any.
        pub fn get_info(env: Env, beneficiary: Address) -> Option<BeneficiarySchedule> {
            env.storage().persistent().get(&DataKey::Schedule(beneficiary))
        }

        /// Tokens vested for a beneficiary as of the current ledger.
        pub fn vested(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            let s = load_schedule(&env, &beneficiary)?;
            vested_now(&s, env.ledger().sequence())
        }
    }
}
