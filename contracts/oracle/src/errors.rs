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
    /// The EMA smoothing factor is zero or exceeds 10 000 basis points.
    InvalidSmoothingFactor = 8,
    /// The publisher already submitted a price in the current ledger.
    SubmissionTooFrequent = 9,
    /// Price submitted is zero or negative.
    InvalidPrice = 8,
    /// Requested asset symbol has no price feed.
    AssetNotFound = 9,
    /// Fewer than the configured minimum quorum of fresh publishers
    /// submitted prices for `get_median_price` (issue #1195).
    InsufficientQuorum = 10,
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
    InvalidPrice                => "price must be greater than zero",
    AssetNotFound               => "asset not found",
    InsufficientQuorum          => "insufficient publisher quorum",
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
OracleError::InsufficientHistory = {}\n\
OracleError::InvalidSmoothingFactor = {}\n\
OracleError::SubmissionTooFrequent = {}\n",
OracleError::InvalidPrice = {}\n\
OracleError::AssetNotFound = {}\n\
OracleError::InsufficientQuorum = {}\n",
            OracleError::AlreadyInitialized as u32,
            OracleError::NotInitialized as u32,
            OracleError::Unauthorized as u32,
            OracleError::StalePrice as u32,
            OracleError::InvalidStalenessThreshold as u32,
            OracleError::NoPublisherData as u32,
            OracleError::InsufficientHistory as u32,
            OracleError::InvalidSmoothingFactor as u32,
            OracleError::SubmissionTooFrequent as u32,
            OracleError::InvalidPrice as u32,
            OracleError::AssetNotFound as u32,
            OracleError::InsufficientQuorum as u32,
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
