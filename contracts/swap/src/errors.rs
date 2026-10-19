// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapError {
    NotAuthorized = 1,
    SwapNotFound = 2,
    InvalidState = 3,
    DeadlineExpired = 4,
    InvalidAmount = 5,
    InvalidDeadline = 6,
    AlreadyCompleted = 7,
    AlreadyCancelled = 8,
    AlreadyInitialized = 9,
    NotInitialized = 10,
    InvalidFee = 11,
    BasketSwapNotFound = 12,
    MathOverflow = 13,
    StorageError = 14,
    FeeZeroNotAllowed = 15,
    ExecutionDelayExceeded = 16,
    PartialFillsDisabled = 17,
    InvalidFillAmount = 18,
    HashLockMismatch = 19,
    NotCreator = 20,
    EmptyBatch = 21,
    SecretNotRevealed = 22,
    FeeZeroNotAllowed = 12,
    ExecutionDelayExceeded = 13,
    SwapExpired = 15,
}

impl_display_error!(
    SwapError,
    NotAuthorized      => "not authorized",
    SwapNotFound       => "swap not found",
    InvalidState       => "invalid swap state",
    DeadlineExpired    => "swap deadline has expired",
    InvalidAmount      => "invalid amount",
    InvalidDeadline    => "invalid deadline",
    AlreadyCompleted   => "swap already completed",
    AlreadyCancelled   => "swap already cancelled",
    AlreadyInitialized => "contract already initialized",
    NotInitialized     => "contract not initialized",
    InvalidFee         => "invalid fee basis points",
    BasketSwapNotFound => "basket swap not found",
    MathOverflow       => "arithmetic overflow",
    StorageError       => "storage error",
    FeeZeroNotAllowed      => "fee truncated to zero or below minimum required",
    ExecutionDelayExceeded => "maximum execution delay exceeded",
    PartialFillsDisabled   => "partial fills are disabled for this swap",
    InvalidFillAmount      => "invalid fill amount",
    HashLockMismatch       => "hash lock preimage does not match",
    NotCreator             => "caller is not the swap creator",
    EmptyBatch             => "batch must contain at least one swap",
    SecretNotRevealed      => "secret has not been revealed",
    SwapExpired            => "swap has expired",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::SwapError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
SwapError::NotAuthorized = {}\n\
SwapError::SwapNotFound = {}\n\
SwapError::InvalidState = {}\n\
SwapError::DeadlineExpired = {}\n\
SwapError::InvalidAmount = {}\n\
SwapError::InvalidDeadline = {}\n\
SwapError::AlreadyCompleted = {}\n\
SwapError::AlreadyCancelled = {}\n\
SwapError::PartialFillsDisabled = {}\n\
SwapError::InvalidFillAmount = {}\n\
SwapError::HashLockMismatch = {}\n\
SwapError::NotCreator = {}\n\
SwapError::EmptyBatch = {}\n\
SwapError::SecretNotRevealed = {}\n",
SwapError::SwapExpired = {}\n",
            SwapError::NotAuthorized as u32,
            SwapError::SwapNotFound as u32,
            SwapError::InvalidState as u32,
            SwapError::DeadlineExpired as u32,
            SwapError::InvalidAmount as u32,
            SwapError::InvalidDeadline as u32,
            SwapError::AlreadyCompleted as u32,
            SwapError::AlreadyCancelled as u32,
            SwapError::PartialFillsDisabled as u32,
            SwapError::InvalidFillAmount as u32,
            SwapError::HashLockMismatch as u32,
            SwapError::NotCreator as u32,
            SwapError::EmptyBatch as u32,
            SwapError::SecretNotRevealed as u32,
            SwapError::SwapExpired as u32,
        )
    }

    #[test]
    fn swap_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
