//! Standardized pausable functionality for Soroban contracts.
//!
//! This module provides a unified way to implement pause/unpause logic across
//! contracts in the workspace. It standardizes storage keys, error types, and
//! event emissions.

use soroban_sdk::{contracterror, contracttype, Env, Symbol};

/// Storage keys used by the pausable module.
mod storage {
    #![allow(missing_docs)]

    use soroban_sdk::contracttype;

    #[contracttype]
    #[derive(Clone)]
    pub enum PausableKey {
        Paused,
    }
}

use storage::PausableKey;

/// Errors that can be returned by pausable functions.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PausableError {
    /// The contract is currently paused.
    Paused = 1,
}

/// Sets the paused state of the contract and emits a standardized event.
///
/// # Arguments
///
/// * `env` - The contract environment.
/// * `paused` - The new paused state. `true` to pause, `false` to unpause.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&PausableKey::Paused, &paused);

    let action = if paused {
        Symbol::new(env, "Paused")
    } else {
        Symbol::new(env, "Unpaused")
    };

    env.events().publish(
        (Symbol::new(env, "Pausable"), action),
        (),
    );
}

/// Returns whether the contract is currently paused.
///
/// Defaults to `false` if the paused flag has never been set.
///
/// # Arguments
///
/// * `env` - The contract environment.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&PausableKey::Paused)
        .unwrap_or(false)
}

/// Requires that the contract is not paused.
///
/// # Arguments
///
/// * `env` - The contract environment.
///
/// # Errors
///
/// Returns [`PausableError::Paused`] if the contract is currently paused.
pub fn require_not_paused(env: &Env) -> Result<(), PausableError> {
    if is_paused(env) {
        Err(PausableError::Paused)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{Env, IntoVal, Symbol, TryFromVal};

    #[test]
    fn default_is_unpaused() {
        let env = Env::default();
        assert!(!is_paused(&env));
    }

    #[test]
    fn set_paused_toggles_state() {
        let env = Env::default();

        set_paused(&env, true);
        assert!(is_paused(&env));

        set_paused(&env, false);
        assert!(!is_paused(&env));
    }

    #[test]
    fn require_not_paused_succeeds_when_unpaused() {
        let env = Env::default();
        assert_eq!(require_not_paused(&env), Ok(()));
    }

    #[test]
    fn require_not_paused_fails_when_paused() {
        let env = Env::default();
        set_paused(&env, true);
        assert_eq!(require_not_paused(&env), Err(PausableError::Paused));
    }

    #[test]
    fn emits_paused_event() {
        let env = Env::default();
        set_paused(&env, true);

        let events = env.events().all();
        let last = events.last().unwrap();

        let expected_topics = (
            Symbol::new(&env, "Pausable"),
            Symbol::new(&env, "Paused"),
        )
            .into_val(&env);

        assert_eq!(last.1, expected_topics);
        assert_eq!(last.2, ().into_val(&env));
    }

    #[test]
    fn emits_unpaused_event() {
        let env = Env::default();
        set_paused(&env, false);

        let events = env.events().all();
        let last = events.last().unwrap();

        let expected_topics = (
            Symbol::new(&env, "Pausable"),
            Symbol::new(&env, "Unpaused"),
        )
            .into_val(&env);

        assert_eq!(last.1, expected_topics);
        assert_eq!(last.2, ().into_val(&env));
    }

    #[test]
    fn event_topics_are_symbols() {
        let env = Env::default();
        set_paused(&env, true);

        let events = env.events().all();
        let last = events.last().unwrap();

        let topics: soroban_sdk::Vec<soroban_sdk::Val> = last.1;
        assert_eq!(topics.len(), 2);

        let first = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        let second = Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap();

        assert_eq!(first, Symbol::new(&env, "Pausable"));
        assert_eq!(second, Symbol::new(&env, "Paused"));
    }
}