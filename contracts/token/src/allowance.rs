//! Allowance read/write/deduction helpers for the token contract.
//!
//! All three operations are centralised here so that `approve`, `transfer_from`,
//! and `burn_from` share a single code path, eliminating the duplication that
//! previously existed across `token_interface.rs`.

use soroban_sdk::{Address, Env, panic_with_error};

use crate::errors::TokenError;
use crate::storage::{AllowanceDataKey, AllowanceValue, DataKey};

/// Soroban ledgers are expected roughly every `soroban_common::LEDGER_SECONDS` seconds on public networks.
/// Allowance expiration is ledger-exact, so no additional wall-clock grace is
/// applied beyond the caller-provided `expiration_ledger` (~0 seconds).
const ALLOWANCE_EXPIRY_GRACE_LEDGERS: u32 = 0;

/// Sentinel for an allowance that is already expired or absent.
///
/// Ledger 0 is genesis-time and therefore represents no future duration
/// (~0 seconds) for active approvals.
const EXPIRED_ALLOWANCE_LEDGER: u32 = 0;

pub(crate) fn allowance_is_active(current_ledger: u32, expiration_ledger: u32) -> bool {
    current_ledger <= expiration_ledger.saturating_add(ALLOWANCE_EXPIRY_GRACE_LEDGERS)
}

/// Returns the active allowance (amount) for `(from, spender)`, or `0` if the
/// allowance is absent or expired.
pub fn get_allowance(env: &Env, from: soroban_sdk::Address, spender: soroban_sdk::Address) -> i128 {
    let key = DataKey::Allowance(AllowanceDataKey { from, spender });
    let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
    match val {
        Some(v) if allowance_is_active(env.ledger().sequence(), v.expiration_ledger) => v.amount,
        _ => 0,
    }
}

/// Writes a new allowance entry.  Extends the TTL when `expiration_ledger` is in
/// the future so the entry survives until the approval expires.
pub fn set_allowance(
    env: &Env,
    from: soroban_sdk::Address,
    spender: soroban_sdk::Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let key = DataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    env.storage().temporary().set(
        &key,
        &AllowanceValue {
            amount,
            expiration_ledger,
        },
    );
    if expiration_ledger > env.ledger().sequence() {
        env.storage()
            .temporary()
            .extend_ttl(&key, expiration_ledger, expiration_ledger);
    }
}

/// Validates that `spender` has sufficient allowance from `from` and deducts
/// `amount`, returning `Err(TokenError::InsufficientAllowance)` instead of
/// panicking. The original `expiration_ledger` is preserved so the TTL is not
/// accidentally reset.
pub(crate) fn validate_and_deduct_allowance(
    env: &Env,
    from: soroban_sdk::Address,
    spender: soroban_sdk::Address,
    amount: i128,
) -> Result<(), TokenError> {
    let key = DataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
    let (current, expiration_ledger) = match val {
        Some(v) if allowance_is_active(env.ledger().sequence(), v.expiration_ledger) => {
            (v.amount, v.expiration_ledger)
        }
        _ => (0, EXPIRED_ALLOWANCE_LEDGER),
    };
    if current < amount {
        return Err(TokenError::InsufficientAllowance);
    }
    env.storage().temporary().set(
        &key,
        &AllowanceValue {
            amount: current - amount,
            expiration_ledger,
        },
    );
    Ok(())
}

/// Deducts `amount` from the `(from, spender)` allowance, **preserving the
/// original `expiration_ledger`** so the entry's TTL is not accidentally reset.
///
/// Panics with [`TokenError::InsufficientAllowance`] when the active allowance
/// is less than `amount`.
pub fn deduct_allowance(
    env: &Env,
    from: soroban_sdk::Address,
    spender: soroban_sdk::Address,
    amount: i128,
) {
    if let Err(e) = validate_and_deduct_allowance(env, from, spender, amount) {
        panic_with_error!(env, e);
    }
}

/// Reads the active allowance together with its expiration ledger, defaulting
/// to `(0, EXPIRED_ALLOWANCE_LEDGER)` when the entry is absent or expired.
fn read_active_allowance(
    env: &Env,
    from: &Address,
    spender: &Address,
) -> (i128, u32) {
    let key = DataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
    match val {
        Some(v) if allowance_is_active(env.ledger().sequence(), v.expiration_ledger) => {
            (v.amount, v.expiration_ledger)
        }
        _ => (0, EXPIRED_ALLOWANCE_LEDGER),
    }
}

/// Atomically increases the `(from, spender)` allowance by `delta`.
///
/// Requires authorization from `from`. Uses checked arithmetic and returns
/// [`TokenError::Overflow`] if the resulting allowance would exceed `i128::MAX`.
/// The original `expiration_ledger` is preserved so the TTL is not reset.
pub fn increase_allowance(
    env: &Env,
    from: Address,
    spender: Address,
    delta: i128,
) -> Result<(), TokenError> {
    from.require_auth();

    let (current, expiration_ledger) = read_active_allowance(env, &from, &spender);
    let new_amount = current.checked_add(delta).ok_or(TokenError::Overflow)?;

    set_allowance(env, from, spender, new_amount, expiration_ledger);
    Ok(())
}

/// Atomically decreases the `(from, spender)` allowance by `delta`.
///
/// Requires authorization from `from`. Uses checked arithmetic and returns
/// [`TokenError::InsufficientAllowance`] if `delta` exceeds the current active
/// allowance. The original `expiration_ledger` is preserved so the TTL is not
/// reset.
pub fn decrease_allowance(
    env: &Env,
    from: Address,
    spender: Address,
    delta: i128,
) -> Result<(), TokenError> {
    from.require_auth();

    let (current, expiration_ledger) = read_active_allowance(env, &from, &spender);
    let new_amount = current
        .checked_sub(delta)
        .ok_or(TokenError::InsufficientAllowance)?;

    set_allowance(env, from, spender, new_amount, expiration_ledger);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let from = Address::generate(&env);
        let spender = Address::generate(&env);
        (env, from, spender)
    }

    #[test]
    fn increase_allowance_adds_to_existing() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 100, expiration);

        increase_allowance(&env, from.clone(), spender.clone(), 50).unwrap();

        assert_eq!(get_allowance(&env, from, spender), 150);
    }

    #[test]
    fn increase_allowance_from_zero() {
        let (env, from, spender) = setup();

        increase_allowance(&env, from.clone(), spender.clone(), 25).unwrap();

        assert_eq!(get_allowance(&env, from, spender), 25);
    }

    #[test]
    fn increase_allowance_overflow_returns_error() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), i128::MAX, expiration);

        let result = increase_allowance(&env, from.clone(), spender.clone(), 1);

        assert_eq!(result, Err(TokenError::Overflow));
        assert_eq!(get_allowance(&env, from, spender), i128::MAX);
    }

    #[test]
    fn decrease_allowance_subtracts_from_existing() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 100, expiration);

        decrease_allowance(&env, from.clone(), spender.clone(), 40).unwrap();

        assert_eq!(get_allowance(&env, from, spender), 60);
    }

    #[test]
    fn decrease_allowance_to_zero() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 100, expiration);

        decrease_allowance(&env, from.clone(), spender.clone(), 100).unwrap();

        assert_eq!(get_allowance(&env, from, spender), 0);
    }

    #[test]
    fn decrease_allowance_underflow_returns_error() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 10, expiration);

        let result = decrease_allowance(&env, from.clone(), spender.clone(), 11);

        assert_eq!(result, Err(TokenError::InsufficientAllowance));
        assert_eq!(get_allowance(&env, from, spender), 10);
    }

    #[test]
    fn decrease_allowance_absent_returns_error() {
        let (env, from, spender) = setup();

        let result = decrease_allowance(&env, from.clone(), spender.clone(), 1);

        assert_eq!(result, Err(TokenError::InsufficientAllowance));
    }

    #[test]
    fn zero_delta_is_noop() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 42, expiration);

        increase_allowance(&env, from.clone(), spender.clone(), 0).unwrap();
        assert_eq!(get_allowance(&env, from.clone(), spender.clone()), 42);

        decrease_allowance(&env, from.clone(), spender.clone(), 0).unwrap();
        assert_eq!(get_allowance(&env, from, spender), 42);
    }

    #[test]
    fn adjustments_preserve_expiration() {
        let (env, from, spender) = setup();
        let expiration = env.ledger().sequence() + 100;
        set_allowance(&env, from.clone(), spender.clone(), 100, expiration);

        increase_allowance(&env, from.clone(), spender.clone(), 10).unwrap();
        decrease_allowance(&env, from.clone(), spender.clone(), 20).unwrap();

        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        let val: AllowanceValue = env.storage().temporary().get(&key).unwrap();
        assert_eq!(val.amount, 90);
        assert_eq!(val.expiration_ledger, expiration);
    }
}
