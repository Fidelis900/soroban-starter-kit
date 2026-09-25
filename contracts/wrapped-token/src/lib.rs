#![no_std]
#![deny(missing_docs)]
//! Wrapped-token contract template.
//!
//! Wraps an underlying asset 1:1, minting wrapped tokens on deposit and
//! burning them on withdrawal.

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::WrappedTokenError;
pub use storage::DataKey;

use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance, extend_ttl_persistent,
};

fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Returns an error if the contract is currently paused. Only compiled when
/// the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
fn require_not_paused(env: &Env) -> Result<(), WrappedTokenError> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(WrappedTokenError::Unauthorized);
    }
    Ok(())
}

/// Loads the admin address, or `NotInitialized` if the contract hasn't been set up.
/// Only compiled when the `pausable` feature is enabled.
#[cfg(feature = "pausable")]
fn require_admin(env: &Env) -> Result<Address, WrappedTokenError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(WrappedTokenError::NotInitialized)
}

/// Wrapped asset token contract.
///
/// Users deposit an underlying asset and receive an equivalent amount of wrapped tokens.
/// Users can burn wrapped tokens to retrieve the underlying asset.
///
/// Flow:
/// 1. Admin calls `initialize` — sets up the wrapped token and underlying asset addresses,
///    and optionally a per-address cap on cumulative wrapping.
/// 2. Users call `wrap` to deposit underlying assets and mint wrapped tokens (1:1 peg).
/// 3. Users call `unwrap` to burn wrapped tokens and receive underlying assets (1:1 peg).
///
/// `get_reserve_balance` lets off-chain monitoring assert the invariant that
/// `get_total_wrapped() <= get_reserve_balance()` always holds.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct WrappedTokenContract;

    #[contractimpl]
    impl WrappedTokenContract {
        /// Initialize the wrapped token contract.
        ///
        /// `max_wrap_per_address` optionally bounds the cumulative amount any single
        /// address may ever wrap, to limit concentration risk. Pass `None` for no cap.
        ///
        /// # Errors
        /// - [`WrappedTokenError::AlreadyInitialized`] if called more than once.
        /// - [`WrappedTokenError::InvalidAmount`] if `max_wrap_per_address` is `Some(n)` with `n <= 0`.
        pub fn initialize(
            env: Env,
            admin: Address,
            wrapped_token: Address,
            underlying_token: Address,
            max_wrap_per_address: Option<i128>,
        ) -> Result<(), WrappedTokenError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(WrappedTokenError::AlreadyInitialized);
            }
            admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage()
                .instance()
                .set(&DataKey::WrappedToken, &wrapped_token);
            env.storage()
                .instance()
                .set(&DataKey::UnderlyingToken, &underlying_token);
            env.storage().instance().set(&DataKey::TotalWrapped, &0i128);

            if let Some(cap) = max_wrap_per_address {
                if cap <= 0 {
                    return Err(WrappedTokenError::InvalidAmount);
                }
                env.storage()
                    .instance()
                    .set(&DataKey::MaxWrapPerAddress, &cap);
            }

            bump(&env);
            events::initialized(&env, &admin, &wrapped_token);
            Ok(())
        }

        /// Wrap underlying assets by transferring them to the contract and minting wrapped tokens.
        ///
        /// 1:1 peg is maintained: amount underlying = amount wrapped tokens.
        ///
        /// # Errors
        /// - [`WrappedTokenError::NotInitialized`] if the contract has not been initialized.
        /// - [`WrappedTokenError::Unauthorized`] if the contract is paused (requires the `pausable` feature).
        /// - [`WrappedTokenError::InvalidAmount`] if `amount` <= 0.
        /// - [`WrappedTokenError::MaxWrapExceeded`] if `user`'s cumulative wrapped amount would exceed
        ///   the per-address cap set at `initialize`.
        pub fn wrap(env: Env, user: Address, amount: i128) -> Result<(), WrappedTokenError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(WrappedTokenError::NotInitialized);
            }
            #[cfg(feature = "pausable")]
            require_not_paused(&env)?;
            if amount <= 0 {
                return Err(WrappedTokenError::InvalidAmount);
            }
            user.require_auth();

            let wrapped_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::WrappedToken)
                .ok_or(WrappedTokenError::NotInitialized)?;

            let underlying_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::UnderlyingToken)
                .ok_or(WrappedTokenError::NotInitialized)?;

            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, i128>(&DataKey::MaxWrapPerAddress)
            {
                let wrapped_by_user: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::WrappedByAddress(user.clone()))
                    .unwrap_or(0i128);
                let new_user_total = wrapped_by_user + amount;
                if new_user_total > cap {
                    return Err(WrappedTokenError::MaxWrapExceeded);
                }
                env.storage()
                    .persistent()
                    .set(&DataKey::WrappedByAddress(user.clone()), &new_user_total);
                extend_ttl_persistent(
                    &env,
                    &DataKey::WrappedByAddress(user.clone()),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalWrapped)
                .unwrap_or(0i128);
            let new_total = total + amount;

            // Commit local accounting before making external token calls. Soroban rolls back
            // this transaction if either interaction fails, while this ordering prevents a
            // re-entrant token implementation from observing stale supply accounting.
            env.storage()
                .instance()
                .set(&DataKey::TotalWrapped, &new_total);
            bump(&env);

            // Transfer underlying asset from user to contract.
            token::Client::new(&env, &underlying_token).transfer(
                &user,
                &env.current_contract_address(),
                &amount,
            );

            // Mint wrapped tokens to user. `wrapped_token` must be a Stellar Asset Contract
            // (or other token exposing the admin-mint interface) with this contract set as its admin.
            token::StellarAssetClient::new(&env, &wrapped_token).mint(&user, &amount);
            events::wrapped(&env, &user, amount, new_total);
            Ok(())
        }

        /// Unwrap wrapped tokens by burning them and sending underlying assets back to the user.
        ///
        /// 1:1 peg is maintained: amount wrapped tokens = amount underlying assets.
        ///
        /// # Errors
        /// - [`WrappedTokenError::NotInitialized`] if the contract has not been initialized.
        /// - [`WrappedTokenError::Unauthorized`] if the contract is paused (requires the `pausable` feature).
        /// - [`WrappedTokenError::InvalidAmount`] if `amount` <= 0.
        /// - [`WrappedTokenError::InsufficientBalance`] if user has insufficient wrapped tokens.
        pub fn unwrap(env: Env, user: Address, amount: i128) -> Result<(), WrappedTokenError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(WrappedTokenError::NotInitialized);
            }
            #[cfg(feature = "pausable")]
            require_not_paused(&env)?;
            if amount <= 0 {
                return Err(WrappedTokenError::InvalidAmount);
            }
            user.require_auth();

            let wrapped_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::WrappedToken)
                .ok_or(WrappedTokenError::NotInitialized)?;

            let underlying_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::UnderlyingToken)
                .ok_or(WrappedTokenError::NotInitialized)?;

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalWrapped)
                .unwrap_or(0i128);
            let new_total = total - amount;

            // Commit local accounting before making external token calls. Any failed call
            // aborts the transaction and rolls back this effect atomically.
            env.storage()
                .instance()
                .set(&DataKey::TotalWrapped, &new_total);
            bump(&env);

            // Burn wrapped tokens from user.
            token::Client::new(&env, &wrapped_token).burn(&user, &amount);

            // Transfer underlying asset from contract to user.
            token::Client::new(&env, &underlying_token).transfer(
                &env.current_contract_address(),
                &user,
                &amount,
            );
            events::unwrapped(&env, &user, amount, new_total);
            Ok(())
        }

        /// Returns the total amount of wrapped tokens.
        pub fn get_total_wrapped(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::TotalWrapped)
                .unwrap_or(0i128)
        }

        /// Returns the underlying asset's balance held by this contract, i.e. the actual
        /// reserve backing the wrapped supply.
        ///
        /// For monitoring: `get_total_wrapped() <= get_reserve_balance()` should always hold.
        ///
        /// # Errors
        /// - [`WrappedTokenError::NotInitialized`] if the contract has not been initialized.
        pub fn get_reserve_balance(env: Env) -> Result<i128, WrappedTokenError> {
            let underlying_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::UnderlyingToken)
                .ok_or(WrappedTokenError::NotInitialized)?;
            Ok(
                token::Client::new(&env, &underlying_token)
                    .balance(&env.current_contract_address()),
            )
        }

        /// Returns the per-address wrap cap set at `initialize`, or `None` if uncapped.
        pub fn max_wrap_per_address(env: Env) -> Option<i128> {
            env.storage().instance().get(&DataKey::MaxWrapPerAddress)
        }

        /// Returns the cumulative amount `account` has wrapped so far (0 if never wrapped,
        /// or if no cap is configured and nothing was ever tracked).
        pub fn wrapped_by(env: Env, account: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::WrappedByAddress(account))
                .unwrap_or(0i128)
        }
    }

    /// Pause / unpause — only compiled when the `pausable` feature is enabled.
    #[cfg(feature = "pausable")]
    #[contractimpl]
    impl WrappedTokenContract {
        /// Pause `wrap` and `unwrap`. Admin only.
        pub fn pause(env: Env) -> Result<(), WrappedTokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &true);
            bump(&env);
            events::paused(&env, &admin);
            Ok(())
        }

        /// Resume `wrap` and `unwrap`. Admin only.
        pub fn unpause(env: Env) -> Result<(), WrappedTokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &false);
            bump(&env);
            events::unpaused(&env, &admin);
            Ok(())
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;
