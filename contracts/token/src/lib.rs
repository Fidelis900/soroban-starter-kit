#![no_std]
#![deny(missing_docs)]
//! Fungible token contract template (SEP-41 / TokenInterface).
//!
//! Implements the standard Soroban token interface with admin-controlled mint,
//! burn, allowances, and optional pausable, freezable, and capped-supply features.

#[cfg(test)]
extern crate std;

use soroban_sdk::{
    Address, BytesN, Env, String, contract, contractimpl, token, token::TokenInterface,
};

mod admin;
mod allowance;
mod errors;
mod events;
mod permit;
mod storage;
mod token_interface;

#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod test;

use admin::require_admin;
use allowance::allowance_is_active;
use errors::TokenError;
use soroban_common::{
    LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance, extend_ttl_persistent,
};
use storage::{AllowanceDataKey, AllowanceValue, DataKey, MetadataKey};

#[cfg(feature = "pausable")]
pub(crate) fn require_not_paused(env: &Env) -> Result<(), TokenError> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(TokenError::Unauthorized);
    }
    Ok(())
}

#[cfg(feature = "freeze")]
pub(crate) fn require_not_frozen(env: &Env, account: &Address) -> Result<(), TokenError> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Frozen(account.clone()))
        .unwrap_or(false)
    {
        return Err(TokenError::Unauthorized);
    }
    Ok(())
}

pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct TokenContract;

    #[contractimpl]
    impl TokenContract {
        /// Initialize the token with metadata and an admin. Must be called once.
        pub fn initialize(
            env: Env,
            admin: Address,
            name: String,
            symbol: String,
            decimals: u32,
            max_supply: Option<i128>,
        ) -> Result<(), TokenError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(TokenError::AlreadyInitialized);
            }
            admin.require_auth();
            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage()
                .instance()
                .set(&DataKey::Metadata(MetadataKey::Name), &name);
            env.storage()
                .instance()
                .set(&DataKey::Metadata(MetadataKey::Symbol), &symbol);
            env.storage()
                .instance()
                .set(&DataKey::Metadata(MetadataKey::Decimals), &decimals);
            env.storage().instance().set(&DataKey::TotalSupply, &0i128);
            env.storage().instance().set(&DataKey::Version, &1u32);
            #[cfg(feature = "capped-supply")]
            if let Some(cap) = max_supply {
                if cap <= 0 {
                    return Err(TokenError::InvalidAmount);
                }
                env.storage().instance().set(&DataKey::MaxSupply, &cap);
            }
            #[cfg(not(feature = "capped-supply"))]
            let _ = max_supply;
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::initialized(&env, &admin, name, symbol, decimals);
            Ok(())
        }

        /// Mint `amount` tokens to `to`. Admin only.
        pub fn mint(env: Env, to: Address, amount: i128) -> Result<(), TokenError> {
            #[cfg(feature = "pausable")]
            require_not_paused(&env)?;
            let admin = require_admin(&env)?;
            admin.require_auth();
            if amount <= 0 {
                return Err(TokenError::InvalidAmount);
            }
            #[cfg(feature = "capped-supply")]
            {
                let supply: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                if let Some(cap) = env
                    .storage()
                    .instance()
                    .get::<DataKey, i128>(&DataKey::MaxSupply)
                {
                    if supply.checked_add(amount).ok_or(TokenError::Overflow)? > cap {
                        return Err(TokenError::InvalidAmount);
                    }
                }
            }
            Self::update_balance(&env, &to, amount)?;
            let supply: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            let new_supply = supply.checked_add(amount).ok_or(TokenError::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &new_supply);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::minted(&env, &to, amount);
            Ok(())
        }

        /// Mint tokens to multiple recipients in a single transaction. Admin only.
        pub fn batch_mint(
            env: Env,
            recipients: soroban_sdk::Vec<(Address, i128)>,
        ) -> Result<(), TokenError> {
            #[cfg(feature = "pausable")]
            require_not_paused(&env)?;
            let admin = require_admin(&env)?;
            admin.require_auth();

            let mut total_amount: i128 = 0;
            for (_, amount) in recipients.iter() {
                if amount <= 0 {
                    return Err(TokenError::InvalidAmount);
                }
                total_amount = total_amount
                    .checked_add(amount)
                    .ok_or(TokenError::Overflow)?;
            }

            #[cfg(feature = "capped-supply")]
            {
                let supply: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::TotalSupply)
                    .unwrap_or(0);
                if let Some(cap) = env
                    .storage()
                    .instance()
                    .get::<DataKey, i128>(&DataKey::MaxSupply)
                {
                    if supply
                        .checked_add(total_amount)
                        .ok_or(TokenError::Overflow)?
                        > cap
                    {
                        return Err(TokenError::InvalidAmount);
                    }
                }
            }

            for (to, amount) in recipients.iter() {
                let balance: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Balance(to.clone()))
                    .unwrap_or(0);
                let new_balance = balance.checked_add(amount).ok_or(TokenError::Overflow)?;
                env.storage()
                    .persistent()
                    .set(&DataKey::Balance(to.clone()), &new_balance);
                extend_ttl_persistent(
                    &env,
                    &DataKey::Balance(to.clone()),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
                events::minted(&env, &to, amount);
            }

            let supply: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            let new_supply = supply
                .checked_add(total_amount)
                .ok_or(TokenError::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::TotalSupply, &new_supply);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);

            Ok(())
        }

        /// Burn `amount` tokens from `from`. Admin only.
        pub fn admin_burn(env: Env, from: Address, amount: i128) -> Result<(), TokenError> {
            #[cfg(feature = "pausable")]
            require_not_paused(&env)?;
            let admin = require_admin(&env)?;
            admin.require_auth();
            if amount <= 0 {
                return Err(TokenError::InvalidAmount);
            }
            Self::update_balance(&env, &from, -amount)?;
            let supply: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0);
            env.storage().instance().set(
                &DataKey::TotalSupply,
                &(supply.checked_sub(amount).ok_or(TokenError::Overflow)?),
            );
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::burned(&env, &from, amount);
            Ok(())
        }

        /// Propose a new admin. Current admin only.
        pub fn propose_admin(env: Env, new_admin: Address) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::PendingAdmin, &new_admin);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::admin_proposed(&env, &admin, &new_admin);
            Ok(())
        }

        /// Accept a pending admin transfer. Must be called by the pending admin.
        pub fn accept_admin(env: Env) -> Result<(), TokenError> {
            let pending: Address = env
                .storage()
                .instance()
                .get(&DataKey::PendingAdmin)
                .ok_or(TokenError::Unauthorized)?;
            pending.require_auth();
            env.storage().instance().set(&DataKey::Admin, &pending);
            env.storage().instance().remove(&DataKey::PendingAdmin);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::admin_accepted(&env, &pending);
            Ok(())
        }

        /// Cancel a pending admin transfer. Current admin only.
        pub fn cancel_admin_proposal(env: Env) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().remove(&DataKey::PendingAdmin);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::admin_proposal_cancelled(&env, &admin);
            Ok(())
        }

        /// Deprecated: use `propose_admin` + `accept_admin` instead.
        pub fn set_admin(env: Env, new_admin: Address) -> Result<(), TokenError> {
            let old_admin = require_admin(&env)?;
            old_admin.require_auth();
            env.storage().instance().set(&DataKey::Admin, &new_admin);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::admin_changed(&env, &old_admin, &new_admin);
            Ok(())
        }

        /// Return the current admin address.
        #[must_use]
        pub fn admin(env: Env) -> Result<Address, TokenError> {
            env.storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(TokenError::NotInitialized)
        }

        /// Return the current total token supply.
        #[must_use]
        pub fn total_supply(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::TotalSupply)
                .unwrap_or(0)
        }

        /// Return `Some(balance)` if `id` has an entry in storage, or `None` if not.
        #[must_use]
        pub fn balance_of(env: Env, id: Address) -> Option<i128> {
            env.storage().persistent().get(&DataKey::Balance(id))
        }

        /// Return the git commit hash baked in at compile time.
        #[must_use]
        pub fn version(env: Env) -> String {
            String::from_str(&env, env!("GIT_HASH"))
        }

        /// Return the on-chain contract version number.
        pub fn contract_version(env: Env) -> u32 {
            env.storage().instance().get(&DataKey::Version).unwrap_or(0)
        }

        /// Return the expiration ledger for an allowance, or `None` if no allowance exists.
        pub fn allowance_expiry(env: Env, from: Address, spender: Address) -> Option<u32> {
            let key = DataKey::Allowance(AllowanceDataKey { from, spender });
            let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
            match val {
                Some(v) if allowance_is_active(env.ledger().sequence(), v.expiration_ledger) => {
                    Some(v.expiration_ledger)
                }
                _ => None,
            }
        }

        /// Record the caller's current balance at `ledger` sequence number.
        ///
        /// Stores `(caller, ledger) → balance` in persistent storage with TTL.
        /// Useful for governance voting-power snapshots (issue #717).
        pub fn snapshot(env: Env, caller: Address, ledger: u32) -> Result<(), TokenError> {
            caller.require_auth();
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Balance(caller.clone()))
                .unwrap_or(0);
            let key = DataKey::Snapshot(caller.clone(), ledger);
            env.storage().persistent().set(&key, &balance);
            extend_ttl_persistent(&env, &key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::snapshot_taken(&env, &caller, ledger, balance);
            Ok(())
        }

        /// Return the balance recorded for `account` at `ledger`, or `None` if no snapshot exists.
        #[must_use]
        pub fn balance_at(env: Env, account: Address, ledger: u32) -> Option<i128> {
            env.storage()
                .persistent()
                .get(&DataKey::Snapshot(account, ledger))
        }

        /// Register (or rotate) the ed25519 public key `owner` will sign permits with.
        /// Owner-authenticated.
        pub fn set_permit_signer(env: Env, owner: Address, public_key: BytesN<32>) {
            permit::set_permit_signer(env, owner, public_key)
        }

        /// Return the ed25519 public key registered for `owner`'s permits, if any.
        #[must_use]
        pub fn permit_signer(env: Env, owner: Address) -> Option<BytesN<32>> {
            permit::permit_signer(env, owner)
        }

        /// Return the nonce `owner` must use in their next `approve_with_signature` call.
        #[must_use]
        pub fn permit_nonce(env: Env, owner: Address) -> u32 {
            permit::permit_nonce(env, owner)
        }

        /// Grant `spender` an allowance over `owner`'s tokens using a pre-signed,
        /// owner-authorized message instead of the owner submitting the transaction
        /// themselves. Analogous to ERC-2612 `permit`.
        ///
        /// The signed message covers `(owner, spender, amount, nonce, expiry_ledger)`;
        /// see [`permit`] for the exact wire format and replay-protection model.
        ///
        /// # Errors
        /// - [`TokenError::PermitExpired`] if the current ledger is past `expiry_ledger`.
        /// - [`TokenError::InvalidNonce`] if `nonce` doesn't match `permit_nonce(owner)`.
        /// - [`TokenError::PermitSignerNotSet`] if `owner` never called `set_permit_signer`.
        ///
        /// # Panics
        /// If `signature` is not a valid ed25519 signature over the expected payload.
        pub fn approve_with_signature(
            env: Env,
            owner: Address,
            spender: Address,
            amount: i128,
            nonce: u32,
            expiry_ledger: u32,
            signature: BytesN<64>,
        ) -> Result<(), TokenError> {
            permit::approve_with_signature(
                env,
                owner,
                spender,
                amount,
                nonce,
                expiry_ledger,
                signature,
            )
        }
    }

    /// Pause / unpause — only compiled when the `pausable` feature is enabled.
    #[cfg(feature = "pausable")]
    #[contractimpl]
    impl TokenContract {
        pub fn pause(env: Env) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &true);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::paused(&env, &admin);
            Ok(())
        }

        pub fn unpause(env: Env) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &false);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::unpaused(&env, &admin);
            Ok(())
        }

        /// Return `true` when the contract is paused, `false` otherwise.
        ///
        /// This is a read-only query — no admin authentication required.
        /// Use this instead of probing `pause`/`unpause` from monitoring scripts.
        #[must_use]
        pub fn is_paused(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false)
        }
    }

    /// Account freeze — only compiled when the `freeze` feature is enabled.
    #[cfg(feature = "freeze")]
    #[contractimpl]
    impl TokenContract {
        pub fn freeze_account(env: Env, account: Address) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::Frozen(account.clone()), &true);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::account_frozen(&env, &account);
            Ok(())
        }

        pub fn unfreeze_account(env: Env, account: Address) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::Frozen(account.clone()), &false);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::account_unfrozen(&env, &account);
            Ok(())
        }
    }

    /// Upgrade path — only compiled when the `upgradeable` feature is enabled.
    #[cfg(feature = "upgradeable")]
    #[contractimpl]
    impl TokenContract {
        /// Delay before an upgrade can execute: 17,280 ledgers, approximately
        /// 24 hours at `soroban_common::LEDGER_SECONDS` seconds per ledger.
        const UPGRADE_DELAY_LEDGERS: u32 = 17_280;

        pub fn propose_upgrade(
            env: Env,
            wasm_hash: soroban_sdk::BytesN<32>,
        ) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            let ready_after = env.ledger().sequence() + Self::UPGRADE_DELAY_LEDGERS;
            env.storage()
                .instance()
                .set(&DataKey::PendingUpgrade, &(wasm_hash.clone(), ready_after));
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "upgrade_proposed"), admin),
                (wasm_hash, ready_after),
            );
            Ok(())
        }

        pub fn execute_upgrade(env: Env) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            let (wasm_hash, ready_after): (soroban_sdk::BytesN<32>, u32) = env
                .storage()
                .instance()
                .get(&DataKey::PendingUpgrade)
                .ok_or(TokenError::Unauthorized)?;
            if env.ledger().sequence() < ready_after {
                return Err(TokenError::Unauthorized);
            }
            env.storage().instance().remove(&DataKey::PendingUpgrade);
            let current_version: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
            let new_version = current_version.checked_add(1).ok_or(TokenError::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::Version, &new_version);
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::upgraded(&env, &admin, &wasm_hash);
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "upgrade_executed"), admin),
                wasm_hash.clone(),
            );
            env.deployer().update_current_contract_wasm(wasm_hash);
            Ok(())
        }
    }

    /// Supply cap — only compiled when the `capped-supply` feature is enabled.
    #[cfg(feature = "capped-supply")]
    #[contractimpl]
    impl TokenContract {
        #[must_use]
        pub fn max_supply(env: Env) -> Option<i128> {
            env.storage().instance().get(&DataKey::MaxSupply)
        }
    }

    /// Transfer hook — only compiled when the `transfer-hook` feature is enabled.
    #[cfg(feature = "transfer-hook")]
    #[contractimpl]
    impl TokenContract {
        /// Set (or clear) the optional transfer hook contract address. Admin only.
        ///
        /// Pass `None` to disable the hook. When set, every transfer will attempt
        /// to call `on_transfer(from, to, amount)` on the hook contract. A failure
        /// in the hook does NOT revert the transfer.
        pub fn set_transfer_hook(env: Env, hook: Option<Address>) -> Result<(), TokenError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            match hook {
                Some(ref addr) => {
                    env.storage().instance().set(&DataKey::TransferHook, addr);
                }
                None => {
                    env.storage().instance().remove(&DataKey::TransferHook);
                }
            }
            extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
            events::transfer_hook_set(&env, &admin, hook.as_ref());
            Ok(())
        }

        /// Return the current transfer hook address, if any.
        pub fn get_transfer_hook(env: Env) -> Option<Address> {
            env.storage().instance().get(&DataKey::TransferHook)
        }
    }

    impl TokenContract {
        pub(crate) fn update_balance(
            env: &Env,
            account: &Address,
            delta: i128,
        ) -> Result<(), TokenError> {
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Balance(account.clone()))
                .unwrap_or(0);
            let new_balance = balance.checked_add(delta).ok_or(TokenError::Overflow)?;
            if new_balance < 0 {
                return Err(TokenError::InsufficientBalance);
            }
            env.storage()
                .persistent()
                .set(&DataKey::Balance(account.clone()), &new_balance);
            extend_ttl_persistent(
                env,
                &DataKey::Balance(account.clone()),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );
            Ok(())
        }

        pub(crate) fn transfer_impl(
            env: &Env,
            from: Address,
            to: Address,
            amount: i128,
        ) -> Result<(), TokenError> {
            if from == to {
                return Ok(());
            }
            if amount <= 0 {
                return Err(TokenError::InvalidAmount);
            }
            Self::update_balance(env, &from, -amount)?;
            Self::update_balance(env, &to, amount)?;
            events::transferred(env, &from, &to, amount);

            // Transfer hook: fire-and-forget, failure does NOT revert the transfer.
            #[cfg(feature = "transfer-hook")]
            {
                if let Some(hook_addr) = env
                    .storage()
                    .instance()
                    .get::<DataKey, Address>(&DataKey::TransferHook)
                {
                    let args = soroban_sdk::vec![
                        env,
                        soroban_sdk::IntoVal::<soroban_sdk::Env, soroban_sdk::Val>::into_val(
                            &from, env
                        ),
                        soroban_sdk::IntoVal::<soroban_sdk::Env, soroban_sdk::Val>::into_val(
                            &to, env
                        ),
                        soroban_sdk::IntoVal::<soroban_sdk::Env, soroban_sdk::Val>::into_val(
                            &amount, env
                        ),
                    ];
                    // Ignore the return value and any error from the hook.
                    let _ = env.try_invoke_contract::<soroban_sdk::Val, soroban_sdk::Error>(
                        &hook_addr,
                        &soroban_sdk::Symbol::new(env, "on_transfer"),
                        args,
                    );
                }
            }

            Ok(())
        }
    }

    #[contractimpl]
    impl token::TokenInterface for TokenContract {
        fn allowance(env: Env, from: Address, spender: Address) -> i128 {
            token_interface::allowance(env, from, spender)
        }
        fn approve(
            env: Env,
            from: Address,
            spender: Address,
            amount: i128,
            expiration_ledger: u32,
        ) {
            token_interface::approve(env, from, spender, amount, expiration_ledger)
        }
        fn balance(env: Env, id: Address) -> i128 {
            token_interface::balance(env, id)
        }
        fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            token_interface::transfer(env, from, to, amount)
        }
        fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
            token_interface::transfer_from(env, spender, from, to, amount)
        }
        fn burn(env: Env, from: Address, amount: i128) {
            token_interface::burn(env, from, amount)
        }
        fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
            token_interface::burn_from(env, spender, from, amount)
        }
        fn decimals(env: Env) -> u32 {
            token_interface::decimals(env)
        }
        fn name(env: Env) -> String {
            token_interface::name(env)
        }
        fn symbol(env: Env) -> String {
            token_interface::symbol(env)
        }
    }
}
