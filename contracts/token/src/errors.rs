use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum TokenError {
    InsufficientBalance = 1,
    InsufficientAllowance = 2,
    Unauthorized = 3,
    AlreadyInitialized = 4,
    NotInitialized = 5,
    InvalidAmount = 6,
    Overflow = 7,
    InvalidNonce = 8,
    PermitExpired = 9,
    PermitSignerNotSet = 10,
    AccountLocked = 11,
}

impl_display_error!(
    TokenError,
    InsufficientBalance  => "insufficient balance",
    InsufficientAllowance => "insufficient allowance",
    Unauthorized         => "unauthorized",
    AlreadyInitialized   => "already initialized",
    NotInitialized       => "not initialized",
    InvalidAmount        => "invalid amount",
    Overflow             => "arithmetic overflow",
    InvalidNonce         => "invalid permit nonce",
    PermitExpired        => "permit expired",
    PermitSignerNotSet   => "permit signer not set",
    AccountLocked        => "account locked",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::TokenError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)] // reading enum discriminants for snapshot verification
    fn render_error_code_snapshot() -> String {
        format!(
            "\
TokenError::InsufficientBalance = {}\n\
TokenError::InsufficientAllowance = {}\n\
TokenError::Unauthorized = {}\n\
TokenError::AlreadyInitialized = {}\n\
TokenError::NotInitialized = {}\n\
TokenError::InvalidAmount = {}\n\
TokenError::Overflow = {}\n\
TokenError::InvalidNonce = {}\n\
TokenError::PermitExpired = {}\n\
TokenError::PermitSignerNotSet = {}\n\
TokenError::AccountLocked = {}\n",
            TokenError::InsufficientBalance as u32,
            TokenError::InsufficientAllowance as u32,
            TokenError::Unauthorized as u32,
            TokenError::AlreadyInitialized as u32,
            TokenError::NotInitialized as u32,
            TokenError::InvalidAmount as u32,
            TokenError::Overflow as u32,
            TokenError::InvalidNonce as u32,
            TokenError::PermitExpired as u32,
            TokenError::PermitSignerNotSet as u32,
            TokenError::AccountLocked as u32,
        )
    }

    #[test]
    fn token_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
