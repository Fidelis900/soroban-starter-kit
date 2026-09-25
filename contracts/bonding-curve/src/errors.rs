// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BondingCurveError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InsufficientReserve = 5,
    Overflow = 6,
    InvalidFee = 7,
    InvalidConfiguration = 8,
    /// Returned when a buyer attempts more than `MAX_BUYS_PER_LEDGER` purchases
    /// in the same ledger sequence, preventing atomic same-ledger sandwiching.
    RateLimitExceeded = 9,
    /// Returned when a buyer attempts to buy before `BUY_COOLDOWN_LEDGERS` have
    /// elapsed since their last purchase.
    CooldownActive = 10,
    /// Returned when an operation is attempted on a graduated curve that is
    /// only valid while the curve is still active (e.g. buying / minting).
    CurveGraduated = 11,
    /// Returned when `migrate_to_amm` is called but the curve has not yet
    /// graduated.
    NotGraduated = 12,
}

impl_display_error!(
    BondingCurveError,
    AlreadyInitialized   => "already initialized",
    NotInitialized       => "not initialized",
    Unauthorized         => "not authorized",
    InvalidAmount        => "invalid amount",
    InsufficientReserve  => "insufficient reserve",
    Overflow             => "arithmetic overflow",
    InvalidFee           => "invalid fee",
    InvalidConfiguration => "invalid curve configuration",
    RateLimitExceeded    => "rate limit exceeded: too many buys in this ledger",
    CooldownActive       => "buy cooldown active: wait before buying again",
    CurveGraduated       => "curve has graduated: minting is locked",
    NotGraduated         => "curve has not graduated yet",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::BondingCurveError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
BondingCurveError::AlreadyInitialized = {}\n\
BondingCurveError::NotInitialized = {}\n\
BondingCurveError::Unauthorized = {}\n\
BondingCurveError::InvalidAmount = {}\n\
BondingCurveError::InsufficientReserve = {}\n\
BondingCurveError::Overflow = {}\n\
BondingCurveError::InvalidFee = {}\n\
BondingCurveError::InvalidConfiguration = {}\n\
BondingCurveError::RateLimitExceeded = {}\n\
BondingCurveError::CooldownActive = {}\n\
BondingCurveError::CurveGraduated = {}\n\
BondingCurveError::NotGraduated = {}\n",
            BondingCurveError::AlreadyInitialized as u32,
            BondingCurveError::NotInitialized as u32,
            BondingCurveError::Unauthorized as u32,
            BondingCurveError::InvalidAmount as u32,
            BondingCurveError::InsufficientReserve as u32,
            BondingCurveError::Overflow as u32,
            BondingCurveError::InvalidFee as u32,
            BondingCurveError::InvalidConfiguration as u32,
            BondingCurveError::RateLimitExceeded as u32,
            BondingCurveError::CooldownActive as u32,
            BondingCurveError::CurveGraduated as u32,
            BondingCurveError::NotGraduated as u32,
        )
    }

    #[test]
    fn bonding_curve_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
