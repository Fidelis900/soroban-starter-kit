//! Multi-stage, percentage-based milestone release with a per-milestone
//! buyer/seller dispute window (#1437).
//!
//! The escrowed total is split into stages by basis-point share. For each
//! stage:
//!
//! 1. The seller calls [`submit_milestone`], which opens that stage's dispute
//!    window (`dispute_window_ledgers`).
//! 2. The buyer may [`approve_milestone`] at any time to release it early, or
//!    [`dispute_milestone`] while the window is still open to escalate it to
//!    the arbiter.
//! 3. If the window elapses without a dispute, the seller may
//!    [`claim_milestone`] — the stage is released automatically.
//! 4. A disputed stage is settled by the arbiter via
//!    [`resolve_milestone_dispute`], either to the seller or back to the buyer.
//!
//! Each settlement pays out only that stage's allocation and decrements the
//! stored remaining [`DataKey::Amount`], so whole-escrow flows (refunds,
//! escrow-level disputes) only ever touch the unreleased remainder. Once every
//! stage is settled the escrow transitions to `Completed` (or `Refunded` if
//! every stage was refunded).

use soroban_sdk::{Address, Env, Vec, token};

use crate::admin;
use crate::errors::EscrowError;
use crate::events;
use crate::lifecycle::{
    apply_fee, emit_init_events, extend_ttl, get_required, maybe_pay_fee, store_escrow_data,
    validate_parties,
};
use crate::storage::{
    DataKey, EscrowState, MilestoneStage, MilestoneStatus, StagedMilestone, acquire_lock,
    release_lock, require_state,
};
use soroban_common::validate_deadline;

/// Basis-point denominator: all stage shares must sum to exactly this value.
const FULL_BPS: u32 = 10_000;

#[cfg(feature = "pausable")]
fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    crate::EscrowContract::require_not_paused(env)
}

#[cfg(not(feature = "pausable"))]
fn require_not_paused(_env: &Env) -> Result<(), EscrowError> {
    Ok(())
}

/// Split `total` across `stages` by basis-point share.
///
/// The final stage absorbs the integer-division remainder so that the stage
/// amounts always sum to exactly `total`.
fn build_schedule(
    env: &Env,
    total: i128,
    stages: &Vec<MilestoneStage>,
) -> Result<Vec<StagedMilestone>, EscrowError> {
    if stages.is_empty() {
        return Err(EscrowError::InvalidMilestoneSchedule);
    }

    let mut bps_sum: u32 = 0;
    for stage in stages.iter() {
        if stage.percentage_bps == 0 || stage.dispute_window_ledgers == 0 {
            return Err(EscrowError::InvalidMilestoneSchedule);
        }
        bps_sum = bps_sum
            .checked_add(stage.percentage_bps)
            .ok_or(EscrowError::InvalidMilestoneSchedule)?;
    }
    if bps_sum != FULL_BPS {
        return Err(EscrowError::InvalidMilestoneSchedule);
    }

    let last = stages.len().saturating_sub(1);
    let mut allocated: i128 = 0;
    let mut schedule = Vec::new(env);
    for (i, stage) in stages.iter().enumerate() {
        let amount = if u32::try_from(i).unwrap_or(u32::MAX) == last {
            total
                .checked_sub(allocated)
                .ok_or(EscrowError::InvalidMilestoneSchedule)?
        } else {
            total
                .checked_mul(i128::from(stage.percentage_bps))
                .and_then(|v| v.checked_div(i128::from(FULL_BPS)))
                .ok_or(EscrowError::InvalidMilestoneSchedule)?
        };
        // A stage too small to receive any funds is a configuration error.
        if amount <= 0 {
            return Err(EscrowError::InvalidMilestoneSchedule);
        }
        allocated = allocated
            .checked_add(amount)
            .ok_or(EscrowError::InvalidMilestoneSchedule)?;
        schedule.push_back(StagedMilestone {
            percentage_bps: stage.percentage_bps,
            amount,
            dispute_window_ledgers: stage.dispute_window_ledgers,
            status: MilestoneStatus::Pending,
            submitted_at: 0,
        });
    }
    Ok(schedule)
}

fn load_schedule(env: &Env) -> Result<Vec<StagedMilestone>, EscrowError> {
    env.storage()
        .instance()
        .get(&DataKey::StagedMilestones)
        .ok_or(EscrowError::MilestoneNotFound)
}

fn load_stage(
    schedule: &Vec<StagedMilestone>,
    index: u32,
) -> Result<StagedMilestone, EscrowError> {
    schedule.get(index).ok_or(EscrowError::MilestoneNotFound)
}

/// Last ledger (inclusive) on which a submitted stage may still be disputed.
fn dispute_deadline(stage: &StagedMilestone) -> u32 {
    stage
        .submitted_at
        .saturating_add(stage.dispute_window_ledgers)
}

/// Settle one stage, paying its allocation to the seller (`to_seller`) or back
/// to the buyer.
///
/// Checks–Effects–Interactions: the stage status, the remaining escrow amount
/// and (when this is the final stage) the escrow state are all committed
/// before any token transfer, which runs under the reentrancy lock.
fn settle_stage(
    env: &Env,
    mut schedule: Vec<StagedMilestone>,
    index: u32,
    to_seller: bool,
) -> Result<(), EscrowError> {
    let mut stage = load_stage(&schedule, index)?;
    let amount = stage.amount;
    stage.status = if to_seller {
        MilestoneStatus::Released
    } else {
        MilestoneStatus::Refunded
    };
    schedule.set(index, stage);
    env.storage()
        .instance()
        .set(&DataKey::StagedMilestones, &schedule);

    let remaining: i128 = get_required(env, &DataKey::Amount)?;
    let new_remaining = remaining
        .checked_sub(amount)
        .filter(|v| *v >= 0)
        .ok_or(EscrowError::InsufficientFunds)?;
    env.storage()
        .instance()
        .set(&DataKey::Amount, &new_remaining);

    let all_settled = schedule.iter().all(|m| m.is_settled());
    if all_settled {
        let all_refunded = schedule
            .iter()
            .all(|m| m.status == MilestoneStatus::Refunded);
        let final_state = if all_refunded {
            EscrowState::Refunded
        } else {
            EscrowState::Completed
        };
        env.storage().instance().set(&DataKey::State, &final_state);
    }
    extend_ttl(env);

    let this = env.current_contract_address();
    if to_seller {
        let seller: Address = get_required(env, &DataKey::Seller)?;
        let (net, fee) = apply_fee(env, amount);
        acquire_lock(env)?;
        admin::transfer_token(env, &this, &seller, net);
        maybe_pay_fee(env, fee);
        release_lock(env);
        events::milestone_released(env, &seller, index, net, fee);
    } else {
        let buyer: Address = get_required(env, &DataKey::Buyer)?;
        acquire_lock(env)?;
        admin::transfer_token(env, &this, &buyer, amount);
        release_lock(env);
        events::milestone_refunded(env, &buyer, index, amount);
    }
    Ok(())
}

/// Initialize an escrow whose `total_amount` is released in fractional stages.
///
/// See [`crate::EscrowContract::initialize_with_milestone_schedule`].
#[allow(clippy::too_many_arguments)]
pub fn initialize_with_milestone_schedule(
    env: Env,
    admin: Address,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    token_contract: Address,
    total_amount: i128,
    stages: Vec<MilestoneStage>,
    deadline_ledger: u32,
    metadata_hash: Option<soroban_sdk::BytesN<32>>,
) -> Result<(), EscrowError> {
    if env.storage().instance().has(&DataKey::State) {
        return Err(EscrowError::AlreadyInitialized);
    }
    if total_amount <= 0 {
        return Err(EscrowError::InvalidAmount);
    }
    let schedule = build_schedule(&env, total_amount, &stages)?;
    validate_parties(&buyer, &seller, &arbiter)?;
    validate_deadline::<EscrowError>(&env, deadline_ledger)?;
    // Fail fast if `token_contract` is not a token.
    token::Client::new(&env, &token_contract).decimals();

    env.storage()
        .instance()
        .set(&soroban_common::AdminKey::Admin, &admin);
    store_escrow_data(
        &env,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        total_amount,
        deadline_ledger,
        1u32,
        0u32,
        0u32,
    );
    env.storage()
        .instance()
        .set(&DataKey::StagedMilestones, &schedule);
    if let Some(hash) = metadata_hash {
        env.storage().instance().set(&DataKey::MetadataHash, &hash);
    }
    extend_ttl(&env);
    emit_init_events(&env, &buyer, &seller, &arbiter, total_amount);
    Ok(())
}

/// Seller marks a stage as delivered, opening its dispute window.
pub fn submit_milestone(env: Env, index: u32) -> Result<(), EscrowError> {
    require_not_paused(&env)?;
    let seller: Address = get_required(&env, &DataKey::Seller)?;
    seller.require_auth();
    require_state(&env, EscrowState::Funded)?;

    let mut schedule = load_schedule(&env)?;
    let mut stage = load_stage(&schedule, index)?;
    if stage.status != MilestoneStatus::Pending {
        return Err(EscrowError::InvalidState);
    }
    stage.status = MilestoneStatus::Submitted;
    stage.submitted_at = env.ledger().sequence();
    let deadline = dispute_deadline(&stage);
    schedule.set(index, stage);
    env.storage()
        .instance()
        .set(&DataKey::StagedMilestones, &schedule);
    extend_ttl(&env);

    events::milestone_submitted(&env, &seller, index, deadline);
    Ok(())
}

/// Buyer approves a pending or submitted stage, releasing it to the seller.
pub fn approve_milestone(env: Env, index: u32) -> Result<(), EscrowError> {
    require_not_paused(&env)?;
    let buyer: Address = get_required(&env, &DataKey::Buyer)?;
    buyer.require_auth();
    require_state(&env, EscrowState::Funded)?;

    let schedule = load_schedule(&env)?;
    let stage = load_stage(&schedule, index)?;
    if !matches!(
        stage.status,
        MilestoneStatus::Pending | MilestoneStatus::Submitted
    ) {
        return Err(EscrowError::InvalidState);
    }
    settle_stage(&env, schedule, index, true)
}

/// Buyer escalates a submitted stage to the arbiter. Only allowed while the
/// stage's dispute window is open (up to and including the deadline ledger).
pub fn dispute_milestone(env: Env, index: u32) -> Result<(), EscrowError> {
    require_not_paused(&env)?;
    let buyer: Address = get_required(&env, &DataKey::Buyer)?;
    buyer.require_auth();
    require_state(&env, EscrowState::Funded)?;

    let mut schedule = load_schedule(&env)?;
    let mut stage = load_stage(&schedule, index)?;
    if stage.status != MilestoneStatus::Submitted {
        return Err(EscrowError::InvalidState);
    }
    if env.ledger().sequence() > dispute_deadline(&stage) {
        return Err(EscrowError::DisputeWindowClosed);
    }
    stage.status = MilestoneStatus::Disputed;
    let amount = stage.amount;
    schedule.set(index, stage);
    env.storage()
        .instance()
        .set(&DataKey::StagedMilestones, &schedule);
    extend_ttl(&env);

    events::milestone_disputed(&env, &buyer, index, amount);
    Ok(())
}

/// Seller claims a submitted stage once its dispute window has elapsed
/// without a dispute — the automatic timeout release.
pub fn claim_milestone(env: Env, index: u32) -> Result<(), EscrowError> {
    require_not_paused(&env)?;
    let seller: Address = get_required(&env, &DataKey::Seller)?;
    seller.require_auth();
    require_state(&env, EscrowState::Funded)?;

    let schedule = load_schedule(&env)?;
    let stage = load_stage(&schedule, index)?;
    if stage.status != MilestoneStatus::Submitted {
        return Err(EscrowError::InvalidState);
    }
    if env.ledger().sequence() <= dispute_deadline(&stage) {
        return Err(EscrowError::DisputeWindowOpen);
    }
    settle_stage(&env, schedule, index, true)
}

/// Arbiter settles a disputed stage, either releasing it to the seller or
/// refunding it to the buyer.
pub fn resolve_milestone_dispute(
    env: Env,
    caller: Address,
    index: u32,
    release_to_seller: bool,
) -> Result<(), EscrowError> {
    require_not_paused(&env)?;
    let arbiter: Address = get_required(&env, &DataKey::Arbiter)?;
    if caller != arbiter {
        return Err(EscrowError::NotAuthorized);
    }
    caller.require_auth();
    require_state(&env, EscrowState::Funded)?;

    let schedule = load_schedule(&env)?;
    let stage = load_stage(&schedule, index)?;
    if stage.status != MilestoneStatus::Disputed {
        return Err(EscrowError::InvalidState);
    }
    settle_stage(&env, schedule, index, release_to_seller)?;
    events::dispute_resolved(&env, &caller, release_to_seller);
    Ok(())
}

/// Return the staged milestone schedule, or an empty list if none.
pub fn get_staged_milestones(env: Env) -> Vec<StagedMilestone> {
    env.storage()
        .instance()
        .get(&DataKey::StagedMilestones)
        .unwrap_or_else(|| Vec::new(&env))
}
