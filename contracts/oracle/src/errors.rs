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
    InvalidPrice = 10,
    /// Requested asset symbol has no price feed.
    AssetNotFound = 11,
    /// The update exceeded the configured maximum deviation.
    PriceDeviationTooHigh = 12,
    /// The feed is paused pending administrator verification.
    FeedPaused = 13,
    /// The circuit-breaker threshold is invalid.
    InvalidDeviationThreshold = 14,
    /// The publisher has no registered bond or the penalty exceeds it.
    InsufficientBond = 15,
    /// Publisher bonding has not been configured.
    BondingNotConfigured = 16,
    /// Fewer than the configured minimum quorum of fresh publishers
    /// submitted prices for `get_median_price` (issue #1195).
    InsufficientQuorum = 17,
    /// The price submission timestamp is older than the allowed maximum ledger
    /// delta, indicating the price data is too stale to accept (issue #1501).
    SubmissionTooOld = 18,
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
    InvalidSmoothingFactor      => "invalid smoothing factor",
    SubmissionTooFrequent       => "submission too frequent",
    InvalidPrice                => "price must be greater than zero",
    AssetNotFound               => "asset not found",
    PriceDeviationTooHigh       => "price deviation too high",
    FeedPaused                  => "feed paused",
    InvalidDeviationThreshold   => "invalid deviation threshold",
    InsufficientBond            => "insufficient publisher bond",
    BondingNotConfigured        => "publisher bonding not configured",
    InsufficientQuorum          => "insufficient publisher quorum",
    SubmissionTooOld            => "submission too old: price data exceeds maximum ledger delta",
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
OracleError::SubmissionTooFrequent = {}\n\
OracleError::InvalidPrice = {}\n\
OracleError::AssetNotFound = {}\n\
OracleError::PriceDeviationTooHigh = {}\n\
OracleError::FeedPaused = {}\n\
OracleError::InvalidDeviationThreshold = {}\n\
OracleError::InsufficientBond = {}\n\
OracleError::BondingNotConfigured = {}\n\
OracleError::InsufficientQuorum = {}\n\
OracleError::SubmissionTooOld = {}\n",
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
            OracleError::PriceDeviationTooHigh as u32,
            OracleError::FeedPaused as u32,
            OracleError::InvalidDeviationThreshold as u32,
            OracleError::InsufficientBond as u32,
            OracleError::BondingNotConfigured as u32,
            OracleError::InsufficientQuorum as u32,
            OracleError::SubmissionTooOld as u32,
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
