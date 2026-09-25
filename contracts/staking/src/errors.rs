// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StakingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    NoStake = 5,
    InsufficientStake = 6,
    NoRewards = 7,
    CompoundTokenMismatch = 8,
    UnbondingNotComplete = 9,
    NoUnbondRequest = 10,
    UnbondRequestPending = 11,
    InvalidTranche = 12,
    InvalidPenalty = 12,
}

impl_display_error!(
    StakingError,
    AlreadyInitialized     => "already initialized",
    NotInitialized         => "not initialized",
    Unauthorized           => "not authorized",
    InvalidAmount          => "invalid amount",
    NoStake                => "no stake",
    InsufficientStake      => "insufficient stake",
    NoRewards              => "no rewards",
    CompoundTokenMismatch  => "compound token mismatch",
    UnbondingNotComplete   => "unbonding not complete",
    NoUnbondRequest        => "no unbond request",
    UnbondRequestPending   => "unbond request pending",
    InvalidTranche         => "invalid unbond tranche",
    InvalidPenalty         => "invalid penalty",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::StakingError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
StakingError::AlreadyInitialized = {}\n\
StakingError::NotInitialized = {}\n\
StakingError::Unauthorized = {}\n\
StakingError::InvalidAmount = {}\n\
StakingError::NoStake = {}\n\
StakingError::InsufficientStake = {}\n\
StakingError::NoRewards = {}\n\
StakingError::CompoundTokenMismatch = {}\n\
StakingError::UnbondingNotComplete = {}\n\
StakingError::NoUnbondRequest = {}\n\
StakingError::UnbondRequestPending = {}\n\
StakingError::InvalidTranche = {}\n",
StakingError::InvalidPenalty = {}\n",
            StakingError::AlreadyInitialized as u32,
            StakingError::NotInitialized as u32,
            StakingError::Unauthorized as u32,
            StakingError::InvalidAmount as u32,
            StakingError::NoStake as u32,
            StakingError::InsufficientStake as u32,
            StakingError::NoRewards as u32,
            StakingError::CompoundTokenMismatch as u32,
            StakingError::UnbondingNotComplete as u32,
            StakingError::NoUnbondRequest as u32,
            StakingError::UnbondRequestPending as u32,
            StakingError::InvalidTranche as u32,
            StakingError::InvalidPenalty as u32,
        )
    }

    #[test]
    fn staking_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
