// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarketplaceError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    InvalidPrice = 4,
    ListingNotFound = 5,
    ListingInactive = 6,
    InvalidRoyalty = 7,
    InvalidExpiry = 8,
    ListingExpired = 9,
    ListingNotExpired = 10,
    InvalidOfferAmount = 11,
    OfferNotFound = 12,
    /// Platform fee above 10 000 bps.
    InvalidPlatformFee = 13,
    /// Dutch auction parameters are invalid (non-positive reserve, starting
    /// price not above reserve, or zero duration).
    InvalidDutchAuction = 14,
    /// A checked arithmetic operation overflowed.
    ArithmeticOverflow = 15,
    /// Creator royalty plus platform fee exceed the sale price.
    PayoutExceedsPrice = 16,
}

impl_display_error!(
    MarketplaceError,
    AlreadyInitialized  => "already initialized",
    NotInitialized      => "not initialized",
    NotAuthorized       => "not authorized",
    InvalidPrice        => "invalid price",
    ListingNotFound     => "listing not found",
    ListingInactive     => "listing inactive",
    InvalidRoyalty      => "invalid royalty",
    InvalidExpiry       => "invalid expiry",
    ListingExpired      => "listing expired",
    ListingNotExpired   => "listing not expired",
    InvalidOfferAmount  => "invalid offer amount",
    OfferNotFound       => "offer not found",
    InvalidPlatformFee  => "invalid platform fee",
    InvalidDutchAuction => "invalid dutch auction parameters",
    ArithmeticOverflow  => "arithmetic overflow",
    PayoutExceedsPrice  => "royalty and fee exceed sale price",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::MarketplaceError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
MarketplaceError::AlreadyInitialized = {}\n\
MarketplaceError::NotInitialized = {}\n\
MarketplaceError::NotAuthorized = {}\n\
MarketplaceError::InvalidPrice = {}\n\
MarketplaceError::ListingNotFound = {}\n\
MarketplaceError::ListingInactive = {}\n\
MarketplaceError::InvalidRoyalty = {}\n\
MarketplaceError::InvalidExpiry = {}\n\
MarketplaceError::ListingExpired = {}\n\
MarketplaceError::ListingNotExpired = {}\n\
MarketplaceError::InvalidOfferAmount = {}\n\
MarketplaceError::OfferNotFound = {}\n\
MarketplaceError::InvalidPlatformFee = {}\n\
MarketplaceError::InvalidDutchAuction = {}\n\
MarketplaceError::ArithmeticOverflow = {}\n\
MarketplaceError::PayoutExceedsPrice = {}\n",
            MarketplaceError::AlreadyInitialized as u32,
            MarketplaceError::NotInitialized as u32,
            MarketplaceError::NotAuthorized as u32,
            MarketplaceError::InvalidPrice as u32,
            MarketplaceError::ListingNotFound as u32,
            MarketplaceError::ListingInactive as u32,
            MarketplaceError::InvalidRoyalty as u32,
            MarketplaceError::InvalidExpiry as u32,
            MarketplaceError::ListingExpired as u32,
            MarketplaceError::ListingNotExpired as u32,
            MarketplaceError::InvalidOfferAmount as u32,
            MarketplaceError::OfferNotFound as u32,
            MarketplaceError::InvalidPlatformFee as u32,
            MarketplaceError::InvalidDutchAuction as u32,
            MarketplaceError::ArithmeticOverflow as u32,
            MarketplaceError::PayoutExceedsPrice as u32,
        )
    }

    #[test]
    fn marketplace_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
