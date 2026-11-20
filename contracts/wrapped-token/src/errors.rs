// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WrappedTokenError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InsufficientBalance = 5,
    InsufficientReserve = 6,
    MaxWrapExceeded = 7,
    /// An i128 supply or amount calculation overflowed.
    Overflow = 8,
    /// A fee is greater than 10_000 bps, or a non-zero fee was configured without a `fee_recipient`.
    InvalidFee = 9,
    /// A batch call was given an empty recipient list.
    EmptyBatch = 10,
    /// Aggregate unwrap volume in the current ledger window exceeded the cap.
    RateLimitExceeded = 11,
    /// Amount is below the configured minimum wrap/unwrap threshold.
    AmountTooLow = 12,
    /// `wrap`/`unwrap` was re-entered while another wrap/unwrap was executing (#1435).
    Reentrancy = 13,
    /// The underlying asset transfer reverted or did not move the expected amount;
    /// the whole operation, including any wrapped-token burn, was rolled back (#1434).
    UnderlyingTransferFailed = 14,
}

impl_display_error!(
    WrappedTokenError,
    AlreadyInitialized       => "already initialized",
    NotInitialized           => "not initialized",
    Unauthorized             => "not authorized",
    InvalidAmount            => "invalid amount",
    InsufficientBalance      => "insufficient balance",
    InsufficientReserve      => "insufficient reserve",
    MaxWrapExceeded          => "max wrap exceeded",
    Overflow                 => "arithmetic overflow",
    InvalidFee               => "invalid fee configuration",
    EmptyBatch               => "empty batch",
    RateLimitExceeded        => "unwrap rate limit exceeded",
    AmountTooLow             => "amount below minimum",
    Reentrancy               => "reentrant call detected",
    UnderlyingTransferFailed => "underlying transfer failed",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::WrappedTokenError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
WrappedTokenError::AlreadyInitialized = {}\n\
WrappedTokenError::NotInitialized = {}\n\
WrappedTokenError::Unauthorized = {}\n\
WrappedTokenError::InvalidAmount = {}\n\
WrappedTokenError::InsufficientBalance = {}\n\
WrappedTokenError::InsufficientReserve = {}\n\
WrappedTokenError::MaxWrapExceeded = {}\n\
WrappedTokenError::Overflow = {}\n\
WrappedTokenError::InvalidFee = {}\n\
WrappedTokenError::EmptyBatch = {}\n\
WrappedTokenError::RateLimitExceeded = {}\n\
WrappedTokenError::AmountTooLow = {}\n\
WrappedTokenError::Reentrancy = {}\n\
WrappedTokenError::UnderlyingTransferFailed = {}\n",
            WrappedTokenError::AlreadyInitialized as u32,
            WrappedTokenError::NotInitialized as u32,
            WrappedTokenError::Unauthorized as u32,
            WrappedTokenError::InvalidAmount as u32,
            WrappedTokenError::InsufficientBalance as u32,
            WrappedTokenError::InsufficientReserve as u32,
            WrappedTokenError::MaxWrapExceeded as u32,
            WrappedTokenError::Overflow as u32,
            WrappedTokenError::InvalidFee as u32,
            WrappedTokenError::EmptyBatch as u32,
            WrappedTokenError::RateLimitExceeded as u32,
            WrappedTokenError::AmountTooLow as u32,
            WrappedTokenError::Reentrancy as u32,
            WrappedTokenError::UnderlyingTransferFailed as u32,
        )
    }

    #[test]
    fn wrapped_token_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
