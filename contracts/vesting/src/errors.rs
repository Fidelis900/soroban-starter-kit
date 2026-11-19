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
    InvalidScheduleBounds = 14,
    NotMilestoneSchedule = 15,
    MilestoneNotFound = 16,
    MilestoneAlreadyApproved = 17,
}

impl_display_error!(
    VestingError,
    AlreadyInitialized => "already initialized",
    NotInitialized => "not initialized",
    NotAuthorized => "not authorized",
    InvalidAmount => "invalid amount",
    InvalidSchedule => "invalid schedule",
    NothingToClaim => "nothing to claim",
    AlreadyRevoked => "already revoked",
    CliffAlreadyPassed => "cliff already passed",
    ScheduleAlreadyExists => "schedule already exists",
    ScheduleNotFound => "schedule not found",
    ArithmeticError => "arithmetic error",
    ScheduleIrrevocable => "schedule irrevocable",
    BeneficiaryAlreadyExists => "beneficiary already exists",
    InvalidScheduleBounds => "invalid schedule bounds",
    NotMilestoneSchedule => "not a milestone schedule",
    MilestoneNotFound => "milestone not found",
    MilestoneAlreadyApproved => "milestone already approved",
);
