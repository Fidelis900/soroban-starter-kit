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

/// Minimum number of initial LP shares permanently locked to the dead address
/// on first liquidity provision, preventing share-price inflation attacks.
pub const MINIMUM_LIQUIDITY: i128 = 1_000;

/// Fixed dead address (all-zero ed25519 public key) that receives the locked
/// minimum liquidity shares. No private key exists for this address.
fn dead_address(env: &Env) -> Address {
    Address::from_string_bytes(&soroban_sdk::Bytes::from_array(env, &[0u8; 32]))
}

/// Accrue cumulative prices (`price_0_cumulative`, `price_1_cumulative`) using
/// the pre-update reserves and the number of ledgers elapsed since the last
/// update, then record the current ledger as the new update point.
///
/// Returns the updated `(price_0_cumulative, price_1_cumulative)` pair.
fn update_cumulative_prices(env: &Env, reserve_0: i128, reserve_1: i128) -> (i128, i128) {
    let current_ledger = env.ledger().sequence();
    let last_update: u32 = env
        .storage()
        .instance()
        .get(&DataKey::LastUpdateLedger)
        .unwrap_or(current_ledger);
    let elapsed = current_ledger.saturating_sub(last_update) as i128;

    let mut price_0_cumulative: i128 = env
        .storage()
        .instance()
        .get(&DataKey::Price0Cumulative)
        .unwrap_or(0);
    let mut price_1_cumulative: i128 = env
        .storage()
        .instance()
        .get(&DataKey::Price1Cumulative)
        .unwrap_or(0);

    if elapsed > 0 && reserve_0 > 0 && reserve_1 > 0 {
        // price_0 = reserve_1 / reserve_0, price_1 = reserve_0 / reserve_1,
        // scaled by 1e7 to retain precision in integer arithmetic.
        const PRICE_SCALE: i128 = 10_000_000;
        let delta_0 = reserve_1.saturating_mul(PRICE_SCALE) / reserve_0;
        let delta_1 = reserve_0.saturating_mul(PRICE_SCALE) / reserve_1;
        price_0_cumulative = price_0_cumulative.saturating_add(delta_0.saturating_mul(elapsed));
        price_1_cumulative = price_1_cumulative.saturating_add(delta_1.saturating_mul(elapsed));
    }

    env.storage()
        .instance()
        .set(&DataKey::Price0Cumulative, &price_0_cumulative);
    env.storage()
        .instance()
        .set(&DataKey::Price1Cumulative, &price_1_cumulative);
    env.storage()
        .instance()
        .set(&DataKey::LastUpdateLedger, &current_ledger);

    (price_0_cumulative, price_1_cumulative)
}

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
        SwapState::Expired => SwapError::AlreadyCancelled,
        SwapState::Pending => SwapError::InvalidState,
    }
}

/// Return the contract's current balance of `token`.
fn token_balance(env: &Env, token: &Address) -> i128 {
    token::Client::new(env, token).balance(&env.current_contract_address())
}

/// Transfer `amount` of `token` from `from` into the contract and return the
/// actual amount received (post-balance minus pre-balance). This accounts for
/// fee-on-transfer tokens where the contract receives less than `amount`.
fn transfer_in_measured(env: &Env, token: &Address, from: &Address, amount: i128) -> i128 {
    let before = token_balance(env, token);
    token::Client::new(env, token).transfer(from, &env.current_contract_address(), amount);
    let after = token_balance(env, token);
    after.saturating_sub(before)
}

/// Integer square root via Newton's method, used for initial LP share minting.
fn isqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn require_pending(state: SwapState) -> Result<(), SwapError> {
    if state != SwapState::Pending {
        return Err(map_state_err(state));
    }
    Ok(())
}

/// Compute the constant-product invariant `k = reserve_a * reserve_b`.
///
/// Uses checked multiplication so that an overflow is surfaced as a dedicated
/// error rather than silently wrapping and corrupting the invariant check.
fn compute_k(reserve_a: i128, reserve_b: i128) -> Result<i128, SwapError> {
    reserve_a
        .checked_mul(reserve_b)
        .ok_or(SwapError::InvariantViolation)
}

/// Assert that the pool invariant did not decrease across a swap.
///
/// Precision rounding in swap output calculations can cause `k` to shrink
/// slightly after a swap, which drains the pool over time. Every swap must
/// therefore end with `new_k >= old_k`.
fn assert_invariant_non_decreasing(
    reserve_a_before: i128,
    reserve_b_before: i128,
    reserve_a_after: i128,
    reserve_b_after: i128,
) -> Result<(), SwapError> {
    let old_k = compute_k(reserve_a_before, reserve_b_before)?;
    let new_k = compute_k(reserve_a_after, reserve_b_after)?;
    if new_k < old_k {
        return Err(SwapError::InvariantViolation);
    }
    Ok(())
}

/// Callback interface invoked by [`SwapContract::flash_swap`] on the caller
/// contract after the requested tokens have been transferred out.
#[soroban_sdk::contractclient(name = "FlashSwapCallbackClient")]
pub trait FlashSwapCallback {
    /// Called by the swap contract after lending `amount_out` of `token_out`.
    ///
    /// The callee must ensure that by the time this returns, the swap contract
    /// holds at least `amount_in` of `token_in` (loan plus fee) so repayment
    /// can be verified within the same transaction.
    fn flash_swap_callback(
        env: Env,
        token_in: Address,
        amount_in: i128,
        token_out: Address,
        amount_out: i128,
        callback_data: soroban_sdk::Bytes,
    );
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
            let id: u32 = get_instance(&env, &DataKey::SwapCount)?;
            let next_id = id.checked_add(1).ok_or(SwapError::InvalidAmount)?;

            // Escrow token_a from party_a to this contract, measuring the actual
            // amount received to remain resilient to fee-on-transfer tokens.
            let received_a = transfer_in_measured(&env, &token_a, &party_a, amount_a);
            let swap = SwapInfo {
                id,

            let swap_id: u32 = env
                .storage()
                .instance()
                .get(&DataKey::SwapCount)
                .unwrap_or(0);
            let next_id = swap_id.checked_add(1).ok_or(SwapError::InvalidAmount)?;

            if escrowed {
                transfer_in_measured(&env, &token_a, &party_a, amount_a);
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

            // Accrue cumulative prices at the start of the swap using the
            // pre-swap reserves so TWAP observations stay manipulation-resistant.
            let reserve_0: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve0)
                .unwrap_or(0);
            let reserve_1: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve1)
                .unwrap_or(0);
            update_cumulative_prices(&env, reserve_0, reserve_1);

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

            // Snapshot pool reserves before any transfers so the constant-product
            // invariant can be verified once the swap completes.
            let reserve_a_before = token_balance(&env, &swap.token_a);
            let reserve_b_before = token_balance(&env, &swap.token_b);

            let fee = calculate_and_validate_fee(fill_amount_b, fee_bps)?;
            let party_a_amount = fill_amount_b
                .checked_sub(fee)
                .ok_or(SwapError::MathOverflow)?;

            // Party B sends token_b to this contract. Measure the actual amount
            // received so fee-on-transfer tokens are handled correctly.
            let received_b = transfer_in_measured(&env, &swap.token_b, &taker, fill_amount_b);

            // Base fulfillment on the actual received delta rather than the
            // requested amount. The fee is recomputed against the received
            // amount so the contract never forwards more than it holds.
            let actual_fee = calculate_and_validate_fee(received_b, fee_bps)?;
            let actual_party_a_amount = received_b
                .checked_sub(actual_fee)
                .ok_or(SwapError::MathOverflow)?;

            // Contract forwards token_b minus fee to party A
            if actual_party_a_amount > 0 {
                token::Client::new(&env, &swap.token_b).transfer(
                    &env.current_contract_address(),
                    &swap.party_a,
                    &actual_party_a_amount,
                );
            }

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
            // Forward fee to treasury if fee > 0
            if actual_fee > 0 {
                token::Client::new(&env, &swap.token_b).transfer(
                    &env.current_contract_address(),
                    &treasury,
                    &actual_fee,
                );
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

            // Enforce the constant-product invariant: k must never decrease.
            let reserve_a_after = token_balance(&env, &swap.token_a);
            let reserve_b_after = token_balance(&env, &swap.token_b);
            assert_invariant_non_decreasing(
                reserve_a_before,
                reserve_b_before,
                reserve_a_after,
                reserve_b_after,
            )?;

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

        /// Execute an uncollateralized flash swap.
        ///
        /// The contract transfers `amount_out` of `token_out` to `caller`, then
        /// invokes [`FlashSwapCallback::flash_swap_callback`] on `caller`. Before
        /// the transaction concludes the contract verifies that its `token_in`
        /// balance increased by at least `amount_in` (loan plus fee). If repayment
        /// is insufficient the whole invocation reverts.
        ///
        /// # Errors
        /// - [`SwapError::InvalidAmount`] if `amount_in` or `amount_out` <= 0.
        /// - [`SwapError::InsufficientLiquidity`] if the pool cannot cover `amount_out`.
        /// - [`SwapError::FlashRepaymentNotMet`] if repayment plus fee is short.
        #[allow(clippy::too_many_arguments)]
        pub fn flash_swap(
            env: Env,
            caller: Address,
            token_in: Address,
            amount_in: i128,
            token_out: Address,
            amount_out: i128,
            callback_data: soroban_sdk::Bytes,
        ) -> Result<(), SwapError> {
            caller.require_auth();
            ensure_valid_leg_amount(amount_in)?;
            ensure_valid_leg_amount(amount_out)?;

            let fee_bps: u32 = get_required(&env, &FeeBps)?;
            let fee = calculate_and_validate_fee(amount_in, fee_bps)?;
            let repayment_due = amount_in
                .checked_add(fee)
                .ok_or(SwapError::MathOverflow)?;

            let reserve_in_before = token_balance(&env, &token_in);
            let reserve_out_before = token_balance(&env, &token_out);
            if reserve_out_before < amount_out {
                return Err(SwapError::InsufficientLiquidity);
            }

            // Lend the requested tokens to the caller.
            token::Client::new(&env, &token_out).transfer(
                &env.current_contract_address(),
                &caller,
                &amount_out,
            );

            // Execute the caller's callback within the same transaction.
            FlashSwapCallbackClient::new(&env, &caller).flash_swap_callback(
                &token_in,
                &amount_in,
                &token_out,
                &amount_out,
                &callback_data,
            );

            // Verify repayment plus fee before the transaction concludes.
            let reserve_in_after = token_balance(&env, &token_in);
            let repaid = reserve_in_after.saturating_sub(reserve_in_before);
            if repaid < repayment_due {
                return Err(SwapError::FlashRepaymentNotMet);
            }

            // Enforce the constant-product invariant across the flash swap.
            let reserve_out_after = token_balance(&env, &token_out);
            assert_invariant_non_decreasing(
                reserve_in_before,
                reserve_out_before,
                reserve_in_after,
                reserve_out_after,
            )?;

            bump_instance(&env);
            events::flash_swap(&env, &caller, &token_in, amount_in, &token_out, amount_out, fee);
            Ok(())
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

            // Snapshot reserves before transfers to verify the invariant after.
            let reserve_a_before = token_balance(&env, &swap.token_a);
            let reserve_b_before = token_balance(&env, &swap.token_b);

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

            // Enforce the constant-product invariant: k must never decrease.
            let reserve_a_after = token_balance(&env, &swap.token_a);
            let reserve_b_after = token_balance(&env, &swap.token_b);
            assert_invariant_non_decreasing(
                reserve_a_before,
                reserve_b_before,
                reserve_a_after,
                reserve_b_after,
            )?;

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
        /// Permissionlessly expire a swap whose deadline has passed.
        ///
        /// Anyone (e.g. an automated keeper) may call this once
        /// `env.ledger().sequence() > swap.expires_at`. Any escrowed tokens are
        /// returned to the original creator and the swap is marked `Expired`.
        ///
        /// # Errors
        /// - [`SwapError::SwapNotFound`]
        /// - [`SwapError::InvalidState`] if the swap is not pending.
        /// - [`SwapError::DeadlineExpired`] if the deadline has not yet passed.
        pub fn expire_swap(env: Env, swap_id: u32) -> Result<(), SwapError> {
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
            swap.state = SwapState::Expired;
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
            events::swap_expired(&env, swap_id, &swap.party_a);
            Ok(())
        }

            match swap.state {
                SwapState::Completed => return Err(SwapError::AlreadyCompleted),
                SwapState::Cancelled => return Err(SwapError::AlreadyCancelled),
                SwapState::Expired => return Err(SwapError::AlreadyCancelled),
                SwapState::Open => {}
            }
            swap.party_a.require_auth();

            if swap.state == SwapState::Accepted {
                return Err(SwapError::AlreadyCompleted);
            }
            if swap.state == SwapState::Cancelled {
                return Err(SwapError::AlreadyCancelled);
            }
            if swap.state == SwapState::Expired {
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
        }

        /// Return the stored cumulative price accumulators as
        /// `(price_0_cumulative, price_1_cumulative)`.
        ///
        /// External contracts can derive a TWAP by differencing two observations
        /// and dividing by the number of elapsed ledgers.
        pub fn get_cumulative_prices(env: Env) -> (i128, i128) {
            let price_0_cumulative: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Price0Cumulative)
                .unwrap_or(0);
            let price_1_cumulative: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Price1Cumulative)
                .unwrap_or(0);
            (price_0_cumulative, price_1_cumulative)
        }

        /// Provide initial liquidity to a pool, permanently locking
        /// [`MINIMUM_LIQUIDITY`] shares to the dead address on first provision.
        ///
        /// Returns the number of LP shares credited to `provider` (net of the
        /// locked minimum on the initial deposit).
        ///
        /// # Errors
        /// - [`SwapError::InsufficientInitialLiquidity`] if the initial deposit
        ///   would mint no more than the locked minimum.
        pub fn add_liquidity(
            env: Env,
            provider: Address,
            amount_0: i128,
            amount_1: i128,
        ) -> Result<i128, SwapError> {
            provider.require_auth();
            ensure_valid_leg_amount(amount_0)?;
            ensure_valid_leg_amount(amount_1)?;

            let reserve_0: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve0)
                .unwrap_or(0);
            let reserve_1: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve1)
                .unwrap_or(0);
            let total_shares: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalShares)
                .unwrap_or(0);

            // Accrue cumulative prices against the pre-update reserves.
            update_cumulative_prices(&env, reserve_0, reserve_1);

            let provider_shares = if total_shares == 0 {
                // Initial provision: geometric mean of the deposited amounts.
                let product = amount_0
                    .checked_mul(amount_1)
                    .ok_or(SwapError::MathOverflow)?;
                let liquidity = isqrt(product);
                if liquidity <= MINIMUM_LIQUIDITY {
                    return Err(SwapError::InsufficientInitialLiquidity);
                }
                let locked = MINIMUM_LIQUIDITY;
                let net = liquidity
                    .checked_sub(locked)
                    .ok_or(SwapError::InsufficientInitialLiquidity)?;
                env.storage().instance().set(&DataKey::TotalShares, &liquidity);
                events::liquidity_locked(&env, &dead_address(&env), locked);
                net
            } else {
                let shares_0 = mul_div(amount_0, total_shares, reserve_0)
                    .map_err(|_| SwapError::MathOverflow)?;
                let shares_1 = mul_div(amount_1, total_shares, reserve_1)
                    .map_err(|_| SwapError::MathOverflow)?;
                let minted = shares_0.min(shares_1);
                if minted <= 0 {
                    return Err(SwapError::InvalidAmount);
                }
                let new_total = total_shares
                    .checked_add(minted)
                    .ok_or(SwapError::MathOverflow)?;
                env.storage()
                    .instance()
                    .set(&DataKey::TotalShares, &new_total);
                minted
            };

            env.storage()
                .instance()
                .set(&DataKey::Reserve0, &reserve_0.saturating_add(amount_0));
            env.storage()
                .instance()
                .set(&DataKey::Reserve1, &reserve_1.saturating_add(amount_1));
            bump_instance(&env);
            Ok(provider_shares)
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
