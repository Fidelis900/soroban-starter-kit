// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

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
    /// An active unexecuted proposal with the identical payload hash already exists (issue #1319).
    DuplicateProposal = 18,
    /// Voter selected a choice symbol that is not part of the proposal's choices.
    InvalidChoice = 19,
    /// Multi-choice proposal created without any choices.
    NoChoices = 20,
    /// Voting type is not supported for this operation.
    InvalidVotingType = 21,
    /// Emergency fast-track requested but no security council is configured.
    MissingSecurityCouncil = 18,
    /// Caller is not the configured security council.
    UnauthorizedCouncil = 19,
    /// Caller is not the proposal's proposer.
    NotProposer = 20,
    /// Proposal cancellation rejected because voting has already started.
    VotingAlreadyStarted = 21,
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
    DuplicateProposal       => "an active proposal with the identical payload already exists",
    InvalidChoice           => "selected choice is not part of this proposal",
    NoChoices               => "multi-choice proposal must define at least one choice",
    InvalidVotingType       => "unsupported voting type",
    MissingSecurityCouncil  => "no security council configured",
    UnauthorizedCouncil     => "caller is not the security council",
    NotProposer             => "caller is not the proposal proposer",
    VotingAlreadyStarted    => "voting has already started; proposal cannot be cancelled",
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
DaoError::VotingStillOpen = {}\n\
DaoError::DuplicateProposal = {}\n\
DaoError::InvalidChoice = {}\n\
DaoError::NoChoices = {}\n\
DaoError::InvalidVotingType = {}\n",
DaoError::MissingSecurityCouncil = {}\n\
DaoError::UnauthorizedCouncil = {}\n\
DaoError::NotProposer = {}\n\
DaoError::VotingAlreadyStarted = {}\n",
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
            DaoError::DuplicateProposal as u32,
            DaoError::InvalidChoice as u32,
            DaoError::NoChoices as u32,
            DaoError::InvalidVotingType as u32,
            DaoError::MissingSecurityCouncil as u32,
            DaoError::UnauthorizedCouncil as u32,
            DaoError::NotProposer as u32,
            DaoError::VotingAlreadyStarted as u32,
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
