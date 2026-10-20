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
