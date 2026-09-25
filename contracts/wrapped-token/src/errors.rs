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
}

impl_display_error!(
    WrappedTokenError,
    AlreadyInitialized  => "already initialized",
    NotInitialized      => "not initialized",
    Unauthorized        => "not authorized",
    InvalidAmount       => "invalid amount",
    InsufficientBalance => "insufficient balance",
    InsufficientReserve => "insufficient reserve",
    MaxWrapExceeded     => "max wrap exceeded",
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
WrappedTokenError::MaxWrapExceeded = {}\n",
            WrappedTokenError::AlreadyInitialized as u32,
            WrappedTokenError::NotInitialized as u32,
            WrappedTokenError::Unauthorized as u32,
            WrappedTokenError::InvalidAmount as u32,
            WrappedTokenError::InsufficientBalance as u32,
            WrappedTokenError::InsufficientReserve as u32,
            WrappedTokenError::MaxWrapExceeded as u32,
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
