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
BondingCurveError::Overflow = {}\n",
            BondingCurveError::AlreadyInitialized as u32,
            BondingCurveError::NotInitialized as u32,
            BondingCurveError::Unauthorized as u32,
            BondingCurveError::InvalidAmount as u32,
            BondingCurveError::InsufficientReserve as u32,
            BondingCurveError::Overflow as u32,
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
