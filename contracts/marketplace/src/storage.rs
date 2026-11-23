// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype, contractclient};

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
    /// Escrowed offer for (listing_id, buyer) (persistent).
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
    /// Optional staking contract used to compute fee discounts (instance).
    StakingContract,
    /// Minimum counter-offer increment over the current highest offer, in
    /// basis points, e.g. 500 = +5 % (instance).
    MinCounterOfferBps,
    /// Highest offer amount seen on a listing, for the counter-offer check (persistent).
    HighestOffer(u64),
}

/// Pricing model of a listing.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ListingType {
    /// A fixed asking price.
    Fixed,
    /// A Dutch auction whose price decays linearly; see [`DutchConfig`].
    Dutch(DutchConfig),
}

/// Price-decay schedule of a Dutch listing: the price falls linearly from
/// `start_price` at `start_ledger` down to `end_price` at `end_ledger`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DutchConfig {
    /// Price at `start_ledger`.
    pub start_price: i128,
    /// Price at (and after) `end_ledger`.
    pub end_price: i128,
    /// Ledger sequence at which the auction starts.
    pub start_ledger: u32,
    /// Ledger sequence at which the price reaches `end_price`.
    pub end_ledger: u32,
}

/// An escrowed offer on a specific listing.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// The buyer whose funds are escrowed.
    pub buyer: Address,
    /// Escrowed amount in the listing's payment-token units.
    pub amount: i128,
    /// Ledger sequence after which the offer can no longer be accepted.
    pub expiration_ledger: u32,
}

/// Minimal interface to the platform staking contract, used to compute
/// fee discounts for stakers.
#[contractclient(name = "StakingContractClient")]
pub trait StakingContract {
    /// Returns the amount of platform tokens staked by `account`.
    fn staked_balance(env: soroban_sdk::Env, account: Address) -> i128;
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
    /// Optional ledger timestamp (Unix seconds) after which the listing can no
    /// longer be bought: a purchase requires `env.ledger().timestamp() <= expiration`.
    pub expiration: Option<u64>,
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
