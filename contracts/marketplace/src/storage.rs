// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Default payment token address, set at initialization (instance).
    PaymentToken,
    /// Royalty in basis points, e.g. 250 = 2.5 % (instance).
    RoyaltyBps,
    /// Royalty recipient address (instance).
    RoyaltyRecipient,
    /// Next listing ID counter (instance).
    NextListingId,
    /// Per-listing details (persistent).
    Listing(u64),
    /// Escrowed offer amount for (listing_id, buyer) (persistent).
    Offer(u64, Address),
    /// Whether the payment-token whitelist is enforced on new listings (instance).
    WhitelistEnabled,
    /// Whether a payment token is whitelisted by the admin (persistent).
    AllowedToken(Address),
    /// Reentrancy guard held while a sale or batch operation is in progress (instance).
    Locked,
    /// Marketplace-wide multi-recipient royalty split, `Vec<(recipient, bps)>` (instance).
    /// When set and non-empty it supersedes `RoyaltyBps`/`RoyaltyRecipient`.
    RoyaltySplits,
    /// Next collection offer ID counter (instance).
    NextCollectionOfferId,
    /// Escrowed collection-wide floor offer (persistent).
    CollectionOffer(u64),
    /// Next bundle listing ID counter (instance).
    NextBundleId,
    /// Per-bundle listing details (persistent).
    Bundle(u64),
}

/// Pricing model of a listing.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ListingType {
    /// A fixed asking price.
    Fixed,
    /// A Dutch auction whose price decays linearly from `start_price` at
    /// `start_ledger` down to `end_price` at `end_ledger`.
    Dutch {
        /// Price at `start_ledger`.
        start_price: i128,
        /// Price at (and after) `end_ledger`.
        end_price: i128,
        /// Ledger sequence at which the auction starts.
        start_ledger: u32,
        /// Ledger sequence at which the price reaches `end_price`.
        end_ledger: u32,
    },
}

/// State of a single NFT listing.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    /// The NFT contract address.
    pub nft_contract: Address,
    /// The token ID being sold.
    pub token_id: u32,
    /// The seller.
    pub seller: Address,
    /// Asking price in `payment_token` units.
    pub price: i128,
    /// Token the seller accepts as payment for this listing. Purchases, offers
    /// and royalties on this listing are all settled in this token.
    pub payment_token: Address,
    /// Whether the listing is still open.
    pub active: bool,
    /// Optional ledger sequence after which the listing can no longer be bought.
    pub expires_at: Option<u32>,
    /// Pricing model of the listing. `Fixed` listings use `price` directly;
    /// `Dutch` listings compute the spot price from the elapsed ledgers.
    pub listing_type: ListingType,
}

/// A listing paired with its ID, as returned by [`super::MarketplaceContract::get_active_listings`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ListingEntry {
    /// The listing ID.
    pub id: u64,
    /// The listing itself.
    pub listing: Listing,
}

/// One page of results from [`super::MarketplaceContract::get_active_listings`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ListingPage {
    /// Active listings found in this page, in ascending ID order.
    pub listings: soroban_sdk::Vec<ListingEntry>,
    /// The cursor to pass to the next call to continue scanning, or `None`
    /// if the end of the listing range has been reached.
    pub next_cursor: Option<u64>,
}

/// Parameters for a single listing created through
/// [`super::MarketplaceContract::list_batch`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ListingParams {
    /// The NFT contract address.
    pub nft_contract: Address,
    /// The token ID being sold.
    pub token_id: u32,
    /// Asking price in `payment_token` units.
    pub price: i128,
    /// Token the seller accepts as payment.
    pub payment_token: Address,
    /// Optional ledger sequence after which the listing can no longer be bought.
    pub expires_at: Option<u32>,
    /// Pricing model of the listing. Defaults to `Fixed` when omitted.
    pub listing_type: ListingType,
}

/// A bundle listing selling several NFTs of one collection atomically.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BundleListing {
    /// The seller of the bundle.
    pub seller: Address,
    /// The NFT contract address shared by all tokens in the bundle.
    pub nft_contract: Address,
    /// The token IDs sold together.
    pub token_ids: soroban_sdk::Vec<u32>,
    /// Total asking price for the whole bundle in `payment_token` units.
    pub total_price: i128,
    /// Token the seller accepts as payment for the bundle.
    pub payment_token: Address,
    /// Whether the bundle is still open.
    pub active: bool,
}

/// An escrowed collection-wide floor offer: `buyer` will pay `amount` for ANY
/// token of `nft_contract`, and any holder of such a token may accept it.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionOffer {
    /// The buyer whose funds are escrowed.
    pub buyer: Address,
    /// The NFT collection the offer applies to.
    pub nft_contract: Address,
    /// Escrowed amount in payment-token units.
    pub amount: i128,
    /// Ledger sequence after which the offer can no longer be accepted.
    pub expires_at: u32,
}
