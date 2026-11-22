#![no_std]
#![deny(missing_docs)]
//! NFT/asset marketplace contract template.
//!
//! Sellers list assets at a fixed price; buyers purchase them, transferring
//! payment to the seller and the asset to the buyer in one transaction.
//! Sellers may also set an optional expiry on a listing, and buyers may
//! propose a lower price via an escrowed offer that the seller can accept.
//!
//! # Dutch auctions
//! `list_dutch` creates a descending-price listing whose price decays linearly
//! from `starting_price` to `reserve_price` over `duration` seconds of ledger
//! time, then stays at `reserve_price`. `buy` charges the current price.
//!
//! # 3-way sale split
//! Every sale (`buy` or `accept_offer`) is split atomically between the creator
//! royalty (NFT-level `royalty_info`, falling back to the marketplace default),
//! an optional platform fee configured via `set_platform_fee`, and the seller,
//! who receives the exact remainder so no funds are stranded by rounding.

use soroban_sdk::{Address, Env, Vec, contract, contractclient, contractimpl, contracttype, token};

mod errors;
mod events;
mod storage;

pub use errors::MarketplaceError;
pub use storage::{DataKey, DutchAuction, Listing, ListingEntry, ListingPage, SaleSplit};

/// Maximum number of listings a single [`MarketplaceContract::get_active_listings`] call
/// may return, regardless of the requested `limit`.
pub const MAX_LISTINGS_PAGE_SIZE: u32 = 50;

/// Basis-point denominator (10 000 bps = 100 %).
const BPS_DENOMINATOR: i128 = 10_000;

/// `amount * bps / 10_000`, rounded down, with checked arithmetic.
fn bps_of(amount: i128, bps: u32) -> Result<i128, MarketplaceError> {
    amount
        .checked_mul(i128::from(bps))
        .and_then(|v| v.checked_div(BPS_DENOMINATOR))
        .ok_or(MarketplaceError::ArithmeticOverflow)
}

/// Current price of a Dutch auction at ledger timestamp `now`.
///
/// - At or before `start_time` the price is `starting_price`.
/// - Between `start_time` and `start_time + duration` it decays linearly
///   (rounded down towards `reserve_price`).
/// - Once `duration` has elapsed it is clamped to `reserve_price`.
///
/// # Errors
///
/// Returns [`MarketplaceError::ArithmeticOverflow`] if an intermediate value overflows.
pub fn dutch_auction_price(auction: &DutchAuction, now: u64) -> Result<i128, MarketplaceError> {
    let elapsed = now.saturating_sub(auction.start_time);
    if elapsed == 0 {
        return Ok(auction.starting_price);
    }
    if elapsed >= auction.duration {
        return Ok(auction.reserve_price);
    }
    let span = auction
        .starting_price
        .checked_sub(auction.reserve_price)
        .ok_or(MarketplaceError::ArithmeticOverflow)?;
    // duration > elapsed > 0 here, so the division is well defined.
    let decay = span
        .checked_mul(i128::from(elapsed))
        .and_then(|v| v.checked_div(i128::from(auction.duration)))
        .ok_or(MarketplaceError::ArithmeticOverflow)?;
    auction
        .starting_price
        .checked_sub(decay)
        .ok_or(MarketplaceError::ArithmeticOverflow)
}

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_listing(env: &Env, id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Listing(id),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

fn bump_dutch(env: &Env, id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::DutchAuction(id),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

fn bump_offer(env: &Env, id: u64, buyer: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Offer(id, buyer.clone()),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

pub use contract::*;

// The `#[contract]` / `#[contractimpl]` / `#[contractclient]` macros generate
// undocumented public client items. Confine the missing_docs allowance to this
// module and re-export the public contract API above.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    /// Mirrors the `RoyaltyInfo` type returned by `contracts/nft`'s
    /// `royalty_info` (EIP-2981-style). Kept as a local, field-compatible type
    /// rather than a crate dependency so the marketplace stays loosely coupled
    /// to whichever NFT contract it is pointed at.
    #[contracttype]
    #[derive(Clone, Debug, PartialEq)]
    pub struct RoyaltyInfo {
        /// Recipient of the royalty payment.
        pub recipient: Address,
        /// Royalty amount for the given sale price (already computed).
        pub amount: i128,
    }

    /// Minimal interface we need from the NFT contract: transfer_from, and
    /// royalty_info so per-token/per-collection royalties (set on the NFT
    /// contract itself) are honored on sale instead of always falling back to
    /// the marketplace's own `RoyaltyBps`/`RoyaltyRecipient` configuration.
    #[contractclient(name = "NftClient")]
    pub trait NftInterface {
        fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32);
        fn royalty_info(env: Env, token_id: u32, sale_price: i128) -> Option<RoyaltyInfo>;
    }

    /// NFT marketplace contract.
    ///
    /// Lifecycle:
    /// 1. Admin calls `initialize` to set the payment token, royalty BPS (0–10 000), and royalty recipient.
    /// 2. Seller calls `list(nft_contract, token_id, price)` — the seller must first `approve` this
    ///    marketplace contract as a spender on the NFT. `list_with_expiry` additionally sets an
    ///    expiry ledger sequence after which the listing can no longer be bought.
    /// 3. Buyer calls `buy(listing_id)` — pays the seller and the royalty recipient, then the NFT is
    ///    transferred to the buyer. Alternatively, a buyer may `make_offer` below the list price for
    ///    the seller to `accept_offer` later.
    /// 4. Seller may call `cancel(listing_id)` to delist before a sale, or `sweep_expired(listing_id)`
    ///    to reclaim a listing whose expiry has passed.
    #[contract]
    pub struct MarketplaceContract;

    #[contractimpl]
    impl MarketplaceContract {
        /// Initialize the marketplace.
        ///
        /// `royalty_bps` is in basis points (0 = no royalty, 10 000 = 100 %).
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::AlreadyInitialized`] if already initialized.
        /// Returns [`MarketplaceError::InvalidRoyalty`] if `royalty_bps > 10_000`.
        pub fn initialize(
            env: Env,
            admin: Address,
            payment_token: Address,
            royalty_bps: u32,
            royalty_recipient: Address,
        ) -> Result<(), MarketplaceError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::AlreadyInitialized);
            }
            if royalty_bps > 10_000 {
                return Err(MarketplaceError::InvalidRoyalty);
            }

            admin.require_auth();

            // Sanity-check the token address implements the token interface.
            token::Client::new(&env, &payment_token).decimals();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage()
                .instance()
                .set(&DataKey::PaymentToken, &payment_token);
            env.storage()
                .instance()
                .set(&DataKey::RoyaltyBps, &royalty_bps);
            env.storage()
                .instance()
                .set(&DataKey::RoyaltyRecipient, &royalty_recipient);
            env.storage().instance().set(&DataKey::NextListingId, &0u64);
            bump_instance(&env);
            Ok(())
        }

        /// List an NFT for sale.
        ///
        /// The seller must have called `nft_contract.approve(seller, marketplace, token_id, expiry)`
        /// before listing so the marketplace can transfer the NFT on sale.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not yet initialized.
        /// Returns [`MarketplaceError::InvalidPrice`] if `price <= 0`.
        pub fn list(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            price: i128,
        ) -> Result<u64, MarketplaceError> {
            Self::list_impl(env, seller, nft_contract, token_id, price, None)
        }

        /// List an NFT for sale with an expiry ledger sequence. Once `env.ledger().sequence()`
        /// passes `expires_at`, `buy` will reject purchase attempts and the seller may call
        /// `sweep_expired` to reclaim the listing.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not yet initialized.
        /// Returns [`MarketplaceError::InvalidPrice`] if `price <= 0`.
        /// Returns [`MarketplaceError::InvalidExpiry`] if `expires_at` is not in the future.
        pub fn list_with_expiry(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            price: i128,
            expires_at: u32,
        ) -> Result<u64, MarketplaceError> {
            if expires_at <= env.ledger().sequence() {
                return Err(MarketplaceError::InvalidExpiry);
            }
            Self::list_impl(env, seller, nft_contract, token_id, price, Some(expires_at))
        }

        fn list_impl(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            price: i128,
            expires_at: Option<u32>,
        ) -> Result<u64, MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }
            if price <= 0 {
                return Err(MarketplaceError::InvalidPrice);
            }

            seller.require_auth();

            let id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextListingId)
                .unwrap_or(0);

            let listing = Listing {
                nft_contract,
                token_id,
                seller: seller.clone(),
                price,
                active: true,
                expires_at,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Listing(id), &listing);
            env.storage()
                .instance()
                .set(&DataKey::NextListingId, &(id + 1));
            bump_listing(&env, id);
            bump_instance(&env);

            events::listed(&env, id, &seller, price);
            Ok(id)
        }

        /// Set the platform fee taken on every sale. Admin only.
        ///
        /// `fee_bps` is in basis points (0 disables the fee, 10 000 = 100 %).
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::NotAuthorized`] if `admin` is not the marketplace admin.
        /// Returns [`MarketplaceError::InvalidPlatformFee`] if `fee_bps > 10_000`.
        pub fn set_platform_fee(
            env: Env,
            admin: Address,
            fee_bps: u32,
            recipient: Address,
        ) -> Result<(), MarketplaceError> {
            let stored_admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(MarketplaceError::NotInitialized)?;
            if admin != stored_admin {
                return Err(MarketplaceError::NotAuthorized);
            }
            if fee_bps > 10_000 {
                return Err(MarketplaceError::InvalidPlatformFee);
            }

            admin.require_auth();

            env.storage()
                .instance()
                .set(&DataKey::PlatformFeeBps, &fee_bps);
            env.storage()
                .instance()
                .set(&DataKey::PlatformFeeRecipient, &recipient);
            bump_instance(&env);

            events::platform_fee_set(&env, fee_bps, &recipient);
            Ok(())
        }

        /// Return the platform fee in basis points and its recipient, if configured.
        pub fn get_platform_fee(env: Env) -> (u32, Option<Address>) {
            (
                env.storage()
                    .instance()
                    .get(&DataKey::PlatformFeeBps)
                    .unwrap_or(0),
                env.storage().instance().get(&DataKey::PlatformFeeRecipient),
            )
        }

        /// List an NFT as a Dutch (descending-price) auction.
        ///
        /// The price starts at `starting_price` at the current ledger timestamp and decays
        /// linearly to `reserve_price` over `duration` seconds, after which it stays at
        /// `reserve_price` until bought or cancelled. The stored listing's `price` is
        /// `starting_price`; use `get_current_price` for the live price.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not yet initialized.
        /// Returns [`MarketplaceError::InvalidDutchAuction`] if `reserve_price <= 0`,
        /// `starting_price <= reserve_price`, or `duration == 0`.
        pub fn list_dutch(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            starting_price: i128,
            reserve_price: i128,
            duration: u64,
        ) -> Result<u64, MarketplaceError> {
            if reserve_price <= 0 || starting_price <= reserve_price || duration == 0 {
                return Err(MarketplaceError::InvalidDutchAuction);
            }

            let id = Self::list_impl(
                env.clone(),
                seller.clone(),
                nft_contract,
                token_id,
                starting_price,
                None,
            )?;

            let auction = DutchAuction {
                starting_price,
                reserve_price,
                start_time: env.ledger().timestamp(),
                duration,
            };
            env.storage()
                .persistent()
                .set(&DataKey::DutchAuction(id), &auction);
            bump_dutch(&env, id);

            events::dutch_listed(&env, id, &seller, starting_price, reserve_price, duration);
            Ok(id)
        }

        /// Return the Dutch auction parameters of `listing_id`, or `None` if it is a
        /// fixed-price listing (or does not exist).
        pub fn get_dutch_auction(env: Env, listing_id: u64) -> Option<DutchAuction> {
            env.storage()
                .persistent()
                .get(&DataKey::DutchAuction(listing_id))
        }

        /// Return the price `buy` would charge for `listing_id` right now: the decayed
        /// price for a Dutch auction, otherwise the fixed listing price.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        pub fn get_current_price(env: Env, listing_id: u64) -> Result<i128, MarketplaceError> {
            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            Self::current_price(&env, listing_id, &listing)
        }

        /// Preview how a sale of `listing_id` at its current price would be split
        /// between creator, platform, and seller.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist, and
        /// any error [`MarketplaceContract::buy`] would return while computing the split.
        pub fn get_sale_split(env: Env, listing_id: u64) -> Result<SaleSplit, MarketplaceError> {
            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            let price = Self::current_price(&env, listing_id, &listing)?;
            Self::compute_sale_split(&env, &listing, price)
        }

        /// Buy the NFT in listing `listing_id`.
        ///
        /// Charges the current price (the decayed price for a Dutch auction) and splits it
        /// atomically: creator royalty to the royalty recipient, platform fee to the platform,
        /// and the exact remainder to the seller. Then transfers the NFT to the buyer.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if `listing_id` does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if the listing was cancelled or already sold.
        /// Returns [`MarketplaceError::ListingExpired`] if the listing's expiry has passed.
        /// Returns [`MarketplaceError::InvalidRoyalty`] if the NFT reports a negative royalty.
        /// Returns [`MarketplaceError::PayoutExceedsPrice`] if royalty plus fee exceed the price.
        pub fn buy(env: Env, buyer: Address, listing_id: u64) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            buyer.require_auth();

            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if let Some(expires_at) = listing.expires_at {
                if env.ledger().sequence() > expires_at {
                    return Err(MarketplaceError::ListingExpired);
                }
            }

            let price = Self::current_price(&env, listing_id, &listing)?;

            // Checks-effects-interactions: mark inactive before external calls.
            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_listing(&env, listing_id);
            bump_instance(&env);

            let split = Self::compute_sale_split(&env, &listing, price)?;
            Self::disburse(&env, &payment_token, &buyer, &listing.seller, &split);
            events::sale_split(&env, listing_id, &split);

            // Transfer the NFT from seller to buyer.
            NftClient::new(&env, &listing.nft_contract).transfer_from(
                &env.current_contract_address(),
                &listing.seller,
                &buyer,
                &listing.token_id,
            );

            events::sold(&env, listing_id, &buyer, price);
            Ok(())
        }

        /// Cancel a listing. Only the original seller may cancel.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        /// Returns [`MarketplaceError::NotAuthorized`] if caller is not the seller.
        pub fn cancel(env: Env, seller: Address, listing_id: u64) -> Result<(), MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            seller.require_auth();

            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if listing.seller != seller {
                return Err(MarketplaceError::NotAuthorized);
            }

            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_listing(&env, listing_id);
            bump_instance(&env);

            events::cancelled(&env, listing_id, &seller);
            Ok(())
        }

        /// Reclaim a listing whose expiry has passed, marking it inactive so the seller may
        /// re-list. Only the original seller may sweep.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        /// Returns [`MarketplaceError::NotAuthorized`] if caller is not the seller.
        /// Returns [`MarketplaceError::ListingNotExpired`] if the listing has no expiry, or the
        /// expiry hasn't passed yet.
        pub fn sweep_expired(
            env: Env,
            seller: Address,
            listing_id: u64,
        ) -> Result<(), MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            seller.require_auth();

            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if listing.seller != seller {
                return Err(MarketplaceError::NotAuthorized);
            }
            let expires_at = listing
                .expires_at
                .ok_or(MarketplaceError::ListingNotExpired)?;
            if env.ledger().sequence() <= expires_at {
                return Err(MarketplaceError::ListingNotExpired);
            }

            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_instance(&env);

            events::swept(&env, listing_id, &seller);
            Ok(())
        }

        /// Propose an escrowed offer below the listing's asking price. Transfers `amount` from
        /// `buyer` into the marketplace contract until the seller accepts or the buyer cancels.
        /// A later call from the same buyer on the same listing replaces the prior offer, refunding
        /// it first.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        /// Returns [`MarketplaceError::InvalidOfferAmount`] if `amount <= 0` or `amount >= price`.
        pub fn make_offer(
            env: Env,
            buyer: Address,
            listing_id: u64,
            amount: i128,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            buyer.require_auth();

            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            let current_price = Self::current_price(&env, listing_id, &listing)?;
            if amount <= 0 || amount >= current_price {
                return Err(MarketplaceError::InvalidOfferAmount);
            }

            let tok = token::Client::new(&env, &payment_token);

            // Replace any prior offer from this buyer, refunding the escrowed amount first.
            if let Some(prior) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::Offer(listing_id, buyer.clone()))
            {
                tok.transfer(&env.current_contract_address(), &buyer, &prior);
            }

            tok.transfer(&buyer, &env.current_contract_address(), &amount);
            env.storage()
                .persistent()
                .set(&DataKey::Offer(listing_id, buyer.clone()), &amount);
            bump_offer(&env, listing_id, &buyer);
            bump_instance(&env);

            events::offer_made(&env, listing_id, &buyer, amount);
            Ok(())
        }

        /// Accept a buyer's escrowed offer, re-checking it at accept time. Splits the escrowed
        /// amount between creator royalty, platform fee, and seller (exact remainder), then
        /// transfers the NFT to the buyer. Only the original seller may accept.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        /// Returns [`MarketplaceError::NotAuthorized`] if caller is not the seller.
        /// Returns [`MarketplaceError::OfferNotFound`] if `buyer` has no active offer on this listing.
        pub fn accept_offer(
            env: Env,
            seller: Address,
            listing_id: u64,
            buyer: Address,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            seller.require_auth();

            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if listing.seller != seller {
                return Err(MarketplaceError::NotAuthorized);
            }

            // Re-check the escrowed offer at accept time.
            let offer_key = DataKey::Offer(listing_id, buyer.clone());
            let amount: i128 = env
                .storage()
                .persistent()
                .get(&offer_key)
                .ok_or(MarketplaceError::OfferNotFound)?;

            // Checks-effects-interactions: mark inactive and clear the offer before external calls.
            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            env.storage().persistent().remove(&offer_key);
            bump_instance(&env);

            let split = Self::compute_sale_split(&env, &listing, amount)?;
            Self::disburse(
                &env,
                &payment_token,
                &env.current_contract_address(),
                &listing.seller,
                &split,
            );
            events::sale_split(&env, listing_id, &split);

            // Transfer the NFT from seller to buyer.
            NftClient::new(&env, &listing.nft_contract).transfer_from(
                &env.current_contract_address(),
                &listing.seller,
                &buyer,
                &listing.token_id,
            );

            events::offer_accepted(&env, listing_id, &buyer, amount);
            Ok(())
        }

        /// Cancel an unaccepted offer, refunding the escrowed amount to the buyer.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::OfferNotFound`] if `buyer` has no active offer on this listing.
        pub fn cancel_offer(
            env: Env,
            buyer: Address,
            listing_id: u64,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            buyer.require_auth();

            let offer_key = DataKey::Offer(listing_id, buyer.clone());
            let amount: i128 = env
                .storage()
                .persistent()
                .get(&offer_key)
                .ok_or(MarketplaceError::OfferNotFound)?;

            env.storage().persistent().remove(&offer_key);
            bump_instance(&env);

            token::Client::new(&env, &payment_token).transfer(
                &env.current_contract_address(),
                &buyer,
                &amount,
            );

            events::offer_cancelled(&env, listing_id, &buyer);
            Ok(())
        }

        /// Return listing details, or `None` if not found.
        pub fn get_listing(env: Env, listing_id: u64) -> Option<Listing> {
            env.storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
        }

        /// Return the escrowed offer amount from `buyer` on `listing_id`, or `None` if none exists.
        pub fn get_offer(env: Env, listing_id: u64, buyer: Address) -> Option<i128> {
            env.storage()
                .persistent()
                .get(&DataKey::Offer(listing_id, buyer))
        }

        /// Enumerate active listings, paginated by listing ID.
        ///
        /// `cursor` is the listing ID to resume scanning from (pass `0` to start from the
        /// beginning). `limit` is the maximum number of active listings to return and is
        /// capped at [`MAX_LISTINGS_PAGE_SIZE`] (a `limit` of `0` is treated as `1`).
        ///
        /// Returns a page of matching listings together with a `next_cursor` to pass to
        /// the following call, or `next_cursor: None` once the end of the listing range
        /// has been reached.
        pub fn get_active_listings(env: Env, cursor: u64, limit: u32) -> ListingPage {
            let capped_limit = limit.clamp(1, MAX_LISTINGS_PAGE_SIZE);
            let next_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextListingId)
                .unwrap_or(0);

            let mut listings = Vec::new(&env);
            let mut id = cursor;
            let mut next_cursor = None;

            while id < next_id {
                if listings.len() >= capped_limit {
                    next_cursor = Some(id);
                    break;
                }

                if let Some(listing) = env
                    .storage()
                    .persistent()
                    .get::<_, Listing>(&DataKey::Listing(id))
                    && listing.active
                {
                    listings.push_back(ListingEntry { id, listing });
                }

                id = id.saturating_add(1);
            }

            ListingPage {
                listings,
                next_cursor,
            }
        }

        /// Price `buy` charges right now: the decayed Dutch auction price if the listing
        /// is one, otherwise the fixed listing price.
        fn current_price(
            env: &Env,
            listing_id: u64,
            listing: &Listing,
        ) -> Result<i128, MarketplaceError> {
            match env
                .storage()
                .persistent()
                .get::<_, DutchAuction>(&DataKey::DutchAuction(listing_id))
            {
                Some(auction) => dutch_auction_price(&auction, env.ledger().timestamp()),
                None => Ok(listing.price),
            }
        }

        /// Resolve the royalty amount and recipient for a token sale.
        ///
        /// Priority order:
        /// 1. Per-token/collection royalty from the NFT contract (`royalty_info`), if set.
        /// 2. Marketplace-wide royalty (from initialize).
        ///
        /// A zero royalty from either source is valid and results in no creator payout.
        fn resolve_royalty(
            env: &Env,
            nft_contract: &Address,
            token_id: u32,
            sale_price: i128,
        ) -> Result<(i128, Address), MarketplaceError> {
            if let Some(nft_royalty) =
                NftClient::new(env, nft_contract).royalty_info(&token_id, &sale_price)
            {
                if nft_royalty.amount < 0 {
                    return Err(MarketplaceError::InvalidRoyalty);
                }
                return Ok((nft_royalty.amount, nft_royalty.recipient));
            }
            let royalty_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RoyaltyBps)
                .unwrap_or(0);
            let royalty_recipient: Address = env
                .storage()
                .instance()
                .get(&DataKey::RoyaltyRecipient)
                .ok_or(MarketplaceError::NotInitialized)?;
            Ok((bps_of(sale_price, royalty_bps)?, royalty_recipient))
        }

        /// Split `sale_price` into creator royalty, platform fee, and seller remainder.
        ///
        /// Royalty and fee are rounded down; the seller receives exactly
        /// `sale_price - creator_royalty - platform_fee`, so the three parts always sum
        /// to `sale_price`.
        fn compute_sale_split(
            env: &Env,
            listing: &Listing,
            sale_price: i128,
        ) -> Result<SaleSplit, MarketplaceError> {
            let (creator_royalty, creator) =
                Self::resolve_royalty(env, &listing.nft_contract, listing.token_id, sale_price)?;

            let platform: Option<Address> =
                env.storage().instance().get(&DataKey::PlatformFeeRecipient);
            let platform_fee = match platform {
                Some(_) => {
                    let fee_bps: u32 = env
                        .storage()
                        .instance()
                        .get(&DataKey::PlatformFeeBps)
                        .unwrap_or(0);
                    bps_of(sale_price, fee_bps)?
                }
                None => 0,
            };

            let seller_amount = sale_price
                .checked_sub(creator_royalty)
                .and_then(|v| v.checked_sub(platform_fee))
                .ok_or(MarketplaceError::ArithmeticOverflow)?;
            if seller_amount < 0 {
                return Err(MarketplaceError::PayoutExceedsPrice);
            }

            Ok(SaleSplit {
                sale_price,
                creator_royalty,
                creator,
                platform_fee,
                platform,
                seller_amount,
            })
        }

        /// Pay out a computed split from `payer`. Runs inside the calling transaction, so
        /// any failed transfer reverts the whole sale.
        fn disburse(
            env: &Env,
            payment_token: &Address,
            payer: &Address,
            seller: &Address,
            split: &SaleSplit,
        ) {
            let tok = token::Client::new(env, payment_token);
            if split.seller_amount > 0 {
                tok.transfer(payer, seller, &split.seller_amount);
            }
            if split.creator_royalty > 0 {
                tok.transfer(payer, &split.creator, &split.creator_royalty);
            }
            if let Some(platform) = &split.platform
                && split.platform_fee > 0
            {
                tok.transfer(payer, platform, &split.platform_fee);
            }
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;
