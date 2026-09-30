//! TokenInterface method implementations.
//!
//! Each function here maps 1-to-1 to a method on `token::TokenInterface`.
//! `lib.rs` hosts the `#[contractimpl]` block and delegates to these functions.

use soroban_sdk::{Address, Env, MuxedAddress, String, panic_with_error};

use crate::TokenContract;
use crate::allowance::{get_allowance, set_allowance, validate_and_deduct_allowance};
use crate::errors::TokenError;
use crate::events;
use crate::storage::{DataKey, MetadataKey};
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};

#[cfg(feature = "pausable")]
use crate::require_not_paused;

#[cfg(feature = "freeze")]
use crate::require_not_frozen;

pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
    get_allowance(&env, from, spender)
}

pub fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
    from.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    set_allowance(
        &env,
        from.clone(),
        spender.clone(),
        amount,
        expiration_ledger,
    );
    if amount == 0 {
        events::revoked(&env, &from, &spender);
    } else {
        events::approved(&env, &from, &spender, amount);
    }
}

pub fn balance(env: Env, id: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Balance(id))
        .unwrap_or(0)
}

pub fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
    from.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    #[cfg(feature = "freeze")]
    if let Err(e) = require_not_frozen(&env, &from) {
        panic_with_error!(&env, e);
    }
    let to_address: Address = to.address();
    if let Err(e) = TokenContract::transfer_impl(&env, from, to_address, amount) {
        panic_with_error!(&env, e);
    }
}

pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
    spender.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    #[cfg(feature = "freeze")]
    if let Err(e) = require_not_frozen(&env, &from) {
        panic_with_error!(&env, e);
    }
    if let Err(e) = validate_and_deduct_allowance(&env, from.clone(), spender.clone(), amount) {
        panic_with_error!(&env, e);
    }
    if let Err(e) = TokenContract::transfer_impl(&env, from, to, amount) {
        panic_with_error!(&env, e);
    }
}

pub fn burn(env: Env, from: Address, amount: i128) {
    from.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    #[cfg(feature = "freeze")]
    if let Err(e) = require_not_frozen(&env, &from) {
        panic_with_error!(&env, e);
    }
    if amount <= 0 {
        panic_with_error!(&env, TokenError::InvalidAmount);
    }
    if let Err(e) = TokenContract::update_balance(&env, &from, -amount) {
        panic_with_error!(&env, e);
    }
    let supply: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0);
    let new_supply = match supply.checked_sub(amount) {
        Some(v) => v,
        None => panic_with_error!(&env, TokenError::Overflow),
    };
    env.storage()
        .instance()
        .set(&DataKey::TotalSupply, &new_supply);
    extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    events::burned(&env, &from, amount);
}

pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
    spender.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    #[cfg(feature = "freeze")]
    if let Err(e) = require_not_frozen(&env, &from) {
        panic_with_error!(&env, e);
    }
    if let Err(e) = validate_and_deduct_allowance(&env, from.clone(), spender.clone(), amount) {
        panic_with_error!(&env, e);
    }
    if let Err(e) = TokenContract::update_balance(&env, &from, -amount) {
        panic_with_error!(&env, e);
    }
    let supply: i128 = env
        .storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0);
    let new_supply = match supply.checked_sub(amount) {
        Some(v) => v,
        None => panic_with_error!(&env, TokenError::Overflow),
    };
    env.storage()
        .instance()
        .set(&DataKey::TotalSupply, &new_supply);
    extend_ttl_instance(&env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
    events::burned(&env, &from, amount);
}

pub fn decimals(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::Metadata(MetadataKey::Decimals))
        .unwrap_or_default()
}

pub fn name(env: Env) -> String {
    env.storage()
        .instance()
        .get(&DataKey::Metadata(MetadataKey::Name))
        .unwrap_or_else(|| String::from_str(&env, ""))
}

pub fn symbol(env: Env) -> String {
    env.storage()
        .instance()
        .get(&DataKey::Metadata(MetadataKey::Symbol))
        .unwrap_or_else(|| String::from_str(&env, ""))
}

/// Batch transfer from `from` to multiple recipients.
/// Authenticated by `from`.
pub fn transfer_batch(env: Env, from: Address, transfers: soroban_sdk::Vec<(Address, i128)>) {
    from.require_auth();
    #[cfg(feature = "pausable")]
    if let Err(e) = require_not_paused(&env) {
        panic_with_error!(&env, e);
    }
    #[cfg(feature = "freeze")]
    if let Err(e) = require_not_frozen(&env, &from) {
        panic_with_error!(&env, e);
    }
    if let Err(e) = TokenContract::transfer_batch(env.clone(), from, transfers) {
        panic_with_error!(&env, e);
    }
}
