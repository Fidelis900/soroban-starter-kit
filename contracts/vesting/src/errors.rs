// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VestingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    InvalidAmount = 4,
    InvalidSchedule = 5,
    NothingToClaim = 6,
    AlreadyRevoked = 7,
    CliffAlreadyPassed = 8,
    ScheduleAlreadyExists = 9,
    ScheduleNotFound = 10,
    ArithmeticError = 11,
    ScheduleIrrevocable = 12,
    BeneficiaryAlreadyExists = 13,
    RevocationPending = 11,
    RevocationNotPending = 12,
    RevocationDelayNotElapsed = 13,
}

impl_display_error!(
    VestingError,
    AlreadyInitialized    => "already initialized",
    NotInitialized        => "not initialized",
    NotAuthorized         => "not authorized",
    InvalidAmount         => "invalid amount",
    InvalidSchedule       => "invalid schedule",
    NothingToClaim        => "nothing to claim",
    AlreadyRevoked        => "already revoked",
    CliffAlreadyPassed    => "cliff already passed",
    ScheduleAlreadyExists => "schedule already exists",
    ScheduleNotFound      => "schedule not found",
    ArithmeticError       => "arithmetic error",
    ScheduleIrrevocable   => "schedule irrevocable",
    BeneficiaryAlreadyExists => "beneficiary already exists",
    AlreadyInitialized       => "already initialized",
    NotInitialized           => "not initialized",
    NotAuthorized            => "not authorized",
    InvalidAmount            => "invalid amount",
    InvalidSchedule          => "invalid schedule",
    NothingToClaim           => "nothing to claim",
    AlreadyRevoked           => "already revoked",
    CliffAlreadyPassed       => "cliff already passed",
    ScheduleAlreadyExists    => "schedule already exists",
    ScheduleNotFound         => "schedule not found",
    RevocationPending        => "revocation pending",
    RevocationNotPending     => "revocation not pending",
    RevocationDelayNotElapsed => "revocation delay not elapsed",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::VestingError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
VestingError::AlreadyInitialized = {}\n\
VestingError::NotInitialized = {}\n\
VestingError::NotAuthorized = {}\n\
VestingError::InvalidAmount = {}\n\
VestingError::InvalidSchedule = {}\n\
VestingError::NothingToClaim = {}\n\
VestingError::AlreadyRevoked = {}\n\
VestingError::CliffAlreadyPassed = {}\n\
VestingError::ScheduleAlreadyExists = {}\n\
VestingError::ScheduleNotFound = {}\n\
VestingError::ArithmeticError = {}\n\
VestingError::ScheduleIrrevocable = {}\n\
VestingError::BeneficiaryAlreadyExists = {}\n",
VestingError::RevocationPending = {}\n\
VestingError::RevocationNotPending = {}\n\
VestingError::RevocationDelayNotElapsed = {}\n",
            VestingError::AlreadyInitialized as u32,
            VestingError::NotInitialized as u32,
            VestingError::NotAuthorized as u32,
            VestingError::InvalidAmount as u32,
            VestingError::InvalidSchedule as u32,
            VestingError::NothingToClaim as u32,
            VestingError::AlreadyRevoked as u32,
            VestingError::CliffAlreadyPassed as u32,
            VestingError::ScheduleAlreadyExists as u32,
            VestingError::ScheduleNotFound as u32,
            VestingError::ArithmeticError as u32,
            VestingError::ScheduleIrrevocable as u32,
            VestingError::BeneficiaryAlreadyExists as u32,
            VestingError::RevocationPending as u32,
            VestingError::RevocationNotPending as u32,
            VestingError::RevocationDelayNotElapsed as u32,
        )
    }

    #[test]
    fn vesting_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
