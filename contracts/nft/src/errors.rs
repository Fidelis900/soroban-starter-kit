// `#[contracterror]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_common::impl_display_error;
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug)]
pub enum NftError {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    TokenNotFound = 4,
    TokenAlreadyMinted = 5,
    NotOwner = 6,
    NotApproved = 7,
    SupplyCapReached = 8,
    InvalidTokenId = 9,
    /// Royalty basis points exceed 10 000 (100 %).
    InvalidRoyalty = 10,
    /// A royalty BPS was provided without a recipient (or vice-versa).
    RoyaltyRecipientMissing = 11,
    /// Token or collection is soulbound and cannot be transferred.
    NonTransferable = 12,
}

impl_display_error!(
    NftError,
    NotAuthorized           => "not authorized",
    AlreadyInitialized      => "already initialized",
    NotInitialized          => "not initialized",
    TokenNotFound           => "token not found",
    TokenAlreadyMinted      => "token already minted",
    NotOwner                => "not the token owner",
    NotApproved             => "not approved for this token",
    SupplyCapReached        => "supply cap reached",
    InvalidTokenId          => "invalid token id",
    InvalidRoyalty          => "royalty bps exceeds 10 000",
    RoyaltyRecipientMissing => "royalty recipient must be set when royalty bps > 0",
    NonTransferable         => "token is soulbound and non-transferable",
);

#[cfg(test)]
mod tests {
    extern crate std;

    use super::NftError;
    use std::format;
    use std::string::String;

    #[allow(clippy::as_conversions)]
    fn render_error_code_snapshot() -> String {
        format!(
            "\
NftError::NotAuthorized = {}\n\
NftError::AlreadyInitialized = {}\n\
NftError::NotInitialized = {}\n\
NftError::TokenNotFound = {}\n\
NftError::TokenAlreadyMinted = {}\n\
NftError::NotOwner = {}\n\
NftError::NotApproved = {}\n\
NftError::SupplyCapReached = {}\n\
NftError::InvalidTokenId = {}\n\
NftError::InvalidRoyalty = {}\n\
NftError::RoyaltyRecipientMissing = {}\n\
NftError::NonTransferable = {}\n",
            NftError::NotAuthorized as u32,
            NftError::AlreadyInitialized as u32,
            NftError::NotInitialized as u32,
            NftError::TokenNotFound as u32,
            NftError::TokenAlreadyMinted as u32,
            NftError::NotOwner as u32,
            NftError::NotApproved as u32,
            NftError::SupplyCapReached as u32,
            NftError::InvalidTokenId as u32,
            NftError::InvalidRoyalty as u32,
            NftError::RoyaltyRecipientMissing as u32,
            NftError::NonTransferable as u32,
        )
    }

    #[test]
    fn nft_error_codes_match_snapshot() {
        assert_eq!(
            render_error_code_snapshot(),
            include_str!("../snapshots/error_codes.snap")
        );
    }
}
