use soroban_sdk::{Address, Env};

use crate::errors::EscrowError;
use crate::events;
use crate::lifecycle::{extend_ttl, get_required, refund_to_buyer, release_to_seller};
use crate::storage::{DataKey, EscrowState};

use DataKey::*;

const MAX_ARBITER_FEE_BPS: u32 = 2_000;
const BPS_DENOMINATOR: u32 = 10_000;

/// Compute the arbiter fee owed on `amount` given the configured fee (in bps).
fn compute_arbiter_fee(env: &Env, amount: i128) -> i128 {
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ArbiterFeeBps)
        .unwrap_or(0);
    if fee_bps == 0 || amount <= 0 {
        return 0;
    }
    let fee = amount
        .saturating_mul(i128::from(fee_bps))
        / i128::from(BPS_DENOMINATOR);
    fee.min(amount)
}

/// Deduct the arbiter fee from the disputed funds and pay the arbiter.
/// Returns the remaining amount to be split between buyer and seller.
fn pay_arbiter_fee(env: &Env, arbiter: &Address, disputed: i128) -> i128 {
    let fee = compute_arbiter_fee(env, disputed);
    if fee > 0 {
        crate::lifecycle::transfer_funds(env, arbiter, fee);
        events::arbiter_fee_paid(env, arbiter, fee);
    }
    disputed.saturating_sub(fee)
}

/// Resolve the dispute by releasing the remaining (post-fee) funds to the
/// seller or refunding them to the buyer.
fn settle(
    env: Env,
    arbiter: &Address,
    release_to_seller_flag: bool,
) -> Result<(), EscrowError> {
    let disputed: i128 = get_required(&env, &Amount)?;
    let remaining = pay_arbiter_fee(&env, arbiter, disputed);

    if release_to_seller_flag {
        env.storage()
            .instance()
            .set(&State, &EscrowState::Delivered);
        release_to_seller_amount(env, remaining)
    } else {
        env.storage().instance().set(&State, &EscrowState::Funded);
        refund_to_buyer_amount(env, remaining)
    }
}

fn release_to_seller_amount(env: Env, amount: i128) -> Result<(), EscrowError> {
    crate::lifecycle::release_to_seller_amount(env, amount)
}

fn refund_to_buyer_amount(env: Env, amount: i128) -> Result<(), EscrowError> {
    crate::lifecycle::refund_to_buyer_amount(env, amount)
}

pub fn raise_dispute(env: Env, caller: Address) -> Result<(), EscrowError> {
    #[cfg(feature = "pausable")]
    crate::EscrowContract::require_not_paused(&env)?;

    let buyer: Address = get_required(&env, &Buyer)?;
    let seller: Address = get_required(&env, &Seller)?;

    if caller != buyer && caller != seller {
        return Err(EscrowError::NotAuthorized);
    }
    caller.require_auth();

    let state: EscrowState = get_required(&env, &State)?;
    if !matches!(state, EscrowState::Funded | EscrowState::Delivered) {
        return Err(EscrowError::InvalidState);
    }

    env.storage().instance().set(&State, &EscrowState::Disputed);
    // Record when the dispute was raised for timeout tracking (issue #709)
    env.storage()
        .instance()
        .set(&DisputeRaisedAt, &env.ledger().sequence());
    extend_ttl(&env);

    events::dispute_raised(&env, &caller);

    Ok(())
}

pub fn claim_dispute_timeout(env: Env) -> Result<(), EscrowError> {
    #[cfg(feature = "pausable")]
    crate::EscrowContract::require_not_paused(&env)?;

    let buyer: Address = get_required(&env, &Buyer)?;
    buyer.require_auth();

    let state: EscrowState = get_required(&env, &State)?;
    if state != EscrowState::Disputed {
        return Err(EscrowError::InvalidState);
    }

    let timeout: u32 = env
        .storage()
        .instance()
        .get(&DisputeTimeoutLedgers)
        .unwrap_or(0);
    if timeout == 0 {
        return Err(EscrowError::NoDisputeTimeout);
    }

    let raised_at: u32 = get_required(&env, &DisputeRaisedAt)?;
    let elapsed = env.ledger().sequence().saturating_sub(raised_at);
    if elapsed < timeout {
        return Err(EscrowError::DisputeTimeoutNotReached);
    }

    // Auto-resolve in buyer's favour
    env.storage().instance().set(&State, &EscrowState::Funded);
    env.storage().instance().remove(&DisputeRaisedAt);
    refund_to_buyer(env)
}

pub fn resolve_dispute(
    env: Env,
    caller: Address,
    release_to_seller_flag: bool,
) -> Result<(), EscrowError> {
    let state: EscrowState = get_required(&env, &State)?;
    if state != EscrowState::Disputed {
        return Err(EscrowError::InvalidState);
    }

    let arbiters_opt: Option<soroban_sdk::Vec<Address>> =
        env.storage().instance().get(&DataKey::Arbiters);

    if let Some(arbiters) = arbiters_opt {
        let required_sigs: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RequiredSignatures)
            .unwrap_or(1);
        resolve_multisig(env, caller, arbiters, required_sigs, release_to_seller_flag)
    } else {
        resolve_single(env, caller, release_to_seller_flag)
    }
}

/// Record an arbiter's approval for a dispute outcome. Once the number of
/// distinct approvals reaches the configured threshold, the dispute is
/// resolved (including arbiter fee deduction).
pub fn approve_dispute(
    env: Env,
    arbiter: Address,
    release_to_seller: bool,
) -> Result<(), EscrowError> {
    #[cfg(feature = "pausable")]
    crate::EscrowContract::require_not_paused(&env)?;

    let state: EscrowState = get_required(&env, &State)?;
    if state != EscrowState::Disputed {
        return Err(EscrowError::InvalidState);
    }

    let arbiters: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Arbiters)
        .ok_or(EscrowError::NotInitialized)?;

    if !arbiters.iter().any(|a| a == arbiter) {
        return Err(EscrowError::NotAuthorized);
    }

    arbiter.require_auth();

    let required_sigs: u32 = env
        .storage()
        .instance()
        .get(&DataKey::RequiredSignatures)
        .unwrap_or(1);

    let mut approvals: soroban_sdk::Vec<(Address, bool)> = env
        .storage()
        .instance()
        .get(&DataKey::DisputeApprovals)
        .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

    // Reject duplicate or conflicting approvals from the same arbiter.
    for (existing, direction) in approvals.iter() {
        if existing == arbiter {
            if direction == release_to_seller {
                // Duplicate approval for the same direction: no-op.
                return Ok(());
            }
            return Err(EscrowError::NotAuthorized);
        }
    }

    approvals.push_back((arbiter.clone(), release_to_seller));
    env.storage()
        .instance()
        .set(&DataKey::DisputeApprovals, &approvals);
    extend_ttl(&env);

    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let approval_count = approvals.len() as u32;
    if approval_count < required_sigs {
        return Ok(());
    }

    // Threshold reached: verify all approvals agree on a direction.
    let mut direction: Option<bool> = None;
    for (_, d) in approvals.iter() {
        match direction {
            None => direction = Some(d),
            Some(prev) if prev != d => return Err(EscrowError::NotAuthorized),
            _ => {}
        }
    }
    let release = direction.unwrap_or(false);

    env.storage().instance().remove(&DataKey::DisputeApprovals);
    env.storage()
        .instance()
        .remove(&DataKey::ArbiterVotesRelease);
    env.storage()
        .instance()
        .remove(&DataKey::ArbiterVotesRefund);

    settle(env, &arbiter, release)
}

fn resolve_single(
    env: Env,
    caller: Address,
    release_to_seller_flag: bool,
) -> Result<(), EscrowError> {
    let arbiter: Address = env
        .storage()
        .instance()
        .get(&Arbiter)
        .ok_or(EscrowError::NotInitialized)?;

    if caller != arbiter {
        return Err(EscrowError::NotAuthorized);
    }
    arbiter.require_auth();

    settle(env, &arbiter, release_to_seller_flag)
}

fn resolve_multisig(
    env: Env,
    caller: Address,
    arbiters: soroban_sdk::Vec<Address>,
    required_sigs: u32,
    release_to_seller_flag: bool,
) -> Result<(), EscrowError> {
    if !arbiters.iter().any(|a| a == caller) {
        return Err(EscrowError::NotAuthorized);
    }
    caller.require_auth();

    // Resolution is gated on threshold approvals collected via approve_dispute.
    let approvals: soroban_sdk::Vec<(Address, bool)> = env
        .storage()
        .instance()
        .get(&DataKey::DisputeApprovals)
        .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let approval_count = approvals.len() as u32;
    if approval_count < required_sigs {
        return Err(EscrowError::NotAuthorized);
    }

    for (_, direction) in approvals.iter() {
        if direction != release_to_seller_flag {
            return Err(EscrowError::NotAuthorized);
        }
    }

    env.storage().instance().remove(&DataKey::DisputeApprovals);
    env.storage()
        .instance()
        .remove(&DataKey::ArbiterVotesRelease);
    env.storage()
        .instance()
        .remove(&DataKey::ArbiterVotesRefund);

    settle(env, &caller, release_to_seller_flag)
}
