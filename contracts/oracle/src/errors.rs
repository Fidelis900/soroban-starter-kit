// `#[contracterror]` generates undocumented public associated items; allow
// missing_docs for this module. The variants themselves are still documented.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum OracleError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    StalePrice = 4,
    InvalidStalenessThreshold = 5,
    NoPublisherData = 6,
    InsufficientHistory = 7,
}

impl_display_error!(
    OracleError,
    AlreadyInitialized          => "already initialized",
    NotInitialized              => "not initialized",
    Unauthorized                => "not authorized",
    StalePrice                  => "stale price",
    InvalidStalenessThreshold   => "invalid staleness threshold",
    NoPublisherData             => "no publisher data",
    InsufficientHistory         => "insufficient history",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::OracleError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
OracleError::AlreadyInitialized = {}\n\
OracleError::NotInitialized = {}\n\
OracleError::Unauthorized = {}\n\
OracleError::StalePrice = {}\n\
OracleError::InvalidStalenessThreshold = {}\n\
OracleError::NoPublisherData = {}\n\
OracleError::InsufficientHistory = {}\n",
            OracleError::AlreadyInitialized as u32,
            OracleError::NotInitialized as u32,
            OracleError::Unauthorized as u32,
            OracleError::StalePrice as u32,
            OracleError::InvalidStalenessThreshold as u32,
            OracleError::NoPublisherData as u32,
            OracleError::InsufficientHistory as u32,
        )
    }

    #[test]
    fn oracle_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
