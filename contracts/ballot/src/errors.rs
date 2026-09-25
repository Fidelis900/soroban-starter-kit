// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
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
    VotingNotClosed = 12,
    TallyNotAvailable = 13,
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
);
