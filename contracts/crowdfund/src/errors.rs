// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum CrowdfundError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    DeadlinePassed = 3,
    DeadlineNotReached = 4,
    GoalAlreadyMet = 5,
    GoalNotMet = 6,
    AlreadyClaimed = 7,
    NothingToPledge = 8,
    NothingToWithdraw = 9,
    InvalidAmount = 10,
    InvalidDeadline = 11,
    InvalidGoal = 12,
    NotAuthorized = 13,
    InvalidTier = 14,
    PledgeCapExceeded = 15,
    DeadlineAlreadyExtended = 16,
    ClaimDeadlineExpired = 17,
    Overflow = 18,
    MilestoneNotFound = 19,
    MilestoneNotReady = 20,
    MilestoneAlreadyReleased = 21,
    InvalidTokenPrice = 22,
    TokenNotWhitelisted = 23,
}

impl_display_error!(
    CrowdfundError,
    AlreadyInitialized      => "already initialized",
    NotInitialized          => "not initialized",
    DeadlinePassed          => "deadline has passed",
    DeadlineNotReached      => "deadline not reached",
    GoalAlreadyMet          => "goal already met",
    GoalNotMet              => "goal not met",
    AlreadyClaimed          => "funds already claimed",
    NothingToPledge         => "nothing to pledge",
    NothingToWithdraw       => "nothing to withdraw",
    InvalidAmount           => "invalid amount",
    InvalidDeadline         => "invalid deadline",
    InvalidGoal             => "invalid goal",
    NotAuthorized           => "not authorized",
    InvalidTier             => "invalid funding tier",
    PledgeCapExceeded       => "pledge would exceed the per-address cap",
    DeadlineAlreadyExtended => "deadline has already been extended once",
    ClaimDeadlineExpired    => "creator claim window has expired",
    Overflow                => "arithmetic overflow",
    MilestoneNotFound       => "milestone not found",
    MilestoneNotReady       => "milestone not ready for release",
    MilestoneAlreadyReleased => "milestone already released",
    InvalidTokenPrice       => "invalid token price from oracle",
    TokenNotWhitelisted     => "token not whitelisted for contributions",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::CrowdfundError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
CrowdfundError::AlreadyInitialized = {}\n\
CrowdfundError::NotInitialized = {}\n\
CrowdfundError::DeadlinePassed = {}\n\
CrowdfundError::DeadlineNotReached = {}\n\
CrowdfundError::GoalAlreadyMet = {}\n\
CrowdfundError::GoalNotMet = {}\n\
CrowdfundError::AlreadyClaimed = {}\n\
CrowdfundError::NothingToPledge = {}\n\
CrowdfundError::NothingToWithdraw = {}\n\
CrowdfundError::InvalidAmount = {}\n\
CrowdfundError::InvalidDeadline = {}\n\
CrowdfundError::InvalidGoal = {}\n\
CrowdfundError::NotAuthorized = {}\n\
CrowdfundError::InvalidTier = {}\n\
CrowdfundError::PledgeCapExceeded = {}\n\
CrowdfundError::DeadlineAlreadyExtended = {}\n\
CrowdfundError::ClaimDeadlineExpired = {}\n\
CrowdfundError::Overflow = {}\n\
CrowdfundError::MilestoneNotFound = {}\n\
CrowdfundError::MilestoneNotReady = {}\n\
CrowdfundError::MilestoneAlreadyReleased = {}\n\
CrowdfundError::InvalidTokenPrice = {}\n\
CrowdfundError::TokenNotWhitelisted = {}\n",
            CrowdfundError::AlreadyInitialized as u32,
            CrowdfundError::NotInitialized as u32,
            CrowdfundError::DeadlinePassed as u32,
            CrowdfundError::DeadlineNotReached as u32,
            CrowdfundError::GoalAlreadyMet as u32,
            CrowdfundError::GoalNotMet as u32,
            CrowdfundError::AlreadyClaimed as u32,
            CrowdfundError::NothingToPledge as u32,
            CrowdfundError::NothingToWithdraw as u32,
            CrowdfundError::InvalidAmount as u32,
            CrowdfundError::InvalidDeadline as u32,
            CrowdfundError::InvalidGoal as u32,
            CrowdfundError::NotAuthorized as u32,
            CrowdfundError::InvalidTier as u32,
            CrowdfundError::PledgeCapExceeded as u32,
            CrowdfundError::DeadlineAlreadyExtended as u32,
            CrowdfundError::ClaimDeadlineExpired as u32,
            CrowdfundError::Overflow as u32,
            CrowdfundError::MilestoneNotFound as u32,
            CrowdfundError::MilestoneNotReady as u32,
            CrowdfundError::MilestoneAlreadyReleased as u32,
            CrowdfundError::InvalidTokenPrice as u32,
            CrowdfundError::TokenNotWhitelisted as u32,
        )
    }

    #[test]
    fn crowdfund_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
