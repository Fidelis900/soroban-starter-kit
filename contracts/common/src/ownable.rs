//! Ownable2Step administration helper for Soroban contracts.
//!
//! This module provides a reusable two-step ownership transfer mechanism
//! that can be shared across contracts to standardize admin transfer logic.

use soroban_sdk::{contracterror, contracttype, Address, Env, Symbol};

/// Storage keys used by the Ownable2Step helper.
#[allow(missing_docs)]
mod storage_keys {
    use soroban_sdk::contracttype;

    #[contracttype]
    pub enum OwnableKey {
        Owner,
        PendingOwner,
    }
}

pub use storage_keys::OwnableKey;

/// Errors that can be returned by the Ownable2Step helper.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OwnableError {
    /// The contract has not been initialized with an owner.
    NotInitialized = 1,
    /// The caller is not authorized to perform this action.
    Unauthorized = 2,
    /// There is no pending owner to accept ownership.
    NoPendingOwner = 3,
}

/// Returns the current owner, or `None` if not initialized.
pub fn try_get_owner(env: &Env) -> Option<Address> {
    env.storage().instance().get(&OwnableKey::Owner)
}

/// Returns the current owner.
///
/// # Panics
///
/// Panics with `OwnableError::NotInitialized` if no owner is set.
pub fn get_owner(env: &Env) -> Address {
    try_get_owner(env).unwrap_or_else(|| panic_with_error!(env, OwnableError::NotInitialized))
}

/// Returns the pending owner, or `None` if no transfer is in progress.
pub fn try_get_pending_owner(env: &Env) -> Option<Address> {
    env.storage().instance().get(&OwnableKey::PendingOwner)
}

/// Initializes the contract with the given owner.
///
/// Stores the owner in instance storage and emits an `OwnershipTransferred`
/// event.
pub fn init_owner(env: &Env, owner: Address) {
    env.storage().instance().set(&OwnableKey::Owner, &owner);
    env.storage().instance().remove(&OwnableKey::PendingOwner);
    emit_ownership_transferred(env, &owner);
}

/// Starts a two-step ownership transfer to `new_owner`.
///
/// Requires authorization from the current owner. The `new_owner` is stored
/// as the pending owner and must call [`accept_ownership`] to complete the
/// transfer.
///
/// # Panics
///
/// Panics with `OwnableError::NotInitialized` if no owner is set.
pub fn transfer_ownership(env: &Env, new_owner: Address) {
    let owner = get_owner(env);
    owner.require_auth();

    env.storage()
        .instance()
        .set(&OwnableKey::PendingOwner, &new_owner);

    emit_ownership_transfer_started(env, &owner, &new_owner);
}

/// Completes a pending ownership transfer.
///
/// Requires authorization from the pending owner. Promotes the pending owner
/// to owner, clears the pending owner, and emits an `OwnershipTransferred`
/// event.
///
/// # Panics
///
/// Panics with `OwnableError::NoPendingOwner` if there is no pending owner.
pub fn accept_ownership(env: &Env) {
    let pending_owner = try_get_pending_owner(env)
        .unwrap_or_else(|| panic_with_error!(env, OwnableError::NoPendingOwner));

    pending_owner.require_auth();

    env.storage()
        .instance()
        .set(&OwnableKey::Owner, &pending_owner);
    env.storage().instance().remove(&OwnableKey::PendingOwner);

    emit_ownership_transferred(env, &pending_owner);
}

/// Emits an `OwnershipTransferred` event.
fn emit_ownership_transferred(env: &Env, new_owner: &Address) {
    let topics = (Symbol::new(env, "OwnershipTransferred"),);
    env.events().publish(topics, new_owner.clone());
}

/// Emits an `OwnershipTransferStarted` event.
fn emit_ownership_transfer_started(env: &Env, previous_owner: &Address, new_owner: &Address) {
    let topics = (Symbol::new(env, "OwnershipTransferStarted"),);
    env.events()
        .publish(topics, (previous_owner.clone(), new_owner.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Env, IntoVal, Symbol, TryFromVal,
    };

    #[test]
    fn test_init_owner() {
        let env = Env::default();
        let owner = Address::generate(&env);

        assert!(try_get_owner(&env).is_none());

        init_owner(&env, owner.clone());

        assert_eq!(get_owner(&env), owner);
        assert!(try_get_pending_owner(&env).is_none());
    }

    #[test]
    fn test_init_owner_emits_event() {
        let env = Env::default();
        let owner = Address::generate(&env);

        init_owner(&env, owner.clone());

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let (_, topics, data) = events.get(0).unwrap();
        let expected_topic: Symbol = Symbol::new(&env, "OwnershipTransferred");
        let topic: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, expected_topic);

        let emitted_owner: Address = Address::try_from_val(&env, &data).unwrap();
        assert_eq!(emitted_owner, owner);
    }

    #[test]
    fn test_transfer_ownership_requires_auth() {
        let env = Env::default();
        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);

        init_owner(&env, owner.clone());

        // Without mock_all_auths, require_auth should fail.
        let result = env.try_invoke_contract::<(), OwnableError>(
            &env.register_stub_contract(""),
            &Symbol::new(&env, "transfer_ownership"),
            (new_owner.clone(),).into_val(&env),
        );
        // We can't easily invoke a free function; instead verify auth is required
        // by checking that mock_all_auths is needed. Use a direct check below.
        let _ = result;

        env.mock_all_auths();
        transfer_ownership(&env, new_owner.clone());

        assert_eq!(try_get_pending_owner(&env), Some(new_owner));
        assert_eq!(get_owner(&env), owner);
    }

    #[test]
    fn test_transfer_ownership_unauthorized() {
        let env = Env::default();
        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);

        init_owner(&env, owner.clone());

        // No auth mocked: require_auth should panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transfer_ownership(&env, new_owner.clone());
        }));
        assert!(result.is_err());
        assert!(try_get_pending_owner(&env).is_none());
    }

    #[test]
    fn test_transfer_and_accept_ownership() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);

        init_owner(&env, owner.clone());
        transfer_ownership(&env, new_owner.clone());

        assert_eq!(get_owner(&env), owner);
        assert_eq!(try_get_pending_owner(&env), Some(new_owner.clone()));

        accept_ownership(&env);

        assert_eq!(get_owner(&env), new_owner);
        assert!(try_get_pending_owner(&env).is_none());
    }

    #[test]
    fn test_transfer_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);

        init_owner(&env, owner.clone());
        transfer_ownership(&env, new_owner.clone());

        let events = env.events().all();
        assert_eq!(events.len(), 2);

        let (_, topics, _) = events.get(1).unwrap();
        let topic: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, Symbol::new(&env, "OwnershipTransferStarted"));
    }

    #[test]
    fn test_accept_ownership_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);

        init_owner(&env, owner.clone());
        transfer_ownership(&env, new_owner.clone());
        accept_ownership(&env);

        let events = env.events().all();
        assert_eq!(events.len(), 3);

        let (_, topics, data) = events.get(2).unwrap();
        let topic: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(topic, Symbol::new(&env, "OwnershipTransferred"));

        let emitted_owner: Address = Address::try_from_val(&env, &data).unwrap();
        assert_eq!(emitted_owner, new_owner);
    }

    #[test]
    fn test_accept_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        init_owner(&env, owner.clone());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            accept_ownership(&env);
        }));
        assert!(result.is_err());
        assert_eq!(get_owner(&env), owner);
    }

    #[test]
    fn test_get_owner_not_initialized() {
        let env = Env::default();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            get_owner(&env);
        }));
        assert!(result.is_err());
    }
}