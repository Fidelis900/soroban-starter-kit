// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Payment token address (instance).
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
    /// Platform fee in basis points taken on every sale (instance).
    PlatformFeeBps,
    /// Platform fee recipient address (instance).
    PlatformFeeRecipient,
    /// Dutch auction pricing for a listing, if it is one (persistent).
    DutchAuction(u64),
}

/// Descending-price (Dutch auction) parameters attached to a listing.
///
/// The price decays linearly from `starting_price` at `start_time` to
/// `reserve_price` at `start_time + duration`, and stays at `reserve_price`
/// afterwards.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DutchAuction {
    /// Price at `start_time`.
    pub starting_price: i128,
    /// Floor price, reached once `duration` seconds have elapsed.
    pub reserve_price: i128,
    /// Ledger timestamp (seconds) at which the decay starts.
    pub start_time: u64,
    /// Length of the decay in seconds.
    pub duration: u64,
}

/// How a sale price is divided between seller, creator, and platform.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SaleSplit {
    /// Total paid by the buyer.
    pub sale_price: i128,
    /// Royalty paid to `creator`.
    pub creator_royalty: i128,
    /// Royalty recipient (NFT-level royalty if set, otherwise the marketplace default).
    pub creator: Address,
    /// Fee paid to `platform`.
    pub platform_fee: i128,
    /// Platform fee recipient, or `None` when no platform fee is configured.
    pub platform: Option<Address>,
    /// Exact remainder paid to the seller:
    /// `sale_price - creator_royalty - platform_fee`.
    pub seller_amount: i128,
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
    /// Asking price in payment-token units.
    pub price: i128,
    /// Whether the listing is still open.
    pub active: bool,
    /// Optional ledger sequence after which the listing can no longer be bought.
    pub expires_at: Option<u32>,
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
