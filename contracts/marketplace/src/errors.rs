// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketplaceError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotAuthorized = 3,
    InvalidPrice = 4,
    ListingNotFound = 5,
    ListingInactive = 6,
    InvalidRoyalty = 7,
    InvalidExpiry = 8,
    /// The listing's expiry (ledger sequence or timestamp) has passed.
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
    PaymentTokenNotAllowed = 13,
    SellerNotOwner = 14,
    BatchTooLarge = 15,
    EmptyBatch = 16,
    ListingStillActive = 17,
    Reentrant = 18,
    InvalidDutchParams = 19,
    DutchNotStarted = 20,
    EmptyBundle = 21,
    DuplicateTokenId = 22,
    BundleNotFound = 23,
    BundleInactive = 24,
    PriceExceedsMax = 25,
    CollectionOfferNotFound = 26,
    CollectionOfferExpired = 27,
    OfferExpired = 28,
    CounterOfferTooLow = 29,
    InvalidMinCounterOfferBps = 30,
    /// `expire_offer` was called before the offer's expiration ledger.
    OfferNotExpired = 31,
}

/// Name used by the issue tracker and external integrations for this contract's
/// error type.
pub type ContractError = MarketplaceError;

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
    PaymentTokenNotAllowed => "payment token not allowed",
    SellerNotOwner      => "seller no longer owns the NFT",
    BatchTooLarge       => "batch too large",
    EmptyBatch          => "empty batch",
    ListingStillActive  => "listing still active",
    Reentrant           => "reentrant call",
    InvalidDutchParams  => "invalid dutch auction parameters",
    DutchNotStarted     => "dutch auction has not started",
    EmptyBundle         => "empty bundle",
    DuplicateTokenId    => "duplicate token id in bundle",
    BundleNotFound      => "bundle not found",
    BundleInactive      => "bundle inactive",
    PriceExceedsMax     => "listing price exceeds buyer max price",
    CollectionOfferNotFound => "collection offer not found",
    CollectionOfferExpired  => "collection offer expired",
    OfferExpired        => "offer expired",
    CounterOfferTooLow  => "counter offer below minimum increment",
    InvalidMinCounterOfferBps => "invalid minimum counter offer bps",
    OfferNotExpired     => "offer not expired",
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
MarketplaceError::PaymentTokenNotAllowed = {}\n\
MarketplaceError::SellerNotOwner = {}\n\
MarketplaceError::BatchTooLarge = {}\n\
MarketplaceError::EmptyBatch = {}\n\
MarketplaceError::ListingStillActive = {}\n\
MarketplaceError::Reentrant = {}\n\
MarketplaceError::InvalidDutchParams = {}\n\
MarketplaceError::DutchNotStarted = {}\n\
MarketplaceError::EmptyBundle = {}\n\
MarketplaceError::DuplicateTokenId = {}\n\
MarketplaceError::BundleNotFound = {}\n\
MarketplaceError::BundleInactive = {}\n\
MarketplaceError::PriceExceedsMax = {}\n\
MarketplaceError::CollectionOfferNotFound = {}\n\
MarketplaceError::CollectionOfferExpired = {}\n\
MarketplaceError::OfferExpired = {}\n\
MarketplaceError::CounterOfferTooLow = {}\n\
MarketplaceError::InvalidMinCounterOfferBps = {}\n\
MarketplaceError::OfferNotExpired = {}\n",
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
            MarketplaceError::PaymentTokenNotAllowed as u32,
            MarketplaceError::SellerNotOwner as u32,
            MarketplaceError::BatchTooLarge as u32,
            MarketplaceError::EmptyBatch as u32,
            MarketplaceError::ListingStillActive as u32,
            MarketplaceError::Reentrant as u32,
            MarketplaceError::InvalidDutchParams as u32,
            MarketplaceError::DutchNotStarted as u32,
            MarketplaceError::EmptyBundle as u32,
            MarketplaceError::DuplicateTokenId as u32,
            MarketplaceError::BundleNotFound as u32,
            MarketplaceError::BundleInactive as u32,
            MarketplaceError::PriceExceedsMax as u32,
            MarketplaceError::CollectionOfferNotFound as u32,
            MarketplaceError::CollectionOfferExpired as u32,
            MarketplaceError::OfferExpired as u32,
            MarketplaceError::CounterOfferTooLow as u32,
            MarketplaceError::InvalidMinCounterOfferBps as u32,
            MarketplaceError::OfferNotExpired as u32,
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
