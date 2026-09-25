// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuctionError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    AuctionEnded = 3,
    AuctionNotEnded = 4,
    BidTooLow = 5,
    AlreadyEnded = 6,
    NoBids = 7,
    NotAuthorized = 8,
    InvalidAmount = 9,
    InvalidDeadline = 10,
    NothingToWithdraw = 11,
    /// The auction ended but the highest bid did not meet the reserve price.
    ReserveNotMet = 12,
    /// `cancel` was called after at least one bid has been placed and the
    /// cancellation grace window is disabled or has elapsed.
    BidAlreadyPlaced = 13,
    /// A checked arithmetic operation on bid, refund, or price values overflowed.
    Overflow = 14,
    /// The call does not apply to this auction's mode (English vs. Dutch).
    WrongMode = 15,
    /// `buy` was called before the Dutch auction's `start_ledger`.
    AuctionNotStarted = 16,
    /// Exactly one of `nft_contract` / `token_id` was supplied.
    InvalidNftParams = 17,
}

impl_display_error!(
    AuctionError,
    AlreadyInitialized => "already initialized",
    NotInitialized     => "not initialized",
    AuctionEnded       => "auction has ended",
    AuctionNotEnded    => "auction has not ended",
    BidTooLow          => "bid too low",
    AlreadyEnded       => "auction already settled",
    NoBids             => "no bids placed",
    NotAuthorized      => "not authorized",
    InvalidAmount      => "invalid amount",
    InvalidDeadline    => "invalid deadline",
    NothingToWithdraw  => "nothing to withdraw",
    ReserveNotMet      => "reserve price not met",
    BidAlreadyPlaced   => "cannot cancel after a bid outside the grace window",
    BidAlreadyPlaced   => "cannot cancel after a bid has been placed",
    Overflow           => "arithmetic overflow",
    WrongMode          => "operation not supported in this auction mode",
    AuctionNotStarted  => "auction has not started",
    InvalidNftParams   => "nft_contract and token_id must be supplied together",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::AuctionError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
AuctionError::AlreadyInitialized = {}\n\
AuctionError::NotInitialized = {}\n\
AuctionError::AuctionEnded = {}\n\
AuctionError::AuctionNotEnded = {}\n\
AuctionError::BidTooLow = {}\n\
AuctionError::AlreadyEnded = {}\n\
AuctionError::NoBids = {}\n\
AuctionError::NotAuthorized = {}\n\
AuctionError::InvalidAmount = {}\n\
AuctionError::InvalidDeadline = {}\n\
AuctionError::NothingToWithdraw = {}\n\
AuctionError::ReserveNotMet = {}\n\
AuctionError::BidAlreadyPlaced = {}\n\
AuctionError::Overflow = {}\n\
AuctionError::WrongMode = {}\n\
AuctionError::AuctionNotStarted = {}\n\
AuctionError::InvalidNftParams = {}\n",
            AuctionError::AlreadyInitialized as u32,
            AuctionError::NotInitialized as u32,
            AuctionError::AuctionEnded as u32,
            AuctionError::AuctionNotEnded as u32,
            AuctionError::BidTooLow as u32,
            AuctionError::AlreadyEnded as u32,
            AuctionError::NoBids as u32,
            AuctionError::NotAuthorized as u32,
            AuctionError::InvalidAmount as u32,
            AuctionError::InvalidDeadline as u32,
            AuctionError::NothingToWithdraw as u32,
            AuctionError::ReserveNotMet as u32,
            AuctionError::BidAlreadyPlaced as u32,
            AuctionError::Overflow as u32,
            AuctionError::WrongMode as u32,
            AuctionError::AuctionNotStarted as u32,
            AuctionError::InvalidNftParams as u32,
        )
    }

    #[test]
    fn auction_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
