#![no_std]
#![deny(missing_docs)]
//! Token vesting contract template.
//!
//! An admin deposits tokens under a linear (cliff → end) or stepped tranche
//! schedule; the beneficiary claims vested tokens over time and the admin may
//! revoke the unvested remainder of revocable schedules.
//!
//! # Dust protection
//!
//! The admin may configure a minimum claim amount
//! ([`VestingContract::set_min_claim_amount`]). Claims smaller than the
//! threshold are rejected unless they withdraw the final remainder of a
//! fully-vested schedule, which prevents per-ledger 1-stroop claims from
//! churning storage and bloating the event log.
//!
//! # Beneficiary key rotation
//!
//! If a beneficiary's key is lost or compromised, the beneficiary or the admin
//! can propose moving the schedule to a new address. The rotation is protected
//! by a timelock ([`VestingContract::set_rotation_delay`]) and must be
//! accepted by the new address once the delay has elapsed. While an
//! admin-proposed rotation is pending, claims from the old address are frozen
//! so a compromised key cannot drain vested tokens during the timelock.

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod test;

pub use errors::VestingError;
pub use storage::{BeneficiarySchedule, DataKey, PendingRotation, VestingInfo};

use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance, extend_ttl_persistent,
};

/// Default timelock (in ledgers, ~1 day at 5s/ledger) before a proposed
/// beneficiary rotation can be executed.
pub const DEFAULT_ROTATION_DELAY: u32 = 17_280;

fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Extend the TTL of a persistent entry. Instance storage TTL (bumped by
/// `bump`) does not cover persistent entries.
fn bump_key(env: &Env, key: &DataKey) {
    extend_ttl_persistent(env, key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Exact `floor(amount * num / den)` for `amount >= 0` and `0 <= num <= den`,
/// with `den <= u32::MAX`, that cannot overflow for any `i128` amount.
///
/// Splits `amount = q * den + r` so that
/// `amount * num / den = q * num + (r * num) / den`, where `q * num <= amount`
/// and `r * num < 2^64`.
fn scale_fraction(amount: i128, num: u32, den: u32) -> Result<i128, VestingError> {
    if den == 0 || num > den || amount < 0 {
        return Err(VestingError::ArithmeticError);
    }
    let (num, den) = (i128::from(num), i128::from(den));
    let q = amount / den;
    let r = amount % den;
    q.checked_mul(num)
        .and_then(|v| v.checked_add(r * num / den))
        .ok_or(VestingError::ArithmeticError)
}

/// Returns the number of tokens vested as of `ledger`, ignoring already-claimed tokens.
///
/// The `amount * elapsed` product is computed without overflow (see
/// [`scale_fraction`]), so large supplies and long durations are safe.
pub(crate) fn vested_amount(
    amount: i128,
    cliff_ledger: u32,
    end_ledger: u32,
    ledger: u32,
) -> Result<i128, VestingError> {
    if ledger < cliff_ledger {
        return Ok(0);
    }
    if ledger >= end_ledger {
        return Ok(amount);
    }
    scale_fraction(amount, ledger - cliff_ledger, end_ledger - cliff_ledger)
}

/// Returns the number of tokens vested as of `ledger` for a tranche-based
/// schedule. `tranches` is a list of `(ledger_sequence, percentage_bps)` pairs.
pub(crate) fn vested_amount_tranches(
    amount: i128,
    tranches: &Vec<(u32, u32)>,
    ledger: u32,
) -> Result<i128, VestingError> {
    let mut vested: i128 = 0;
    for (tranche_ledger, bps) in tranches.iter() {
        if ledger < tranche_ledger {
            break;
        }
        let release = scale_fraction(amount, bps, 10_000)?;
        vested = vested
            .checked_add(release)
            .ok_or(VestingError::ArithmeticError)?;
    }
    Ok(vested.min(amount))
}

/// Validate a tranche schedule: ledgers strictly increasing and in the future,
/// each BPS in `(0, 10_000]`, and the cumulative BPS not exceeding 10_000.
fn validate_tranches(tranches: &Vec<(u32, u32)>, now: u32) -> Result<(), VestingError> {
    if tranches.is_empty() {
        return Err(VestingError::InvalidSchedule);
    }
    let mut prev_ledger: Option<u32> = None;
    let mut total_bps: u32 = 0;
    for (tranche_ledger, bps) in tranches.iter() {
        if bps == 0 || bps > 10_000 || tranche_ledger <= now {
            return Err(VestingError::InvalidSchedule);
        }
        if prev_ledger.is_some_and(|p| tranche_ledger <= p) {
            return Err(VestingError::InvalidSchedule);
        }
        total_bps = total_bps
            .checked_add(bps)
            .ok_or(VestingError::InvalidSchedule)?;
        if total_bps > 10_000 {
            return Err(VestingError::InvalidSchedule);
        }
        prev_ledger = Some(tranche_ledger);
    }
    Ok(())
}

fn validate_schedule(cliff_ledger: u32, end_ledger: u32, now: u32) -> Result<(), VestingError> {
    if cliff_ledger >= end_ledger || end_ledger <= now {
        return Err(VestingError::InvalidSchedule);
    }
    Ok(())
}

fn get_admin(env: &Env) -> Result<Address, VestingError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(VestingError::NotInitialized)
}

fn get_token(env: &Env) -> Result<Address, VestingError> {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(VestingError::NotInitialized)
}

fn revocation_delay(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::RevocationDelay)
        .unwrap_or(0)
}

fn rotation_delay(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::RotationDelay)
        .unwrap_or(DEFAULT_ROTATION_DELAY)
}

fn load_schedule(env: &Env, beneficiary: &Address) -> Result<BeneficiarySchedule, VestingError> {
    env.storage()
        .persistent()
        .get(&DataKey::Schedule(beneficiary.clone()))
        .ok_or(VestingError::ScheduleNotFound)
}

fn save_schedule(env: &Env, beneficiary: &Address, schedule: &BeneficiarySchedule) {
    let key = DataKey::Schedule(beneficiary.clone());
    env.storage().persistent().set(&key, schedule);
    bump(env);
    bump_key(env, &key);
}

/// Ledger after which vesting stops accruing, if the schedule is (or is about
/// to be) revoked as of `now`.
fn vesting_cap(env: &Env, s: &BeneficiarySchedule, now: u32) -> Option<u32> {
    if s.revoked {
        Some(s.revocation_ledger)
    } else if s.revocation_pending {
        let finalize = s.revocation_ledger.saturating_add(revocation_delay(env));
        (now >= finalize).then_some(finalize)
    } else {
        None
    }
}

/// Total tokens vested for `s` as of `now`, accounting for milestone release,
/// tranches and revocation caps.
fn total_vested(env: &Env, s: &BeneficiarySchedule, now: u32) -> Result<i128, VestingError> {
    if s.milestone_released {
        return Ok(s.amount);
    }
    let at = vesting_cap(env, s, now).map_or(now, |cap| cap.min(now));
    if s.tranches.is_empty() {
        vested_amount(s.amount, s.cliff_ledger, s.end_ledger, at)
    } else {
        vested_amount_tranches(s.amount, &s.tranches, at)
    }
}

/// The most the beneficiary can ever receive from `s`: the full amount, or
/// the amount vested at the revocation cap for revoked schedules.
fn final_entitlement(env: &Env, s: &BeneficiarySchedule) -> Result<i128, VestingError> {
    if s.revoked {
        total_vested(env, s, s.revocation_ledger)
    } else {
        Ok(s.amount)
    }
}

/// Vesting contract.
///
/// Flow:
/// 1. Admin calls `initialize`, then `create_schedule` / `create_tranche_schedule`.
/// 2. Beneficiary calls `claim` to receive vested tokens.
/// 3. Admin may `revoke` revocable schedules (optionally with a grace period).
/// 4. Lost keys are handled with `propose_beneficiary_rotation` →
///    `accept_beneficiary_rotation` after a timelock.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct VestingContract;

    #[contractimpl]
    impl VestingContract {
        /// Initialize the vesting contract with admin and token.
        ///
        /// # Errors
        /// - [`VestingError::AlreadyInitialized`] if called more than once.
        pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), VestingError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(VestingError::AlreadyInitialized);
            }
            admin.require_auth();

            let store = env.storage().instance();
            store.set(&DataKey::Admin, &admin);
            store.set(&DataKey::Token, &token);
            store.set(&DataKey::Version, &1u32);
            store.set(&DataKey::AdminReleased, &0i128);
            store.set(&DataKey::RevocationDelay, &0u32);
            store.set(&DataKey::RotationDelay, &DEFAULT_ROTATION_DELAY);
            bump(&env);
            Ok(())
        }

        /// Configure the revocation grace period, in ledgers. `0` = immediate.
        pub fn set_revocation_delay(env: Env, delay: u32) -> Result<(), VestingError> {
            get_admin(&env)?.require_auth();
            env.storage().instance().set(&DataKey::RevocationDelay, &delay);
            bump(&env);
            Ok(())
        }

        /// Return the configured revocation grace period, in ledgers.
        pub fn revocation_delay(env: Env) -> u32 {
            revocation_delay(&env)
        }

        /// Configure the minimum amount a single `claim` must withdraw.
        ///
        /// `None` disables the threshold. Claims below the threshold are
        /// rejected unless they withdraw the final remainder of the schedule.
        ///
        /// # Errors
        /// - [`VestingError::InvalidAmount`] if the threshold is not positive.
        pub fn set_min_claim_amount(env: Env, min: Option<i128>) -> Result<(), VestingError> {
            get_admin(&env)?.require_auth();
            match min {
                Some(m) if m <= 0 => return Err(VestingError::InvalidAmount),
                Some(m) => env.storage().instance().set(&DataKey::MinClaimAmount, &m),
                None => env.storage().instance().remove(&DataKey::MinClaimAmount),
            }
            bump(&env);
            Ok(())
        }

        /// Return the configured minimum claim amount, if any.
        pub fn min_claim_amount(env: Env) -> Option<i128> {
            env.storage().instance().get(&DataKey::MinClaimAmount)
        }

        /// Configure the beneficiary rotation timelock, in ledgers.
        pub fn set_rotation_delay(env: Env, delay: u32) -> Result<(), VestingError> {
            get_admin(&env)?.require_auth();
            env.storage().instance().set(&DataKey::RotationDelay, &delay);
            bump(&env);
            Ok(())
        }

        /// Return the beneficiary rotation timelock, in ledgers.
        pub fn rotation_delay(env: Env) -> u32 {
            rotation_delay(&env)
        }

        /// Create a linear vesting schedule and pull `amount` tokens from the admin.
        ///
        /// # Errors
        /// - [`VestingError::NotInitialized`] if the contract has not been initialized.
        /// - [`VestingError::InvalidAmount`] if `amount` <= 0.
        /// - [`VestingError::InvalidSchedule`] if `cliff_ledger >= end_ledger` or
        ///   `end_ledger` <= current ledger.
        /// - [`VestingError::ScheduleAlreadyExists`] if the beneficiary already has a schedule.
        pub fn create_schedule(
            env: Env,
            beneficiary: Address,
            cliff_ledger: u32,
            end_ledger: u32,
            amount: i128,
            is_revocable: bool,
        ) -> Result<(), VestingError> {
            validate_schedule(cliff_ledger, end_ledger, env.ledger().sequence())?;
            Self::store_new_schedule(
                &env,
                &beneficiary,
                cliff_ledger,
                end_ledger,
                amount,
                is_revocable,
                Vec::new(&env),
            )
        }

        /// Create a tranche-based vesting schedule. `tranches` is a list of
        /// `(ledger_sequence, percentage_bps)` pairs; ledgers must be strictly
        /// increasing and in the future, and BPS must sum to at most 10_000.
        ///
        /// # Errors
        /// Same as [`Self::create_schedule`].
        pub fn create_tranche_schedule(
            env: Env,
            beneficiary: Address,
            tranches: Vec<(u32, u32)>,
            amount: i128,
            is_revocable: bool,
        ) -> Result<(), VestingError> {
            validate_tranches(&tranches, env.ledger().sequence())?;
            let first = tranches.first().ok_or(VestingError::InvalidSchedule)?.0;
            let last = tranches.last().ok_or(VestingError::InvalidSchedule)?.0;
            Self::store_new_schedule(
                &env,
                &beneficiary,
                first,
                last,
                amount,
                is_revocable,
                tranches,
            )
        }

        /// Set the oracle (e.g. a multi-sig) allowed to release the milestone.
        pub fn set_milestone_oracle(
            env: Env,
            beneficiary: Address,
            oracle: Address,
        ) -> Result<(), VestingError> {
            get_admin(&env)?.require_auth();
            let mut schedule = load_schedule(&env, &beneficiary)?;
            schedule.milestone_oracle = Some(oracle);
            save_schedule(&env, &beneficiary, &schedule);
            Ok(())
        }

        /// Release the milestone: the full schedule amount becomes vested.
        ///
        /// # Errors
        /// - [`VestingError::NotAuthorized`] if no oracle is configured.
        /// - [`VestingError::MilestoneAlreadyReleased`] if already released.
        pub fn release_milestone(env: Env, beneficiary: Address) -> Result<(), VestingError> {
            let mut schedule = load_schedule(&env, &beneficiary)?;
            let oracle = schedule
                .milestone_oracle
                .clone()
                .ok_or(VestingError::NotAuthorized)?;
            oracle.require_auth();
            if schedule.milestone_released {
                return Err(VestingError::MilestoneAlreadyReleased);
            }
            schedule.milestone_released = true;
            save_schedule(&env, &beneficiary, &schedule);
            Ok(())
        }

        /// Claim all currently vested, unclaimed tokens. Returns the amount claimed.
        ///
        /// # Errors
        /// - [`VestingError::ScheduleNotFound`] if no schedule exists.
        /// - [`VestingError::RotationPending`] if an admin-proposed key rotation
        ///   is pending for this beneficiary (claims are frozen).
        /// - [`VestingError::NothingToClaim`] if nothing is claimable.
        /// - [`VestingError::ClaimBelowMinimum`] if the claimable amount is below
        ///   the configured minimum and does not complete the schedule.
        pub fn claim(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            beneficiary.require_auth();
            let token = get_token(&env)?;
            let admin = get_admin(&env)?;
            let mut schedule = load_schedule(&env, &beneficiary)?;

            if let Some(p) = Self::get_pending_rotation(env.clone(), beneficiary.clone()) {
                if p.proposer == admin {
                    return Err(VestingError::RotationPending);
                }
            }

            let now = env.ledger().sequence();
            let vested = total_vested(&env, &schedule, now)?;
            let claimable = vested
                .checked_sub(schedule.claimed)
                .ok_or(VestingError::ArithmeticError)?;
            if claimable <= 0 {
                return Err(VestingError::NothingToClaim);
            }

            // Dust protection: reject micro-claims unless this claim takes the
            // final remainder of a fully-vested schedule.
            if let Some(min) = Self::min_claim_amount(env.clone()) {
                let completes = vested >= final_entitlement(&env, &schedule)?;
                if claimable < min && !completes {
                    return Err(VestingError::ClaimBelowMinimum);
                }
            }

            schedule.claimed = vested;
            save_schedule(&env, &beneficiary, &schedule);

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &beneficiary,
                &claimable,
            );
            events::claimed(&env, &beneficiary, claimable);
            Ok(claimable)
        }

        /// Revoke a beneficiary's schedule. With a grace period configured,
        /// the schedule becomes pending and vesting continues until
        /// [`Self::finalize_revocation`]; otherwise it is revoked immediately
        /// and the unvested remainder is returned to the admin.
        ///
        /// Returns the amount returned to the admin (0 when pending).
        ///
        /// # Errors
        /// - [`VestingError::ScheduleIrrevocable`] if the schedule is irrevocable.
        /// - [`VestingError::AlreadyRevoked`] if already revoked or pending.
        pub fn revoke(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            get_admin(&env)?.require_auth();
            let mut schedule = load_schedule(&env, &beneficiary)?;
            if !schedule.is_revocable {
                return Err(VestingError::ScheduleIrrevocable);
            }
            if schedule.revoked || schedule.revocation_pending {
                return Err(VestingError::AlreadyRevoked);
            }

            let now = env.ledger().sequence();
            schedule.revocation_ledger = now;
            if revocation_delay(&env) == 0 {
                Self::apply_revocation(&env, &beneficiary, schedule, now)
            } else {
                schedule.revocation_pending = true;
                save_schedule(&env, &beneficiary, &schedule);
                events::revoked(&env, &beneficiary, now, 0);
                Ok(0)
            }
        }

        /// Finalize a pending revocation once the grace period has elapsed.
        /// Callable by anyone. Returns the amount returned to the admin.
        ///
        /// # Errors
        /// - [`VestingError::AlreadyRevoked`] if no revocation is pending.
        /// - [`VestingError::RevocationNotDue`] if the grace period has not elapsed.
        pub fn finalize_revocation(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            let mut schedule = load_schedule(&env, &beneficiary)?;
            if schedule.revoked || !schedule.revocation_pending {
                return Err(VestingError::AlreadyRevoked);
            }
            let finalize = schedule
                .revocation_ledger
                .saturating_add(revocation_delay(&env));
            if env.ledger().sequence() < finalize {
                return Err(VestingError::RevocationNotDue);
            }
            schedule.revocation_pending = false;
            Self::apply_revocation(&env, &beneficiary, schedule, finalize)
        }

        /// Admin emergency release of all remaining tokens to the beneficiary.
        ///
        /// # Errors
        /// - [`VestingError::AlreadyRevoked`] if the schedule was revoked.
        /// - [`VestingError::NothingToClaim`] if nothing remains.
        pub fn admin_release(env: Env, beneficiary: Address) -> Result<i128, VestingError> {
            get_admin(&env)?.require_auth();
            let token = get_token(&env)?;
            let mut schedule = load_schedule(&env, &beneficiary)?;
            if schedule.revoked {
                return Err(VestingError::AlreadyRevoked);
            }
            let remaining = schedule.amount - schedule.claimed;
            if remaining <= 0 {
                return Err(VestingError::NothingToClaim);
            }
            schedule.claimed = schedule.amount;
            save_schedule(&env, &beneficiary, &schedule);

            let released: i128 = env
                .storage()
                .instance()
                .get(&DataKey::AdminReleased)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::AdminReleased, &(released + remaining));

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &beneficiary,
                &remaining,
            );
            events::admin_released(&env, &beneficiary, remaining);
            Ok(remaining)
        }

        // ── Beneficiary key rotation (#1344) ────────────────────────────────

        /// Propose moving `current`'s schedule to `new_beneficiary`.
        ///
        /// `caller` must be either the current beneficiary or the admin and
        /// must authorize the call. The rotation becomes executable after the
        /// rotation timelock. A pending admin proposal can only be replaced by
        /// the admin; while it is pending, claims by `current` are frozen.
        ///
        /// # Errors
        /// - [`VestingError::NotAuthorized`] if `caller` is neither beneficiary nor admin.
        /// - [`VestingError::ScheduleNotFound`] if `current` has no schedule.
        /// - [`VestingError::InvalidBeneficiary`] if `new_beneficiary == current`.
        /// - [`VestingError::BeneficiaryAlreadyExists`] if `new_beneficiary` has a schedule.
        /// - [`VestingError::RotationPending`] if a beneficiary tries to replace an
        ///   admin-proposed rotation.
        pub fn propose_beneficiary_rotation(
            env: Env,
            caller: Address,
            current: Address,
            new_beneficiary: Address,
        ) -> Result<u32, VestingError> {
            let admin = get_admin(&env)?;
            if caller != current && caller != admin {
                return Err(VestingError::NotAuthorized);
            }
            caller.require_auth();

            load_schedule(&env, &current)?;
            if new_beneficiary == current {
                return Err(VestingError::InvalidBeneficiary);
            }
            if env
                .storage()
                .persistent()
                .has(&DataKey::Schedule(new_beneficiary.clone()))
            {
                return Err(VestingError::BeneficiaryAlreadyExists);
            }
            if let Some(existing) = Self::get_pending_rotation(env.clone(), current.clone()) {
                if existing.proposer == admin && caller != admin {
                    return Err(VestingError::RotationPending);
                }
            }

            let eta_ledger = env
                .ledger()
                .sequence()
                .checked_add(rotation_delay(&env))
                .ok_or(VestingError::ArithmeticError)?;
            let key = DataKey::PendingRotation(current.clone());
            env.storage().persistent().set(
                &key,
                &PendingRotation {
                    new_beneficiary: new_beneficiary.clone(),
                    proposer: caller.clone(),
                    eta_ledger,
                },
            );
            bump(&env);
            bump_key(&env, &key);

            events::rotation_proposed(&env, &current, &new_beneficiary, &caller, eta_ledger);
            Ok(eta_ledger)
        }

        /// Execute a pending rotation once its timelock has elapsed. The new
        /// beneficiary must authorize, proving control of the new key.
        ///
        /// # Errors
        /// - [`VestingError::RotationNotFound`] if nothing is pending for `current`.
        /// - [`VestingError::RotationTimelockActive`] if the timelock has not elapsed.
        /// - [`VestingError::BeneficiaryAlreadyExists`] if the new address gained a schedule.
        pub fn accept_beneficiary_rotation(env: Env, current: Address) -> Result<(), VestingError> {
            let pending = Self::get_pending_rotation(env.clone(), current.clone())
                .ok_or(VestingError::RotationNotFound)?;
            pending.new_beneficiary.require_auth();

            if env.ledger().sequence() < pending.eta_ledger {
                return Err(VestingError::RotationTimelockActive);
            }
            let new_key = DataKey::Schedule(pending.new_beneficiary.clone());
            if env.storage().persistent().has(&new_key) {
                return Err(VestingError::BeneficiaryAlreadyExists);
            }

            let schedule = load_schedule(&env, &current)?;
            env.storage()
                .persistent()
                .remove(&DataKey::Schedule(current.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::PendingRotation(current.clone()));
            save_schedule(&env, &pending.new_beneficiary, &schedule);

            events::rotation_executed(&env, &current, &pending.new_beneficiary);
            Ok(())
        }

        /// Cancel a pending rotation. The admin may cancel any rotation; the
        /// beneficiary may only cancel a rotation they proposed themselves.
        ///
        /// # Errors
        /// - [`VestingError::RotationNotFound`] if nothing is pending.
        /// - [`VestingError::NotAuthorized`] if `caller` may not cancel it.
        pub fn cancel_beneficiary_rotation(
            env: Env,
            caller: Address,
            current: Address,
        ) -> Result<(), VestingError> {
            let admin = get_admin(&env)?;
            let pending = Self::get_pending_rotation(env.clone(), current.clone())
                .ok_or(VestingError::RotationNotFound)?;
            let allowed = caller == admin || (caller == current && pending.proposer == current);
            if !allowed {
                return Err(VestingError::NotAuthorized);
            }
            caller.require_auth();

            env.storage()
                .persistent()
                .remove(&DataKey::PendingRotation(current.clone()));
            bump(&env);
            events::rotation_cancelled(&env, &current, &caller);
            Ok(())
        }

        /// Return the pending rotation for `current`, if any.
        pub fn get_pending_rotation(env: Env, current: Address) -> Option<PendingRotation> {
            env.storage()
                .persistent()
                .get(&DataKey::PendingRotation(current))
        }

        // ── Views ───────────────────────────────────────────────────────────

        /// Return the schedule snapshot for `beneficiary`, if any.
        pub fn get_info(env: Env, beneficiary: Address) -> Option<VestingInfo> {
            let s = load_schedule(&env, &beneficiary).ok()?;
            Some(VestingInfo {
                cliff_ledger: s.cliff_ledger,
                end_ledger: s.end_ledger,
                amount: s.amount,
                claimed: s.claimed,
                revoked: s.revoked,
                is_revocable: s.is_revocable,
                revocation_pending: s.revocation_pending,
                tranches: s.tranches,
            })
        }

        /// Amount `beneficiary` could claim right now (ignoring the dust threshold).
        pub fn claimable(env: Env, beneficiary: Address) -> i128 {
            load_schedule(&env, &beneficiary)
                .and_then(|s| Ok(total_vested(&env, &s, env.ledger().sequence())? - s.claimed))
                .map_or(0, |c| c.max(0))
        }
    }

    impl VestingContract {
        fn store_new_schedule(
            env: &Env,
            beneficiary: &Address,
            cliff_ledger: u32,
            end_ledger: u32,
            amount: i128,
            is_revocable: bool,
            tranches: Vec<(u32, u32)>,
        ) -> Result<(), VestingError> {
            let admin = get_admin(env)?;
            let token = get_token(env)?;
            admin.require_auth();

            if amount <= 0 {
                return Err(VestingError::InvalidAmount);
            }
            if env
                .storage()
                .persistent()
                .has(&DataKey::Schedule(beneficiary.clone()))
            {
                return Err(VestingError::ScheduleAlreadyExists);
            }

            let schedule = BeneficiarySchedule {
                amount,
                cliff_ledger,
                end_ledger,
                claimed: 0,
                revoked: false,
                is_revocable,
                tranches,
                milestone_oracle: None,
                milestone_released: false,
                revocation_pending: false,
                revocation_ledger: 0,
            };
            save_schedule(env, beneficiary, &schedule);

            token::Client::new(env, &token).transfer(
                &admin,
                &env.current_contract_address(),
                &amount,
            );
            events::initialized(env, beneficiary, amount, cliff_ledger, end_ledger);
            Ok(())
        }

        /// Mark `schedule` revoked with vesting capped at `cap_ledger`, and
        /// return the unvested remainder to the admin.
        fn apply_revocation(
            env: &Env,
            beneficiary: &Address,
            mut schedule: BeneficiarySchedule,
            cap_ledger: u32,
        ) -> Result<i128, VestingError> {
            schedule.revoked = true;
            schedule.revocation_ledger = cap_ledger;
            let vested = total_vested(env, &schedule, cap_ledger)?;
            let unvested = schedule.amount - vested;
            save_schedule(env, beneficiary, &schedule);

            if unvested > 0 {
                token::Client::new(env, &get_token(env)?).transfer(
                    &env.current_contract_address(),
                    &get_admin(env)?,
                    &unvested,
                );
            }
            events::revoked(env, beneficiary, cap_ledger, unvested);
            Ok(unvested)
        }
    }
}
