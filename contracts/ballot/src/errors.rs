// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BallotError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    NotRegistered = 4,
    AlreadyVoted = 5,
    InvalidChoice = 6,
    VotingClosed = 7,
    VotingAlreadyStarted = 8,
    InvalidWindow = 9,
    VotingNotStarted = 10,
    NoChoices = 11,
    InvalidRanking = 12,
    DuplicateRanking = 13,
    VotingNotClosed = 14,
    TallyNotAvailable = 15,
}

impl core::fmt::Display for BallotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            BallotError::AlreadyInitialized => "already initialized",
            BallotError::NotInitialized => "not initialized",
            BallotError::Unauthorized => "not authorized",
            BallotError::NotRegistered => "voter not registered",
            BallotError::AlreadyVoted => "voter has already voted",
            BallotError::InvalidChoice => "invalid vote choice",
            BallotError::VotingClosed => "voting is closed",
            BallotError::VotingAlreadyStarted => "voting already started",
            BallotError::InvalidWindow => "invalid voting window",
            BallotError::VotingNotStarted => "voting not started",
            BallotError::NoChoices => "no choices provided",
            BallotError::InvalidRanking => "invalid preference ranking",
            BallotError::DuplicateRanking => "duplicate choice in ranking",
            BallotError::VotingNotClosed => "voting window has not closed",
            BallotError::TallyNotAvailable => "tally not available",
        })
    }
}
}

impl core::fmt::Display for BallotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            BallotError::AlreadyInitialized => "already initialized",
            BallotError::NotInitialized => "not initialized",
            BallotError::Unauthorized => "not authorized",
            BallotError::NotRegistered => "voter not registered",
            BallotError::AlreadyVoted => "voter has already voted",
            BallotError::InvalidChoice => "invalid vote choice",
            BallotError::VotingClosed => "voting is closed",
            BallotError::VotingAlreadyStarted => "voting already started",
            BallotError::InvalidWindow => "invalid voting window",
            BallotError::VotingNotStarted => "voting not started",
            BallotError::NoChoices => "no choices provided",
            BallotError::InvalidRanking => "invalid preference ranking",
            BallotError::DuplicateRanking => "duplicate choice in ranking",
            BallotError::VotingNotClosed => "voting window has not closed",
            BallotError::TallyNotAvailable => "tally not available",
        })
    }
}
}

impl core::fmt::Display for BallotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            BallotError::AlreadyInitialized => "already initialized",
            BallotError::NotInitialized => "not initialized",
            BallotError::Unauthorized => "not authorized",
            BallotError::NotRegistered => "voter not registered",
            BallotError::AlreadyVoted => "voter has already voted",
            BallotError::InvalidChoice => "invalid vote choice",
            BallotError::VotingClosed => "voting is closed",
            BallotError::VotingAlreadyStarted => "voting already started",
            BallotError::InvalidWindow => "invalid voting window",
            BallotError::VotingNotStarted => "voting not started",
            BallotError::NoChoices => "no choices provided",
            BallotError::InvalidRanking => "invalid preference ranking",
            BallotError::DuplicateRanking => "duplicate choice in ranking",
            BallotError::VotingNotClosed => "voting window has not closed",
            BallotError::TallyNotAvailable => "tally not available",
        })
    }
}
}

impl core::fmt::Display for BallotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            BallotError::AlreadyInitialized => "already initialized",
            BallotError::NotInitialized => "not initialized",
            BallotError::Unauthorized => "not authorized",
            BallotError::NotRegistered => "voter not registered",
            BallotError::AlreadyVoted => "voter has already voted",
            BallotError::InvalidChoice => "invalid vote choice",
            BallotError::VotingClosed => "voting is closed",
            BallotError::VotingAlreadyStarted => "voting already started",
            BallotError::InvalidWindow => "invalid voting window",
            BallotError::VotingNotStarted => "voting not started",
            BallotError::NoChoices => "no choices provided",
            BallotError::InvalidRanking => "invalid preference ranking",
            BallotError::DuplicateRanking => "duplicate choice in ranking",
            BallotError::VotingNotClosed => "voting window has not closed",
            BallotError::TallyNotAvailable => "tally not available",
        })
    }
}
    Tied = 16,
    AlreadyCertified = 17,
    QuorumNotMet = 18,
    ResultsNotReady = 19,
    InvalidTieResolution = 20,
}

impl_display_error!(
    BallotError,
    AlreadyInitialized   => "already initialized",
    NotInitialized       => "not initialized",
    Unauthorized         => "not authorized",
    NotRegistered        => "voter not registered",
    AlreadyVoted         => "voter has already voted",
    InvalidChoice        => "invalid vote choice",
    VotingClosed         => "voting is closed",
    VotingAlreadyStarted => "voting already started",
    InvalidWindow        => "invalid voting window",
    VotingNotStarted     => "voting not started",
    NoChoices            => "no choices provided",
    InvalidRanking       => "invalid preference ranking",
    DuplicateRanking     => "duplicate choice in ranking",
    VotingNotClosed      => "voting window has not closed",
    TallyNotAvailable    => "tally not available",
    Tied                 => "leading choices are tied",
    AlreadyCertified     => "results already certified",
    QuorumNotMet         => "quorum not met",
    ResultsNotReady      => "no votes to certify",
    InvalidTieResolution => "choice is not among the tied choices",
);
