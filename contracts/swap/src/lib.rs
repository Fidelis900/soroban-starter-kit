#![no_std]
#![deny(missing_docs)]
//! Atomic and basket token swaps with optional escrow and partial fills.

#[cfg(test)]
extern crate std;

//! Atomic two-party token swap contract template.
//!
//! Party A escrows tokens when proposing a swap. Party B supplies the matching
//! tokens on acceptance; cancellation returns the escrow to Party A.

use soroban_sdk::{Address, BytesN, Env, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::SwapError;
pub use storage::{BasketLeg, BasketSwapInfo, DataKey, SwapInfo, SwapKey, SwapState};

use soroban_common::math::{FixedPoint, FixedPointError, mul_div};
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};
use storage::DataKey::{Admin, BasketSwapCount, FeeBps, SwapCount, Treasury};
pub use storage::{DataKey, SwapInfo, SwapState};
pub use storage::{DataKey, SwapInfo, SwapKey, SwapPage, SwapState};

use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee, paginate,
};

/// Maximum number of swaps a single [`SwapContract::get_active_swaps`] call
/// may return, regardless of the requested `limit`.
pub const MAX_SWAPS_PAGE_SIZE: u32 = 50;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_swap(env: &Env, id: u32) {
    env.storage().persistent().extend_ttl(
        &DataKey::Swap(id),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

fn get_instance<V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Result<V, SwapError> {
    env.storage()
        .instance()
        .get(key)
        .ok_or(SwapError::NotInitialized)
}

fn ensure_valid_leg_amount(amount: i128) -> Result<(), SwapError> {
    if amount <= 0 {
        return Err(SwapError::InvalidAmount);
    }
    Ok(())
}

fn map_state_err(state: SwapState) -> SwapError {
    match state {
        SwapState::Executed => SwapError::AlreadyCompleted,
        SwapState::Cancelled => SwapError::AlreadyCancelled,
        SwapState::Pending => SwapError::InvalidState,
    }
}

fn require_pending(state: SwapState) -> Result<(), SwapError> {
    if state != SwapState::Pending {
        return Err(map_state_err(state));
    }
    Ok(())
}

/// Atomic swap contract.
/// Atomic two-party token swap.
/// Computes the protocol treasury fee and validates anti-fee-evasion constraints.
///
/// If `fee_bps > 0`, fee is computed via [`apply_bps_fee`]. If integer division truncates
/// the computed fee to 0 (which occurs on micro-transactions), a minimum fee policy is enforced:
/// the fee is set to `1` unit, provided `amount_b >= 1`.
/// The fee is strictly bounded such that `fee <= amount_b`.
pub fn calculate_and_validate_fee(amount_b: i128, fee_bps: u32) -> Result<i128, SwapError> {
    if fee_bps == 0 {
        return Ok(0);
    }
    let raw_fee = FixedPoint::apply_bps_fee(amount_b, fee_bps)
        .map_err(|_| SwapError::MathOverflow)?;
    let fee = if raw_fee == 0 && amount_b > 0 {
        1i128
    } else {
        raw_fee
    };

    if fee > amount_b {
        return Err(SwapError::InvalidAmount);
    }

    Ok(fee)
}

/// Atomic two-party token swap contract.
pub use contract::*;

mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct SwapContract;

    #[contractimpl]
    impl SwapContract {
        pub fn initialize(env: Env, admin: Address, treasury: Address, fee_bps: u32) -> Result<(), SwapError> {
            if env.storage().instance().has(&DataKey::Initialized) {
        /// Initialize the swap contract.
        pub fn initialize(
            env: Env,
            admin: Address,
            treasury: Address,
            fee_bps: u32,
        ) -> Result<(), SwapError> {
            if env.storage().instance().has(&DataKey::Initialized) {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(SwapError::AlreadyInitialized);
            }
            if fee_bps > 10_000 {
                return Err(SwapError::InvalidFee);
            }
            admin.require_auth();

            env.storage().instance().set(&DataKey::Initialized, &true);
            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Treasury, &treasury);
            env.storage().instance().set(&FeeBps, &fee_bps);
            env.storage().instance().set(&SwapCount, &0u32);
            env.storage().instance().set(&BasketSwapCount, &0u32);
            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Treasury, &treasury);
            env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
            env.storage().instance().set(&DataKey::SwapCount, &0u32);
            env.storage().instance().set(&DataKey::Initialized, &true);

            extend_ttl_instance(&env);
            bump_instance(&env);
            Ok(())
        }

        pub fn set_treasury(env: Env, new_treasury: Address) -> Result<(), SwapError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();
            env.storage().instance().set(&Treasury, &new_treasury);
            bump_instance(&env);
            Ok(())
        }

        /// Set the fee recipient. Only the administrator may call this.
        /// Update the treasury address. Only admin can call.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`] if the contract is not initialized.
        /// - [`SwapError::NotAuthorized`] if caller is not the admin.
        pub fn set_treasury(env: Env, new_treasury: Address) -> Result<(), SwapError> {
            let admin: Address = get_instance(&env, &DataKey::Admin)?;
            admin.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::Treasury, &new_treasury);
            bump_instance(&env);
            Ok(())
        }

        /// Set the fee in basis points. Only the administrator may call this.
        pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), SwapError> {
            let admin: Address = get_instance(&env, &DataKey::Admin)?;
            extend_ttl_instance(&env);

            Ok(())
        }

        /// Update the fee basis points. Only admin can call this.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`] if the contract is not initialized.
        /// - [`SwapError::NotAuthorized`] if caller is not the admin.
        /// - [`SwapError::InvalidFee`] if `new_fee_bps` > 10000 (100%).
        pub fn set_fee_bps(env: Env, new_fee_bps: u32) -> Result<(), SwapError> {
            let admin: Address = get_required(&env, &DataKey::Admin)?;
            admin.require_auth();
            if new_fee_bps > 10_000 {
                return Err(SwapError::InvalidFee);
            }
            env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
            bump_instance(&env);
            Ok(())
        }

        pub fn set_admin(env: Env, new_admin: Address) -> Result<(), SwapError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();
            env.storage().instance().set(&Admin, &new_admin);
            bump_instance(&env);
            Ok(())
        }

        pub fn get_admin(env: Env) -> Result<Address, SwapError> {
            get_required(&env, &Admin)
        }

        pub fn get_treasury(env: Env) -> Result<Address, SwapError> {
            get_required(&env, &Treasury)
        }

        /// Set the administrator address. Only the current administrator may call this.
        /// Update the admin address. Only current admin can call this.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`] if the contract is not initialized.
        /// - [`SwapError::NotAuthorized`] if caller is not the current admin.
        pub fn set_admin(env: Env, new_admin: Address) -> Result<(), SwapError> {
            let admin: Address = get_instance(&env, &DataKey::Admin)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Admin, &new_admin);
            bump_instance(&env);
            Ok(())
        }

        /// Get the current admin address.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`] if the contract is not initialized.
        pub fn get_admin(env: Env) -> Result<Address, SwapError> {
            get_instance(&env, &DataKey::Admin)
        }

        /// Return the configured treasury.
        /// Get the current treasury address.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`] if the contract is not initialized.
        pub fn get_treasury(env: Env) -> Result<Address, SwapError> {
            get_instance(&env, &DataKey::Treasury)
        }

        /// Return the configured fee in basis points.
        pub fn get_fee_bps(env: Env) -> Result<u32, SwapError> {
            get_instance(&env, &DataKey::FeeBps)
        }

        /// Return the number of swaps created so far.
        pub fn swap_count(env: Env) -> Result<u32, SwapError> {
            get_instance(&env, &DataKey::SwapCount)
        }

        /// Propose a swap and escrow Party A's asset in persistent storage.
        /// Get the current fee basis points.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        pub fn get_fee_bps(env: Env) -> Result<u32, SwapError> {
            get_required(&env, &DataKey::FeeBps)
        }

        /// Return the total number of swaps created.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        pub fn swap_count(env: Env) -> Result<u32, SwapError> {
            get_required(&env, &DataKey::SwapCount)
        }

        pub fn swap_count(env: Env) -> u32 {
            env.storage().instance().get(&SwapCount).unwrap_or(0)
        }

        pub fn basket_swap_count(env: Env) -> u32 {
            env.storage().instance().get(&BasketSwapCount).unwrap_or(0)
        }

        /// Propose a new swap. Party A proposes a swap of `amount_a` of `token_a`
        /// for `amount_b` of `token_b`. Returns the swap ID.
        ///
        /// The swap is valid until `expires_at` ledger.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        /// - [`SwapError::InvalidAmount`] if amounts <= 0.
        /// - [`SwapError::InvalidDeadline`] if `expires_at` <= current ledger.
        /// Propose a new swap. Party A proposes a swap of `amount_a` of `token_a`
        /// for `amount_b` of `token_b`. Returns the swap ID.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        /// - [`SwapError::InvalidAmount`] if amounts <= 0.
        /// - [`SwapError::InvalidDeadline`] if `expires_at` <= current ledger.
        #[allow(clippy::too_many_arguments)]
        pub fn propose_swap(
            env: Env,
            party_a: Address,
            token_a: Address,
            amount_a: i128,
            token_b: Address,
            amount_b: i128,
            expires_at: u32,
            allowed_counterparty: Option<Address>,
            max_execution_delay: Option<u32>,
        ) -> Result<u32, SwapError> {
            Self::propose_swap_with_options(
                env,
                party_a,
                token_a,
                amount_a,
                token_b,
                amount_b,
                expires_at,
                allowed_counterparty,
                max_execution_delay,
                false,
                false,
                None,
            )
        }

        /// Propose a swap with explicit partial-fill, escrow and HTLC options.
        ///
        /// When `hash_lock` is set the swap can only be executed by revealing a
        /// `secret_preimage` whose SHA-256 digest matches the lock.
        #[allow(clippy::too_many_arguments)]
        pub fn propose_swap_with_options(
            env: Env,
            party_a: Address,
            token_a: Address,
            amount_a: i128,
            token_b: Address,
            amount_b: i128,
            expires_at: u32,
            allowed_counterparty: Option<Address>,
            max_execution_delay: Option<u32>,
            allow_partial_fills: bool,
            escrowed: bool,
            hash_lock: Option<BytesN<32>>,
        ) -> Result<u32, SwapError> {
            get_required::<bool>(&env, &DataKey::Initialized)?;
            party_a.require_auth();
            ensure_valid_leg_amount(amount_a)?;
            ensure_valid_leg_amount(amount_b)?;

            let current_ledger = env.ledger().sequence();
            if expires_at <= current_ledger {
                return Err(SwapError::InvalidDeadline);
            }

            let swap_id: u32 = env
                .storage()
                .instance()
                .get(&DataKey::SwapCount)
                .unwrap_or(0);
            let next_id = swap_id.checked_add(1).ok_or(SwapError::InvalidAmount)?;

            if escrowed {
                token::Client::new(&env, &token_a).transfer(
                    &party_a,
                    &env.current_contract_address(),
                    &amount_a,
                );
            }

            let swap = SwapInfo {
                id: swap_id,
                party_a: party_a.clone(),
                token_a: token_a.clone(),
                amount_a,
                token_b: token_b.clone(),
                amount_b,
                expires_at,
                state: SwapState::Pending,
                filled_amount: 0,
                remaining_amount: amount_a,
                allow_partial_fills,
                escrowed,
                allowed_counterparty: allowed_counterparty.clone(),
                max_execution_delay,
                created_at: current_ledger,
                hash_lock,
            };
            env.storage()
                .persistent()
                .set(&SwapKey::Swap(swap_id), &swap);
            env.storage().instance().set(&DataKey::SwapCount, &next_id);
            extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
            bump_persistent(&env, &SwapKey::Swap(swap_id));
            bump_instance(&env);
            events::swap_proposed(
                &env,
                &party_a,
                swap_id,
                &token_a,
                amount_a,
                &token_b,
                amount_b,
                expires_at,
                &allowed_counterparty,
            );
            Ok(swap_id)
        }

        /// Accept a proposed swap. Party B accepts and the swap executes atomically.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        /// - [`SwapError::SwapNotFound`]
        /// - [`SwapError::InvalidState`] / [`SwapError::AlreadyCompleted`]
        /// - [`SwapError::DeadlineExpired`]
        /// - [`SwapError::NotAuthorized`] if restricted taker does not match
        /// - [`SwapError::ExecutionDelayExceeded`] if max delay window is exceeded
        /// - [`SwapError::HashLockMismatch`] if the revealed preimage does not match
        pub fn accept_swap(
            env: Env,
            taker: Address,
            swap_id: u32,
            fill_amount_a: i128,
        ) -> Result<u32, SwapError> {
            Self::execute_swap(env, taker, swap_id, fill_amount_a, None, None)
        }

        /// Execute a swap, optionally revealing an HTLC preimage and crediting a referrer.
        #[allow(clippy::too_many_arguments)]
        pub fn execute_swap(
            env: Env,
            taker: Address,
            swap_id: u32,
            fill_amount_a: i128,
            secret_preimage: Option<BytesN<32>>,
            referrer: Option<Address>,
        ) -> Result<u32, SwapError> {
            taker.require_auth();
            ensure_valid_leg_amount(fill_amount_a)?;

            let fee_bps: u32 = get_required(&env, &FeeBps)?;
            let treasury: Address = get_required(&env, &Treasury)?;

            let mut swap: SwapInfo = env
                .storage()
                .persistent()
                .get(&SwapKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)?;
            require_pending(swap.state)?;

            let current_ledger = env.ledger().sequence();
            if current_ledger > swap.expires_at {
                return Err(SwapError::DeadlineExpired);
            }

            // Verify allowed counterparty restriction if configured
            if let Some(ref allowed) = swap.allowed_counterparty {
                if taker != *allowed {
                    return Err(SwapError::NotAuthorized);
                }
            }

            // Verify maximum execution delay window check if configured
            if let Some(max_delay) = swap.max_execution_delay {
                let max_allowed_ledger = swap.created_at.saturating_add(max_delay);
                if current_ledger > max_allowed_ledger {
                    return Err(SwapError::ExecutionDelayExceeded);
                }
            }

            // HTLC: verify the revealed preimage against the stored hash lock.
            if let Some(ref hash_lock) = swap.hash_lock {
                let preimage = secret_preimage.ok_or(SwapError::HashLockMismatch)?;
                let digest = env.crypto().sha256(&preimage);
                if digest.to_bytes() != hash_lock.clone() {
                    return Err(SwapError::HashLockMismatch);
                }
            }

            let remaining_a = swap.remaining_amount;
            if fill_amount_a > remaining_a {
                return Err(SwapError::InvalidAmount);
            }
            if !swap.allow_partial_fills && fill_amount_a != remaining_a {
                return Err(SwapError::InvalidState);
            }

            let fill_amount_b = mul_div(fill_amount_a, swap.amount_b, swap.amount_a)
                .map_err(|_| SwapError::MathOverflow)?;
            ensure_valid_leg_amount(fill_amount_b)?;

            let fee = calculate_and_validate_fee(fill_amount_b, fee_bps)?;
            let party_a_amount = fill_amount_b
                .checked_sub(fee)
                .ok_or(SwapError::MathOverflow)?;

            // Party B sends token_b to this contract
            token::Client::new(&env, &swap.token_b).transfer(
                &taker,
                &env.current_contract_address(),
                &fill_amount_b,
            );

            // Contract forwards token_b minus fee to party A
            token::Client::new(&env, &swap.token_b).transfer(
                &env.current_contract_address(),
                &swap.party_a,
                &party_a_amount,
            );

            // Forward fee, splitting a referral share when a referrer is provided.
            if fee > 0 {
                let token_b = token::Client::new(&env, &swap.token_b);
                match referrer {
                    Some(ref referrer_addr) => {
                        let referral_fee = fee / 2;
                        let treasury_fee = fee
                            .checked_sub(referral_fee)
                            .ok_or(SwapError::MathOverflow)?;
                        if referral_fee > 0 {
                            token_b.transfer(
                                &env.current_contract_address(),
                                referrer_addr,
                                &referral_fee,
                            );
                        }
                        if treasury_fee > 0 {
                            token_b.transfer(
                                &env.current_contract_address(),
                                &treasury,
                                &treasury_fee,
                            );
                        }
                        events::referral_fee_paid(
                            &env,
                            swap_id,
                            referrer_addr,
                            referral_fee,
                        );
                    }
                    None => {
                        token_b.transfer(&env.current_contract_address(), &treasury, &fee);
                    }
                }
            }

            if swap.escrowed {
                token::Client::new(&env, &swap.token_a).transfer(
                    &env.current_contract_address(),
                    &taker,
                    &fill_amount_a,
                );
            } else {
                token::Client::new(&env, &swap.token_a).transfer_from(
                    &env.current_contract_address(),
                    &swap.party_a,
                    &taker,
                    &fill_amount_a,
                );
            }

            swap.filled_amount = swap
                .filled_amount
                .checked_add(fill_amount_a)
                .ok_or(SwapError::MathOverflow)?;
            swap.remaining_amount = swap
                .remaining_amount
                .checked_sub(fill_amount_a)
                .ok_or(SwapError::MathOverflow)?;
            if swap.remaining_amount == 0 {
                swap.state = SwapState::Executed;
            }

            env.storage()
                .persistent()
                .set(&SwapKey::Swap(swap_id), &swap);
            extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
            bump_persistent(&env, &SwapKey::Swap(swap_id));
            bump_instance(&env);
            events::swap_accepted(&env, &taker, swap_id, fill_amount_a);
            Ok(swap_id)
        }

        /// Fill part of a swap that allows partial fills, delivering a proportional
        /// amount of the counterparty token.
        ///
        /// # Errors
        /// - [`SwapError::SwapNotFound`]
        /// - [`SwapError::InvalidState`] if the swap does not allow partial fills
        /// - [`SwapError::InvalidAmount`] if `fill_amount` is non-positive or exceeds the remainder
        /// - [`SwapError::DeadlineExpired`]
        pub fn fill_swap_partial(
            env: Env,
            swap_id: u32,
            fill_amount: i128,
        ) -> Result<(), SwapError> {
            ensure_valid_leg_amount(fill_amount)?;

            let mut swap: SwapInfo = env
                .storage()
                .persistent()
                .get(&SwapKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)?;
            require_pending(swap.state)?;
            if !swap.allow_partial_fills {
                return Err(SwapError::InvalidState);
            }
            if env.ledger().sequence() > swap.expires_at {
                return Err(SwapError::DeadlineExpired);
            }
            if fill_amount > swap.remaining_amount {
                return Err(SwapError::InvalidAmount);
            }

            let taker = swap.party_a.clone();
            taker.require_auth();

            let fill_amount_b = mul_div(fill_amount, swap.amount_b, swap.amount_a)
                .map_err(|_| SwapError::MathOverflow)?;
            ensure_valid_leg_amount(fill_amount_b)?;

            token::Client::new(&env, &swap.token_b).transfer(
                &taker,
                &env.current_contract_address(),
                &fill_amount_b,
            );
            token::Client::new(&env, &swap.token_b).transfer(
                &env.current_contract_address(),
                &swap.party_a,
                &fill_amount_b,
            );
            token::Client::new(&env, &swap.token_a).transfer(
                &env.current_contract_address(),
                &taker,
                &fill_amount,
            );

            swap.filled_amount = swap
                .filled_amount
                .checked_add(fill_amount)
                .ok_or(SwapError::MathOverflow)?;
            swap.remaining_amount = swap
                .remaining_amount
                .checked_sub(fill_amount)
                .ok_or(SwapError::MathOverflow)?;
            if swap.remaining_amount == 0 {
                swap.state = SwapState::Executed;
            }

            env.storage()
                .persistent()
                .set(&SwapKey::Swap(swap_id), &swap);
            extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
            bump_persistent(&env, &SwapKey::Swap(swap_id));
            bump_instance(&env);
            events::swap_partially_filled(&env, &taker, swap_id, fill_amount, swap.remaining_amount);
            Ok(())
        }

        /// Cancel an open swap and return Party A's escrowed asset.
        /// Cancel a pending swap. Party A can cancel before expiry; anyone can cancel after expiry.
        ///
        /// # Errors
        /// - [`SwapError::NotInitialized`]
        /// - [`SwapError::SwapNotFound`]
        /// - [`SwapError::AlreadyCompleted`] / [`SwapError::AlreadyCancelled`]
        /// - [`SwapError::NotAuthorized`]
        pub fn cancel_swap(env: Env, swap_id: u32) -> Result<(), SwapError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(SwapError::NotInitialized);
            }

            let mut swap: SwapInfo = env
                .storage()
                .persistent()
                .get(&DataKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)?;
            require_pending(swap.state)?;

            let now = env.ledger().sequence();
            if now <= swap.expires_at {
                swap.party_a.require_auth();
            }

            if swap.escrowed {
                let remaining_a = swap
                    .amount_a
                    .checked_sub(swap.filled_amount)
                    .ok_or(SwapError::MathOverflow)?;
                if remaining_a > 0 {
                    token::Client::new(&env, &swap.token_a).transfer(
                        &env.current_contract_address(),
                        &swap.party_a,
                        &remaining_a,
                    );
                }
            }

            swap.state = SwapState::Cancelled;
            env.storage().persistent().set(&SwapKey::Swap(swap_id), &swap);
            extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
            bump_persistent(&env, &SwapKey::Swap(swap_id));
            bump_instance(&env);
            events::swap_cancelled(&env, &swap.party_a, swap_id);
            Ok(())
        }

        /// Refund a hash-locked swap to its creator after the deadline when the
        /// secret preimage was never revealed.
        ///
        /// # Errors
        /// - [`SwapError::SwapNotFound`]
        /// - [`SwapError::InvalidState`] if the swap is not hash-locked or not pending
        /// - [`SwapError::DeadlineExpired`] if the deadline has not yet passed
        pub fn refund_hashlocked_swap(env: Env, swap_id: u32) -> Result<(), SwapError> {
            let mut swap: SwapInfo = env
                .storage()
                .persistent()
                .get(&SwapKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)?;
            require_pending(swap.state)?;
            if swap.hash_lock.is_none() {
                return Err(SwapError::InvalidState);
            }
            if env.ledger().sequence() <= swap.expires_at {
                return Err(SwapError::DeadlineExpired);
            }

            swap.party_a.require_auth();

            if swap.escrowed {
                let remaining_a = swap.remaining_amount;
                if remaining_a > 0 {
                    token::Client::new(&env, &swap.token_a).transfer(
                        &env.current_contract_address(),
                        &swap.party_a,
                        &remaining_a,
                    );
                }
            }

            swap.state = SwapState::Cancelled;
            env.storage()
                .persistent()
                .set(&SwapKey::Swap(swap_id), &swap);
            extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
            bump_persistent(&env, &SwapKey::Swap(swap_id));
            bump_instance(&env);
            events::swap_cancelled(&env, &swap.party_a, swap_id);
            Ok(())
        }

        /// Cancel multiple pending swaps owned by `creator` in a single invocation,
        /// refunding all escrowed balances.
        ///
        /// # Errors
        /// - [`SwapError::InvalidAmount`] if `swap_ids` is empty
        /// - [`SwapError::SwapNotFound`] if any swap does not exist
        /// - [`SwapError::NotCreator`] if any swap is not owned by `creator`
        /// - [`SwapError::InvalidState`] if any swap is not pending
        pub fn cancel_swaps_batch(
            env: Env,
            creator: Address,
            swap_ids: Vec<u32>,
        ) -> Result<(), SwapError> {
            creator.require_auth();
            if swap_ids.is_empty() {
                return Err(SwapError::InvalidAmount);
            }

            for swap_id in swap_ids.iter() {
                let mut swap: SwapInfo = env
                    .storage()
                    .persistent()
                    .get(&SwapKey::Swap(swap_id))
                    .ok_or(SwapError::SwapNotFound)?;
                require_pending(swap.state)?;
                if swap.party_a != creator {
                    return Err(SwapError::NotCreator);
                }

                if swap.escrowed {
                    let remaining_a = swap.remaining_amount;
                    if remaining_a > 0 {
                        token::Client::new(&env, &swap.token_a).transfer(
                            &env.current_contract_address(),
                            &swap.party_a,
                            &remaining_a,
                        );
                    }
                }

                swap.state = SwapState::Cancelled;
                env.storage()
                    .persistent()
                    .set(&SwapKey::Swap(swap_id), &swap);
                extend_ttl_persistent(&env, &SwapKey::Swap(swap_id));
                bump_persistent(&env, &SwapKey::Swap(swap_id));
                events::swap_cancelled(&env, &swap.party_a, swap_id);
            }

            bump_instance(&env);
            Ok(())
        }

            match swap.state {
                SwapState::Completed => return Err(SwapError::AlreadyCompleted),
                SwapState::Cancelled => return Err(SwapError::AlreadyCancelled),
                SwapState::Open => {}
            }
            swap.party_a.require_auth();

            if swap.state == SwapState::Accepted {
                return Err(SwapError::AlreadyCompleted);
            }
            if swap.state == SwapState::Cancelled {
                return Err(SwapError::AlreadyCancelled);
            }
            if swap.state != SwapState::Pending {
                return Err(SwapError::InvalidState);
            }

            let current_ledger = env.ledger().sequence();
            let is_expired = current_ledger > swap.expires_at;

            if !is_expired {
                swap.party_a.require_auth();
            }

            swap.state = SwapState::Cancelled;
            env.storage()
                .persistent()
                .set(&DataKey::Swap(swap_id), &swap);
            bump_swap(&env, swap_id);
            token::Client::new(&env, &swap.token_a).transfer(
                &env.current_contract_address(),
                &swap.party_a,
                &swap.amount_a,
            );
            bump_instance(&env);

            // Refund escrowed token_a back to party A
            token::Client::new(&env, &swap.token_a).transfer(
                &env.current_contract_address(),
                &swap.party_a,
                &swap.amount_a,
            );

            events::swap_cancelled(&env, swap_id);
            Ok(())
        }

        /// Return a swap from persistent storage.
        /// Get swap details.
        ///
        /// # Errors
        /// - [`SwapError::SwapNotFound`]
        pub fn get_swap(env: Env, swap_id: u32) -> Result<SwapInfo, SwapError> {
            env.storage()
                .persistent()
                .get(&SwapKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)
        }

        pub fn propose_basket_swap(
            env: Env,
            party_a: Address,
            offers: Vec<BasketLeg>,
            demands: Vec<BasketLeg>,
            expires_at: u32,
        ) -> Result<u32, SwapError> {
            get_required::<bool>(&env, &DataKey::Initialized)?;
            party_a.require_auth();
            if expires_at <= env.ledger().sequence() {
                return Err(SwapError::InvalidDeadline);
            }
            if offers.is_empty() || demands.is_empty() {
                return Err(SwapError::InvalidAmount);
            }
            for leg in offers.iter() {
                ensure_valid_leg_amount(leg.amount)?;
            }
            for leg in demands.iter() {
                ensure_valid_leg_amount(leg.amount)?;
            }

            let swap_id: u32 = env.storage().instance().get(&BasketSwapCount).unwrap_or(0);
            let next_id = swap_id.checked_add(1).ok_or(SwapError::StorageError)?;
            let swap = BasketSwapInfo {
                id: swap_id,
                party_a: party_a.clone(),
                offers,
                demands,
                expires_at,
                state: SwapState::Pending,
            };
            env.storage()
                .persistent()
                .set(&SwapKey::BasketSwap(swap_id), &swap);
            env.storage().instance().set(&BasketSwapCount, &next_id);
            extend_ttl_persistent(&env, &SwapKey::BasketSwap(swap_id));
            bump_persistent(&env, &SwapKey::BasketSwap(swap_id));
            bump_instance(&env);
            events::basket_swap_proposed(&env, &party_a, swap_id, expires_at);
            Ok(swap_id)
        }

        pub fn accept_basket_swap(env: Env, swap_id: u32, party_b: Address) -> Result<u32, SwapError> {
            party_b.require_auth();
            let mut swap: BasketSwapInfo = env
                .storage()
                .persistent()
                .get(&SwapKey::BasketSwap(swap_id))
                .ok_or(SwapError::BasketSwapNotFound)?;
            require_pending(swap.state)?;
            if env.ledger().sequence() > swap.expires_at {
                return Err(SwapError::DeadlineExpired);
            }

            // Two legs execute in one invocation; host failure rolls the full call back.
            for leg in swap.demands.iter() {
                token::Client::new(&env, &leg.token).transfer(&party_b, &swap.party_a, &leg.amount);
            }
            for leg in swap.offers.iter() {
                token::Client::new(&env, &leg.token).transfer_from(
                    &env.current_contract_address(),
                    &swap.party_a,
                    &party_b,
                    &leg.amount,
                );
            }

            swap.state = SwapState::Executed;
            env.storage()
                .persistent()
                .set(&SwapKey::BasketSwap(swap_id), &swap);
            extend_ttl_persistent(&env, &SwapKey::BasketSwap(swap_id));
            bump_persistent(&env, &SwapKey::BasketSwap(swap_id));
            bump_instance(&env);
            events::basket_swap_accepted(&env, &party_b, swap_id);
            Ok(swap_id)
        }

        pub fn get_basket_swap(env: Env, swap_id: u32) -> Result<BasketSwapInfo, SwapError> {
            env.storage()
                .persistent()
                .get(&SwapKey::BasketSwap(swap_id))
                .ok_or(SwapError::BasketSwapNotFound)
                .get(&DataKey::Swap(swap_id))
                .ok_or(SwapError::SwapNotFound)?;
            bump_swap(&env, swap_id);
            Ok(swap)
        }

        /// Enumerate active (pending and non-expired) swaps, paginated by swap ID.
        ///
        /// `cursor` is the swap ID to resume scanning from (pass `0` to start from the beginning).
        /// `limit` is the maximum number of active swaps to return and is capped at [`MAX_SWAPS_PAGE_SIZE`].
        ///
        /// Uses [`soroban_common::paginate`] to filter deterministic active listings.
        pub fn get_active_swaps(env: Env, cursor: u32, limit: u32) -> SwapPage {
            let total_count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::SwapCount)
                .unwrap_or(0);

            let current_ledger = env.ledger().sequence();

            let page = paginate(
                &env,
                cursor,
                limit,
                MAX_SWAPS_PAGE_SIZE,
                |c| {
                    let next = c.saturating_add(1);
                    if next < total_count {
                        Some(next)
                    } else {
                        None
                    }
                },
                |c| {
                    if c >= total_count {
                        return None;
                    }
                    let swap: SwapInfo = env.storage().persistent().get(&SwapKey::Swap(c))?;
                    if swap.state == SwapState::Pending && current_ledger <= swap.expires_at {
                        Some(swap)
                    } else {
                        None
                    }
                },
            );

            SwapPage {
                swaps: page.items,
                next_cursor: page.next_cursor,
            }
        }
    }
}

#[cfg(test)]
mod prop_test;
mod test;
