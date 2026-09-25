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
    PaymentTokenNotAllowed = 13,
    SellerNotOwner = 14,
    BatchTooLarge = 15,
    EmptyBatch = 16,
    ListingStillActive = 17,
    Reentrant = 18,
    PriceExceedsMax = 13,
    CollectionOfferNotFound = 14,
    CollectionOfferExpired = 15,
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
    PaymentTokenNotAllowed => "payment token not allowed",
    SellerNotOwner      => "seller no longer owns the NFT",
    BatchTooLarge       => "batch too large",
    EmptyBatch          => "empty batch",
    ListingStillActive  => "listing still active",
    Reentrant           => "reentrant call",
    PriceExceedsMax     => "listing price exceeds buyer max price",
    CollectionOfferNotFound => "collection offer not found",
    CollectionOfferExpired  => "collection offer expired",
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
MarketplaceError::PaymentTokenNotAllowed = {}\n\
MarketplaceError::SellerNotOwner = {}\n\
MarketplaceError::BatchTooLarge = {}\n\
MarketplaceError::EmptyBatch = {}\n\
MarketplaceError::ListingStillActive = {}\n\
MarketplaceError::Reentrant = {}\n",
MarketplaceError::PriceExceedsMax = {}\n\
MarketplaceError::CollectionOfferNotFound = {}\n\
MarketplaceError::CollectionOfferExpired = {}\n",
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
            MarketplaceError::PaymentTokenNotAllowed as u32,
            MarketplaceError::SellerNotOwner as u32,
            MarketplaceError::BatchTooLarge as u32,
            MarketplaceError::EmptyBatch as u32,
            MarketplaceError::ListingStillActive as u32,
            MarketplaceError::Reentrant as u32,
            MarketplaceError::PriceExceedsMax as u32,
            MarketplaceError::CollectionOfferNotFound as u32,
            MarketplaceError::CollectionOfferExpired as u32,
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
