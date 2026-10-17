// #[contracterror] generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

/// Error codes returned by [`EscrowContract`](crate::EscrowContract) methods.
///
/// Each variant maps to a unique `u32` discriminant embedded in the contract ABI.
///
/// # Examples
///
/// ```ignore
/// match escrow_client.try_fund() {
///     Err(Ok(EscrowError::InvalidState)) => { /* wrong lifecycle state */ }
///     _ => {}
/// }
/// ```
#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum EscrowError {
    /// The caller is not permitted to perform this action.
    NotAuthorized = 1,
    /// The escrow is not in the required lifecycle state for this operation.
    InvalidState = 2,
    /// The operation cannot proceed because the deadline has already passed.
    DeadlinePassed = 3,
    /// A refund was requested but the deadline has not yet been reached.
    DeadlineNotReached = 4,
    /// [`EscrowContract::initialize`](crate::EscrowContract::initialize) was
    /// called on an already-initialized contract.
    AlreadyInitialized = 5,
    /// An operation was attempted before the contract was initialized.
    NotInitialized = 6,
    /// The requested partial-release amount exceeds the escrowed balance.
    InsufficientFunds = 7,
    /// The escrow amount must be greater than zero.
    InvalidAmount = 8,
    /// The specified parties for the escrow are not valid.
    InvalidParties = 9,
    /// A dispute timeout claim was attempted but the timeout has not elapsed yet.
    DisputeTimeoutNotReached = 10,
    /// A dispute timeout claim was attempted but no timeout is configured.
    NoDisputeTimeout = 11,
    /// The requested milestone index does not exist in this escrow.
    MilestoneNotFound = 12,
    /// The requested fee in basis points exceeds the maximum allowed (10 000).
    FeeTooHigh = 13,
    /// A reentrant call was detected while a token transfer was in progress.
    Reentrancy = 14,
    /// At least one token/amount pair is required for a multi-token escrow.
    EmptyTokenList = 15,
    /// Token lists and amount lists must have the same length.
    TokenListMismatch = 16,
}

/// Converts the unit error returned by `soroban_common::validate_deadline`
/// into [`EscrowError::DeadlinePassed`].
///
/// `validate_deadline` is generic over `E: From<()>` so that callers do not
/// need to map the error manually. This impl satisfies that bound for
/// `EscrowError`. It cannot be expressed with `#[derive]`.
impl From<()> for EscrowError {
    fn from(_: ()) -> Self {
        Self::DeadlinePassed
    }
}

impl_display_error!(
    EscrowError,
    NotAuthorized             => "not authorized",
    InvalidState              => "invalid state",
    DeadlinePassed            => "deadline passed",
    DeadlineNotReached        => "deadline not reached",
    AlreadyInitialized        => "already initialized",
    NotInitialized            => "not initialized",
    InsufficientFunds         => "insufficient funds",
    InvalidAmount             => "invalid amount",
    InvalidParties            => "invalid parties",
    DisputeTimeoutNotReached  => "dispute timeout not reached",
    NoDisputeTimeout          => "no dispute timeout configured",
    MilestoneNotFound         => "milestone not found",
    FeeTooHigh                => "fee exceeds maximum (10 000 bps)",
    Reentrancy                => "reentrant call detected",
    EmptyTokenList            => "token list must not be empty",
    TokenListMismatch         => "token and amount lists must have the same length",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::EscrowError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)] // reading enum discriminants for snapshot verification
    fn render_error_code_snapshot() -> String {
        format!(
            "\
EscrowError::NotAuthorized = {}\n\
EscrowError::InvalidState = {}\n\
EscrowError::DeadlinePassed = {}\n\
EscrowError::DeadlineNotReached = {}\n\
EscrowError::AlreadyInitialized = {}\n\
EscrowError::NotInitialized = {}\n\
EscrowError::InsufficientFunds = {}\n\
EscrowError::InvalidAmount = {}\n\
EscrowError::InvalidParties = {}\n\
EscrowError::DisputeTimeoutNotReached = {}\n\
EscrowError::NoDisputeTimeout = {}\n\
EscrowError::MilestoneNotFound = {}\n\
EscrowError::FeeTooHigh = {}\n\
EscrowError::Reentrancy = {}\n\
EscrowError::EmptyTokenList = {}\n\
EscrowError::TokenListMismatch = {}\n",
            EscrowError::NotAuthorized as u32,
            EscrowError::InvalidState as u32,
            EscrowError::DeadlinePassed as u32,
            EscrowError::DeadlineNotReached as u32,
            EscrowError::AlreadyInitialized as u32,
            EscrowError::NotInitialized as u32,
            EscrowError::InsufficientFunds as u32,
            EscrowError::InvalidAmount as u32,
            EscrowError::InvalidParties as u32,
            EscrowError::DisputeTimeoutNotReached as u32,
            EscrowError::NoDisputeTimeout as u32,
            EscrowError::MilestoneNotFound as u32,
            EscrowError::FeeTooHigh as u32,
            EscrowError::Reentrancy as u32,
            EscrowError::EmptyTokenList as u32,
            EscrowError::TokenListMismatch as u32,
        )
    }

    #[test]
    fn escrow_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
