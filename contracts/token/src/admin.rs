use crate::errors::TokenError;
use crate::storage::DataKey;
use soroban_sdk::{Address, Env};

pub fn require_admin(env: &Env) -> Result<Address, TokenError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(TokenError::NotInitialized)
}

/// Immediately transfers administrative control to `new_admin`.
///
/// Kept for backward compatibility. Prefer the two-step
/// `propose_admin` / `accept_admin` flow to avoid permanently losing
/// contract ownership due to a mistyped address.
pub fn set_admin(env: &Env, new_admin: &Address) -> Result<(), TokenError> {
    let current = require_admin(env)?;
    current.require_auth();
    env.storage().instance().set(&DataKey::Admin, new_admin);
    Ok(())
}

/// Proposes `new_admin` as the next administrator.
///
/// Requires the current admin's authorization. The proposal must be
/// accepted by `new_admin` via `accept_admin` before ownership moves.
pub fn propose_admin(env: &Env, new_admin: &Address) -> Result<(), TokenError> {
    let current = require_admin(env)?;
    current.require_auth();
    env.storage()
        .instance()
        .set(&DataKey::PendingAdmin, new_admin);
    Ok(())
}

/// Accepts a pending admin proposal, transferring ownership to the caller.
///
/// Requires the proposed admin's authorization.
pub fn accept_admin(env: &Env) -> Result<(), TokenError> {
    let pending: Address = env
        .storage()
        .instance()
        .get(&DataKey::PendingAdmin)
        .ok_or(TokenError::NotInitialized)?;
    pending.require_auth();
    env.storage().instance().set(&DataKey::Admin, &pending);
    env.storage().instance().remove(&DataKey::PendingAdmin);
    Ok(())
}

/// Cancels a pending admin proposal.
///
/// Requires the current admin's authorization.
pub fn cancel_admin_proposal(env: &Env) -> Result<(), TokenError> {
    let current = require_admin(env)?;
    current.require_auth();
    env.storage().instance().remove(&DataKey::PendingAdmin);
    Ok(())
}

/// Sets a transfer lockup deadline (ledger sequence) for `account`.
///
/// Requires the current admin's authorization. Transfers from `account`
/// are rejected until `env.ledger().sequence() > lock_ledger`.
pub fn set_transfer_lock(
    env: &Env,
    account: &Address,
    lock_ledger: u32,
) -> Result<(), TokenError> {
    let current = require_admin(env)?;
    current.require_auth();
    env.storage()
        .persistent()
        .set(&DataKey::TransferLock(account.clone()), &lock_ledger);
    Ok(())
}

/// Ensures `account` is not currently under a transfer lockup.
///
/// Returns `TokenError::AccountLocked` if the account's lockup deadline
/// has not yet passed.
pub fn require_not_locked(env: &Env, account: &Address) -> Result<(), TokenError> {
    if let Some(lock_ledger) = env
        .storage()
        .persistent()
        .get::<DataKey, u32>(&DataKey::TransferLock(account.clone()))
    {
        if env.ledger().sequence() <= lock_ledger {
            return Err(TokenError::AccountLocked);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
        (env, admin, other)
    }

    #[test]
    fn test_two_step_transfer_success() {
        let (env, admin, new_admin) = setup();

        propose_admin(&env, &new_admin).unwrap();
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap();
        assert_eq!(pending, new_admin);

        accept_admin(&env).unwrap();

        let current: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(current, new_admin);
        assert!(env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::PendingAdmin)
            .is_none());
        assert_ne!(current, admin);
    }

    #[test]
    fn test_propose_requires_current_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);

        // No auths mocked: require_auth should fail.
        let result = propose_admin(&env, &new_admin);
        assert!(result.is_err());
    }

    #[test]
    fn test_accept_requires_pending_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        // No auths mocked: require_auth should fail.
        let result = accept_admin(&env);
        assert!(result.is_err());

        // Admin should be unchanged.
        let current: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(current, admin);
    }

    #[test]
    fn test_accept_without_proposal_fails() {
        let (env, _admin, _other) = setup();
        let result = accept_admin(&env);
        assert!(result.is_err());
    }

    #[test]
    fn test_cancel_admin_proposal() {
        let (env, admin, new_admin) = setup();

        propose_admin(&env, &new_admin).unwrap();
        cancel_admin_proposal(&env).unwrap();

        assert!(env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::PendingAdmin)
            .is_none());

        let current: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(current, admin);

        // Accepting after cancellation should fail.
        assert!(accept_admin(&env).is_err());
    }

    #[test]
    fn test_set_transfer_lock_and_enforcement() {
        let (env, _admin, account) = setup();
        env.ledger().set_sequence_number(100);

        set_transfer_lock(&env, &account, 200).unwrap();

        // Before expiration: locked.
        assert_eq!(
            require_not_locked(&env, &account),
            Err(TokenError::AccountLocked)
        );

        // At the deadline: still locked.
        env.ledger().set_sequence_number(200);
        assert_eq!(
            require_not_locked(&env, &account),
            Err(TokenError::AccountLocked)
        );

        // After expiration: unlocked.
        env.ledger().set_sequence_number(201);
        assert_eq!(require_not_locked(&env, &account), Ok(()));
    }

    #[test]
    fn test_unlocked_account_passes() {
        let (env, _admin, account) = setup();
        env.ledger().set_sequence_number(100);
        assert_eq!(require_not_locked(&env, &account), Ok(()));
    }

    #[test]
    fn test_set_transfer_lock_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let account = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);

        // No auths mocked: require_auth should fail.
        let result = set_transfer_lock(&env, &account, 200);
        assert!(result.is_err());
    }
}