// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;
use soroban_common::impl_display_error;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum DaoError {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    ProposalNotFound = 4,
    InvalidState = 5,
    /// Returned by `execute_proposal` when the voting deadline has not yet passed.
    DeadlineNotReached = 6,
    AlreadyVoted = 7,
    QuorumNotMet = 8,
    ProposalRejected = 9,
    InsufficientVotingPower = 10,
    /// Proposer's token balance is below the required proposal bond (issue #1106).
    InsufficientBondBalance = 11,
    /// Action dispatch via `env.invoke_contract` failed (issue #1108).
    ActionFailed = 12,
    /// Proposer self-cancel rejected because votes have already been cast.
    VotesAlreadyCast = 11,
    /// `quorum_bps` must be in the range [0, 10_000].
    InvalidQuorumBps = 12,
    /// Returned by `vote` when the voting period has ended.
    VotingClosed = 13,
    /// Execution attempted before the execution timelock has elapsed.
    TimelockNotExpired = 14,
    /// Delegation would create a cycle in the delegation graph.
    CircularDelegation = 15,
    /// No locked tokens found for this voter/proposal combination.
    NoLockedTokens = 16,
    /// Attempted to unlock tokens before the proposal voting window has closed.
    VotingStillOpen = 17,
}

impl_display_error!(
    DaoError,
    NotAuthorized           => "not authorized",
    AlreadyInitialized      => "already initialized",
    NotInitialized          => "not initialized",
    ProposalNotFound        => "proposal not found",
    InvalidState            => "invalid proposal state",
    DeadlineNotReached      => "voting deadline not yet reached",
    AlreadyVoted            => "already voted on this proposal",
    QuorumNotMet            => "quorum not met",
    ProposalRejected        => "proposal rejected by majority",
    InsufficientVotingPower => "insufficient voting power",
    InsufficientBondBalance => "insufficient balance for proposal bond",
    ActionFailed            => "proposal action dispatch failed",
    VotesAlreadyCast        => "votes have already been cast; proposer cannot cancel",
    InvalidQuorumBps        => "quorum_bps must be between 0 and 10_000",
    VotingClosed            => "voting period has ended",
    TimelockNotExpired      => "execution timelock has not yet expired",
    CircularDelegation      => "delegation would create a circular loop",
    NoLockedTokens          => "no locked tokens found for this voter and proposal",
    VotingStillOpen         => "voting period is still open; tokens cannot be unlocked yet",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::DaoError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
DaoError::NotAuthorized = {}\n\
DaoError::AlreadyInitialized = {}\n\
DaoError::NotInitialized = {}\n\
DaoError::ProposalNotFound = {}\n\
DaoError::InvalidState = {}\n\
DaoError::DeadlineNotReached = {}\n\
DaoError::AlreadyVoted = {}\n\
DaoError::QuorumNotMet = {}\n\
DaoError::ProposalRejected = {}\n\
DaoError::InsufficientVotingPower = {}\n\
DaoError::InsufficientBondBalance = {}\n\
DaoError::ActionFailed = {}\n",
DaoError::VotesAlreadyCast = {}\n\
DaoError::InvalidQuorumBps = {}\n\
DaoError::VotingClosed = {}\n\
DaoError::TimelockNotExpired = {}\n\
DaoError::CircularDelegation = {}\n\
DaoError::NoLockedTokens = {}\n\
DaoError::VotingStillOpen = {}\n",
            DaoError::NotAuthorized as u32,
            DaoError::AlreadyInitialized as u32,
            DaoError::NotInitialized as u32,
            DaoError::ProposalNotFound as u32,
            DaoError::InvalidState as u32,
            DaoError::DeadlineNotReached as u32,
            DaoError::AlreadyVoted as u32,
            DaoError::QuorumNotMet as u32,
            DaoError::ProposalRejected as u32,
            DaoError::InsufficientVotingPower as u32,
            DaoError::InsufficientBondBalance as u32,
            DaoError::ActionFailed as u32,
            DaoError::VotesAlreadyCast as u32,
            DaoError::InvalidQuorumBps as u32,
            DaoError::VotingClosed as u32,
            DaoError::TimelockNotExpired as u32,
            DaoError::CircularDelegation as u32,
            DaoError::NoLockedTokens as u32,
            DaoError::VotingStillOpen as u32,
        )
    }

    #[test]
    fn dao_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
