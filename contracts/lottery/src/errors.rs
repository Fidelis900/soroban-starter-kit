// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum LotteryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    LotteryClosed = 4,
    DrawAlreadyDone = 5,
    DrawNotDone = 6,
    InvalidTicketPrice = 7,
    CommitAlreadySubmitted = 8,
    RevealMismatch = 9,
    NoTickets = 10,
    InvalidRevealDeadline = 11,
    RefundNotAvailable = 12,
    NothingToRefund = 13,
    InvalidWinnerConfig = 14,
    InsufficientParticipants = 15,
    InvalidTicketCap = 16,
    TicketCapExceeded = 17,
}

impl_display_error!(
    LotteryError,
    AlreadyInitialized      => "already initialized",
    NotInitialized          => "not initialized",
    Unauthorized            => "not authorized",
    LotteryClosed           => "lottery closed",
    DrawAlreadyDone         => "draw already done",
    DrawNotDone             => "draw not done",
    InvalidTicketPrice      => "invalid ticket price",
    CommitAlreadySubmitted  => "commit already submitted",
    RevealMismatch          => "reveal mismatch",
    NoTickets               => "no tickets",
    InvalidRevealDeadline   => "invalid reveal deadline",
    RefundNotAvailable      => "refund not available",
    NothingToRefund         => "nothing to refund",
    InvalidWinnerConfig     => "invalid winner config",
    InsufficientParticipants => "insufficient participants",
    InvalidTicketCap        => "invalid ticket cap",
    TicketCapExceeded       => "ticket cap exceeded",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::LotteryError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
LotteryError::AlreadyInitialized = {}\n\
LotteryError::NotInitialized = {}\n\
LotteryError::Unauthorized = {}\n\
LotteryError::LotteryClosed = {}\n\
LotteryError::DrawAlreadyDone = {}\n\
LotteryError::DrawNotDone = {}\n\
LotteryError::InvalidTicketPrice = {}\n\
LotteryError::CommitAlreadySubmitted = {}\n\
LotteryError::RevealMismatch = {}\n\
LotteryError::NoTickets = {}\n\
LotteryError::InvalidRevealDeadline = {}\n\
LotteryError::RefundNotAvailable = {}\n\
LotteryError::NothingToRefund = {}\n\
LotteryError::InvalidWinnerConfig = {}\n\
LotteryError::InsufficientParticipants = {}\n\
LotteryError::InvalidTicketCap = {}\n\
LotteryError::TicketCapExceeded = {}\n",
            LotteryError::AlreadyInitialized as u32,
            LotteryError::NotInitialized as u32,
            LotteryError::Unauthorized as u32,
            LotteryError::LotteryClosed as u32,
            LotteryError::DrawAlreadyDone as u32,
            LotteryError::DrawNotDone as u32,
            LotteryError::InvalidTicketPrice as u32,
            LotteryError::CommitAlreadySubmitted as u32,
            LotteryError::RevealMismatch as u32,
            LotteryError::NoTickets as u32,
            LotteryError::InvalidRevealDeadline as u32,
            LotteryError::RefundNotAvailable as u32,
            LotteryError::NothingToRefund as u32,
            LotteryError::InvalidWinnerConfig as u32,
            LotteryError::InsufficientParticipants as u32,
            LotteryError::InvalidTicketCap as u32,
            LotteryError::TicketCapExceeded as u32,
        )
    }

    #[test]
    fn lottery_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
