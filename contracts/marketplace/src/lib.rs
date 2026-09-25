#![no_std]
#![deny(missing_docs)]
//! NFT/asset marketplace contract template.
//!
//! Sellers list assets at a fixed price in a payment token of their choice;
//! buyers purchase them, transferring payment to the seller and the asset to
//! the buyer in one transaction. Sellers may also set an optional expiry on a
//! listing, and buyers may propose a lower price via an escrowed offer that the
//! seller can accept. Listing, buying and delisting can be batched, and escrowed
//! offers on closed listings can be refunded permissionlessly.
//! Sellers list assets at a fixed price; buyers purchase them, transferring
//! payment to the seller and the asset to the buyer in one transaction.
//! Sellers may also set an optional expiry on a listing, and buyers may
//! propose a lower price via an escrowed offer that the seller can accept.
//! Buyers may also escrow a collection-wide floor offer that any holder of a
//! token from that collection can accept, and the admin may split the
//! marketplace royalty across multiple recipients.

use soroban_sdk::{Address, Env, Vec, contract, contractclient, contractimpl, contracttype, token};

mod errors;
mod events;
mod storage;

pub use errors::MarketplaceError;
pub use storage::{DataKey, Listing, ListingEntry, ListingPage, ListingParams};
pub use storage::{CollectionOffer, DataKey, Listing, ListingEntry, ListingPage};

/// Maximum number of listings a single [`MarketplaceContract::get_active_listings`] call
/// may return, regardless of the requested `limit`.
pub const MAX_LISTINGS_PAGE_SIZE: u32 = 50;

/// Maximum number of items accepted by a single batch entry point
/// (`buy_batch`, `list_batch`, `cancel_batch`, `sweep_offers`).
pub const MAX_BATCH_SIZE: u32 = 20;
/// Maximum number of recipients in a marketplace royalty split, bounding the
/// number of token transfers a single sale performs.
pub const MAX_ROYALTY_RECIPIENTS: u32 = 10;

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

fn bump_collection_offer(env: &Env, id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::CollectionOffer(id),
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

    /// Minimal interface we need from the NFT contract: transfer_from, owner_of
    /// (to detect ghost listings whose seller no longer holds the NFT), and
    /// royalty_info so per-token/per-collection royalties (set on the NFT
    /// contract itself) are honored on sale instead of always falling back to
    /// the marketplace's own `RoyaltyBps`/`RoyaltyRecipient` configuration.
    #[contractclient(name = "NftClient")]
    pub trait NftInterface {
        fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32);
        fn owner_of(env: Env, token_id: u32) -> Address;
        fn royalty_info(env: Env, token_id: u32, sale_price: i128) -> Option<RoyaltyInfo>;
    }

    /// NFT marketplace contract.
    ///
    /// Lifecycle:
    /// 1. Admin calls `initialize` to set the default payment token, royalty BPS (0–10 000), and
    ///    royalty recipient. The admin may additionally maintain a payment-token whitelist
    ///    (`set_payment_token_allowed`, `set_whitelist_enabled`).
    /// 2. Seller calls `list(nft_contract, token_id, price, payment_token)` — the seller must first
    ///    `approve` this marketplace contract as a spender on the NFT. `list_with_expiry`
    ///    additionally sets an expiry ledger sequence after which the listing can no longer be
    ///    bought. `list_batch` creates several listings at once.
    /// 3. Buyer calls `buy(listing_id)` (or `buy_batch`) — pays the seller and the royalty recipient
    ///    in the listing's payment token, then the NFT is transferred to the buyer. Alternatively, a
    ///    buyer may `make_offer` below the list price for the seller to `accept_offer` later.
    /// 4. Seller may call `cancel(listing_id)` (or `cancel_batch`) to delist before a sale, or
    ///    `sweep_expired(listing_id)` to reclaim a listing whose expiry has passed. Anyone may call
    ///    `invalidate_listing` on a listing whose seller no longer owns the NFT, and
    ///    `sweep_offers` to refund escrowed offers on any closed listing.
    /// 1. Admin calls `initialize` to set the payment token, royalty BPS (0–10 000), and royalty recipient.
    /// 2. Seller calls `list(nft_contract, token_id, price)` — the seller must first `approve` this
    ///    marketplace contract as a spender on the NFT. `list_with_expiry` additionally sets an
    ///    expiry ledger sequence after which the listing can no longer be bought.
    /// 3. Buyer calls `buy(listing_id, max_price)` — pays the seller and the royalty recipient(s),
    ///    then the NFT is transferred to the buyer. The purchase is rejected if the listing price
    ///    exceeds `max_price`. Alternatively, a buyer may `make_offer` below the list price for
    ///    the seller to `accept_offer` later, or `make_collection_offer` on a whole collection for
    ///    any holder to `accept_collection_offer`.
    /// 4. Seller may call `cancel(listing_id)` to delist before a sale, or `sweep_expired(listing_id)`
    ///    to reclaim a listing whose expiry has passed.
    #[contract]
    pub struct MarketplaceContract;

    #[contractimpl]
    impl MarketplaceContract {
        /// Initialize the marketplace.
        ///
        /// `payment_token` is the marketplace's default payment token and is automatically
        /// added to the payment-token whitelist. `royalty_bps` is in basis points
        /// (0 = no royalty, 10 000 = 100 %).
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
            env.storage()
                .instance()
                .set(&DataKey::WhitelistEnabled, &false);
            Self::write_token_allowed(&env, &payment_token, true);
            bump_instance(&env);
            Ok(())
        }

        // -----------------------------------------------------------------
        // Payment-token whitelist (admin)
        // -----------------------------------------------------------------

        /// Add (`allowed = true`) or remove (`allowed = false`) a payment token from the
        /// whitelist. Only the admin may call this. Adding a token sanity-checks that it
        /// implements the token interface.
        ///
        /// Removing a token only affects new listings; existing listings in that token
        /// remain purchasable.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        pub fn set_payment_token_allowed(
            env: Env,
            token: Address,
            allowed: bool,
        ) -> Result<(), MarketplaceError> {
            let admin = Self::admin(&env)?;
            admin.require_auth();

            if allowed {
                token::Client::new(&env, &token).decimals();
            }
            Self::write_token_allowed(&env, &token, allowed);
            bump_instance(&env);

            events::payment_token_allowed(&env, &token, allowed);
            Ok(())
        }

        /// Enable or disable enforcement of the payment-token whitelist on new listings.
        /// While disabled (the default), sellers may list in any token. Only the admin
        /// may call this.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        pub fn set_whitelist_enabled(env: Env, enabled: bool) -> Result<(), MarketplaceError> {
            let admin = Self::admin(&env)?;
            admin.require_auth();

            env.storage()
                .instance()
                .set(&DataKey::WhitelistEnabled, &enabled);
            bump_instance(&env);

            events::whitelist_enabled(&env, enabled);
            Ok(())
        }

        /// Return whether the payment-token whitelist is enforced on new listings.
        pub fn is_whitelist_enabled(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&DataKey::WhitelistEnabled)
                .unwrap_or(false)
        }

        /// Return whether `token` is on the payment-token whitelist.
        pub fn is_payment_token_allowed(env: Env, token: Address) -> bool {
            env.storage()
                .persistent()
                .get(&DataKey::AllowedToken(token))
                .unwrap_or(false)
        }

        /// Return the default payment token configured at initialization.
        pub fn get_default_payment_token(env: Env) -> Option<Address> {
            env.storage().instance().get(&DataKey::PaymentToken)
        }

        // -----------------------------------------------------------------
        // Listing
        // -----------------------------------------------------------------

        /// List an NFT for sale, priced in `payment_token`.
        ///
        /// The seller must have called `nft_contract.approve(seller, marketplace, token_id, expiry)`
        /// before listing so the marketplace can transfer the NFT on sale.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not yet initialized.
        /// Returns [`MarketplaceError::InvalidPrice`] if `price <= 0`.
        /// Returns [`MarketplaceError::PaymentTokenNotAllowed`] if the whitelist is enabled and
        /// `payment_token` is not on it.
        pub fn list(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            price: i128,
            payment_token: Address,
        ) -> Result<u64, MarketplaceError> {
            seller.require_auth();
            Self::list_impl(
                &env,
                &seller,
                ListingParams {
                    nft_contract,
                    token_id,
                    price,
                    payment_token,
                    expires_at: None,
                },
            )
        }

        /// List an NFT for sale, priced in `payment_token`, with an expiry ledger sequence.
        /// Once `env.ledger().sequence()` passes `expires_at`, `buy` will reject purchase
        /// attempts and the seller may call `sweep_expired` to reclaim the listing.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not yet initialized.
        /// Returns [`MarketplaceError::InvalidPrice`] if `price <= 0`.
        /// Returns [`MarketplaceError::InvalidExpiry`] if `expires_at` is not in the future.
        /// Returns [`MarketplaceError::PaymentTokenNotAllowed`] if the whitelist is enabled and
        /// `payment_token` is not on it.
        pub fn list_with_expiry(
            env: Env,
            seller: Address,
            nft_contract: Address,
            token_id: u32,
            price: i128,
            payment_token: Address,
            expires_at: u32,
        ) -> Result<u64, MarketplaceError> {
            seller.require_auth();
            Self::list_impl(
                &env,
                &seller,
                ListingParams {
                    nft_contract,
                    token_id,
                    price,
                    payment_token,
                    expires_at: Some(expires_at),
                },
            )
        }

        /// Create several listings for `seller` in one transaction. All-or-nothing: if any
        /// item is invalid, no listing is created. Returns the new listing IDs in input order.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::EmptyBatch`] if `items` is empty.
        /// Returns [`MarketplaceError::BatchTooLarge`] if `items` exceeds [`MAX_BATCH_SIZE`].
        /// Otherwise the same errors as [`Self::list_with_expiry`] for the first failing item.
        pub fn list_batch(
            env: Env,
            seller: Address,
            items: Vec<ListingParams>,
        ) -> Result<Vec<u64>, MarketplaceError> {
            Self::check_batch_len(items.len())?;
            seller.require_auth();

            let mut ids = Vec::new(&env);
            for params in items.iter() {
                ids.push_back(Self::list_impl(&env, &seller, params)?);
            }
            Ok(ids)
        }

        fn list_impl(
            env: &Env,
            seller: &Address,
            params: ListingParams,
        ) -> Result<u64, MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }
            if params.price <= 0 {
                return Err(MarketplaceError::InvalidPrice);
            }
            if let Some(expires_at) = params.expires_at
                && expires_at <= env.ledger().sequence()
            {
                return Err(MarketplaceError::InvalidExpiry);
            }
            if Self::is_whitelist_enabled(env.clone())
                && !Self::is_payment_token_allowed(env.clone(), params.payment_token.clone())
            {
                return Err(MarketplaceError::PaymentTokenNotAllowed);
            }

            let id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextListingId)
                .unwrap_or(0);

            let price = params.price;
            let listing = Listing {
                nft_contract: params.nft_contract,
                token_id: params.token_id,
                seller: seller.clone(),
                price,
                payment_token: params.payment_token,
                active: true,
                expires_at: params.expires_at,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Listing(id), &listing);
            env.storage()
                .instance()
                .set(&DataKey::NextListingId, &(id + 1));
            bump_listing(env, id);
            bump_instance(env);

            events::listed(env, id, seller, price);
            Ok(id)
        }

        // -----------------------------------------------------------------
        // Buying
        // -----------------------------------------------------------------

        /// Buy the NFT in listing `listing_id`.
        /// Buy the NFT in listing `listing_id`, paying at most `max_price`.
        ///
        /// `max_price` protects the buyer against the listing price changing between the time
        /// they sign and the time the transaction executes (e.g. a seller cancelling and
        /// re-listing higher, or a front-run price update). Pass the price you saw.
        ///
        /// Before any payment is taken, verifies that the seller still owns the NFT.
        /// Transfers payment (minus royalty) to the seller and the royalty portion to the royalty
        /// recipient, both in the listing's payment token, then transfers the NFT to the buyer.
        /// recipient(s), then transfers the NFT to the buyer.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if `listing_id` does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if the listing was cancelled or already sold.
        /// Returns [`MarketplaceError::ListingExpired`] if the listing's expiry has passed.
        /// Returns [`MarketplaceError::SellerNotOwner`] if the seller no longer owns the NFT
        /// (a ghost listing); no payment is transferred. Anyone may then call
        /// [`Self::invalidate_listing`] to close it.
        pub fn buy(env: Env, buyer: Address, listing_id: u64) -> Result<(), MarketplaceError> {
            let (royalty_bps, royalty_recipient) = Self::royalty_config(&env)?;
        /// Returns [`MarketplaceError::PriceExceedsMax`] if the listing price exceeds `max_price`.
        pub fn buy(
            env: Env,
            buyer: Address,
            listing_id: u64,
            max_price: i128,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            buyer.require_auth();

            Self::acquire_lock(&env)?;
            Self::buy_impl(&env, &buyer, listing_id, royalty_bps, &royalty_recipient)?;
            Self::release_lock(&env);
            Ok(())
        }

        /// Buy several listings in one transaction (e.g. sweeping a collection floor).
        ///
        /// All-or-nothing: every listing is validated and settled with the same checks as
        /// [`Self::buy`], and if any purchase fails the entire batch reverts, so the buyer is
        /// never left with a partial sweep. Listings may be priced in different payment tokens.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::EmptyBatch`] if `listing_ids` is empty.
        /// Returns [`MarketplaceError::BatchTooLarge`] if `listing_ids` exceeds [`MAX_BATCH_SIZE`].
        /// Otherwise the same errors as [`Self::buy`] for the first failing listing (including
        /// [`MarketplaceError::ListingInactive`] if the same ID appears twice).
        pub fn buy_batch(
            env: Env,
            buyer: Address,
            listing_ids: Vec<u64>,
        ) -> Result<(), MarketplaceError> {
            Self::check_batch_len(listing_ids.len())?;
            let (royalty_bps, royalty_recipient) = Self::royalty_config(&env)?;

            buyer.require_auth();

            Self::acquire_lock(&env)?;
            for listing_id in listing_ids.iter() {
                Self::buy_impl(&env, &buyer, listing_id, royalty_bps, &royalty_recipient)?;
            }
            Self::release_lock(&env);
            Ok(())
        }

        fn buy_impl(
            env: &Env,
            buyer: &Address,
            listing_id: u64,
            royalty_bps: u32,
            royalty_recipient: &Address,
        ) -> Result<(), MarketplaceError> {
            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if let Some(expires_at) = listing.expires_at
                && env.ledger().sequence() > expires_at
            {
                return Err(MarketplaceError::ListingExpired);
            }
            // Ghost-listing check: must happen before any payment is moved.
            if !Self::seller_owns(env, &listing) {
                return Err(MarketplaceError::SellerNotOwner);
            }
            if listing.price > max_price {
                return Err(MarketplaceError::PriceExceedsMax);
            }

            // Checks-effects-interactions: mark inactive before external calls.
            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_listing(env, listing_id);
            bump_instance(env);

            let price = listing.price;
            let (royalty, royalty_recipient) = Self::resolve_royalty(
                env,
                &listing.nft_contract,
                listing.token_id,
                price,
                royalty_bps,
                royalty_recipient,
            );
            #[allow(clippy::arithmetic_side_effects)]
            let seller_amount = price - royalty;

            let tok = token::Client::new(env, &listing.payment_token);
            tok.transfer(buyer, &listing.seller, &seller_amount);
            if royalty > 0 {
                tok.transfer(buyer, &royalty_recipient, &royalty);
            }

            // Transfer the NFT from seller to buyer.
            NftClient::new(env, &listing.nft_contract).transfer_from(
                &env.current_contract_address(),
                &listing.seller,
                buyer,
                &listing.token_id,
            Self::settle_sale(
                &env,
                &payment_token,
                &buyer,
                &listing.seller,
                &buyer,
                &listing.nft_contract,
                listing.token_id,
                price,
            );

            events::sold(env, listing_id, buyer, price);
            Ok(())
        }

        // -----------------------------------------------------------------
        // Delisting
        // -----------------------------------------------------------------

        /// Cancel a listing. Only the original seller may cancel.
        ///
        /// Escrowed offers on the listing are not refunded automatically; anyone may return
        /// them to their buyers with [`Self::sweep_offers`].
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

            Self::cancel_impl(&env, &seller, listing_id)
        }

        /// Cancel several listings in one transaction. All-or-nothing: if any listing cannot
        /// be cancelled by `seller`, none are.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::EmptyBatch`] if `listing_ids` is empty.
        /// Returns [`MarketplaceError::BatchTooLarge`] if `listing_ids` exceeds [`MAX_BATCH_SIZE`].
        /// Otherwise the same errors as [`Self::cancel`] for the first failing listing.
        pub fn cancel_batch(
            env: Env,
            seller: Address,
            listing_ids: Vec<u64>,
        ) -> Result<(), MarketplaceError> {
            Self::check_batch_len(listing_ids.len())?;
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            seller.require_auth();

            for listing_id in listing_ids.iter() {
                Self::cancel_impl(&env, &seller, listing_id)?;
            }
            Ok(())
        }

        fn cancel_impl(env: &Env, seller: &Address, listing_id: u64) -> Result<(), MarketplaceError> {
            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if listing.seller != *seller {
                return Err(MarketplaceError::NotAuthorized);
            }

            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_listing(env, listing_id);
            bump_instance(env);

            events::cancelled(env, listing_id, seller);
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

        /// Close a ghost listing: one whose seller no longer owns the NFT (it was transferred
        /// away or burned). Permissionless, so buyers, indexers or keepers can clean up stale
        /// listings. Returns `true` if the listing was invalidated, `false` if the seller still
        /// owns the NFT (the listing is left untouched).
        ///
        /// A failing `buy` cannot persist this itself because a contract error reverts all of
        /// the call's state changes, so it is exposed as its own entry point.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        pub fn invalidate_listing(env: Env, listing_id: u64) -> Result<bool, MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            let mut listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if Self::seller_owns(&env, &listing) {
                return Ok(false);
            }

            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            bump_listing(&env, listing_id);
            bump_instance(&env);

            events::listing_invalidated(&env, listing_id, &listing.seller);
            Ok(true)
        }

        // -----------------------------------------------------------------
        // Offers
        // -----------------------------------------------------------------

        /// Propose an escrowed offer below the listing's asking price, in the listing's payment
        /// token. Transfers `amount` from `buyer` into the marketplace contract until the seller
        /// accepts or the buyer cancels. A later call from the same buyer on the same listing
        /// replaces the prior offer, refunding it first.
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
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            buyer.require_auth();

            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            if !listing.active {
                return Err(MarketplaceError::ListingInactive);
            }
            if amount <= 0 || amount >= listing.price {
                return Err(MarketplaceError::InvalidOfferAmount);
            }

            let tok = token::Client::new(&env, &listing.payment_token);

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

        /// Accept a buyer's escrowed offer, re-checking it at accept time. Transfers the escrowed
        /// amount (minus royalty) to the seller and the royalty portion to the royalty recipient,
        /// in the listing's payment token, then transfers the NFT to the buyer. Only the original
        /// seller may accept.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingInactive`] if already sold or cancelled.
        /// Returns [`MarketplaceError::NotAuthorized`] if caller is not the seller.
        /// Returns [`MarketplaceError::OfferNotFound`] if `buyer` has no active offer on this listing.
        /// Returns [`MarketplaceError::SellerNotOwner`] if the seller no longer owns the NFT.
        pub fn accept_offer(
            env: Env,
            seller: Address,
            listing_id: u64,
            buyer: Address,
        ) -> Result<(), MarketplaceError> {
            let (royalty_bps, royalty_recipient) = Self::royalty_config(&env)?;
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

            if !Self::seller_owns(&env, &listing) {
                return Err(MarketplaceError::SellerNotOwner);
            }

            Self::acquire_lock(&env)?;

            // Checks-effects-interactions: mark inactive and clear the offer before external calls.
            listing.active = false;
            env.storage()
                .persistent()
                .set(&DataKey::Listing(listing_id), &listing);
            env.storage().persistent().remove(&offer_key);
            bump_instance(&env);

            Self::settle_sale(
                &env,
                &listing.nft_contract,
                listing.token_id,
                amount,
                royalty_bps,
                &royalty_recipient,
            );
            #[allow(clippy::arithmetic_side_effects)]
            let seller_amount = amount - royalty;

            let tok = token::Client::new(&env, &listing.payment_token);
            tok.transfer(
                &env.current_contract_address(),
                &listing.seller,
                &seller_amount,
            );
            if royalty > 0 {
                tok.transfer(&env.current_contract_address(), &royalty_recipient, &royalty);
            }

            // Transfer the NFT from seller to buyer.
            NftClient::new(&env, &listing.nft_contract).transfer_from(
                &payment_token,
                &env.current_contract_address(),
                &listing.seller,
                &buyer,
                &listing.nft_contract,
                listing.token_id,
                amount,
            );

            Self::release_lock(&env);
            events::offer_accepted(&env, listing_id, &buyer, amount);
            Ok(())
        }

        /// Cancel an unaccepted offer, refunding the escrowed amount to the buyer in the
        /// listing's payment token.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::OfferNotFound`] if `buyer` has no active offer on this listing.
        pub fn cancel_offer(
            env: Env,
            buyer: Address,
            listing_id: u64,
        ) -> Result<(), MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }

            buyer.require_auth();

            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;

            let offer_key = DataKey::Offer(listing_id, buyer.clone());
            let amount: i128 = env
                .storage()
                .persistent()
                .get(&offer_key)
                .ok_or(MarketplaceError::OfferNotFound)?;

            env.storage().persistent().remove(&offer_key);
            bump_instance(&env);

            token::Client::new(&env, &listing.payment_token).transfer(
                &env.current_contract_address(),
                &buyer,
                &amount,
            );

            events::offer_cancelled(&env, listing_id, &buyer);
            Ok(())
        }

        /// Refund escrowed offers from `buyers` on a listing that is no longer active
        /// (cancelled, swept, invalidated or sold). Permissionless: anyone may trigger the
        /// refund, and funds always go back to the buyer who made the offer, so buyers do not
        /// need to notice the listing closed and call `cancel_offer` themselves.
        ///
        /// Buyers without an offer on the listing are skipped. Emits `offer_refunded` for each
        /// refunded offer and returns the number of offers refunded.
        /// Escrow a collection-wide floor offer: `buyer` commits to pay `amount` for ANY token of
        /// `nft_contract`. Any holder of such a token may accept it via `accept_collection_offer`
        /// until `expires_at` (a ledger sequence). The buyer may cancel at any time — including
        /// after expiry — to recover the escrowed funds.
        ///
        /// Returns the new collection offer ID.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::EmptyBatch`] if `buyers` is empty.
        /// Returns [`MarketplaceError::BatchTooLarge`] if `buyers` exceeds [`MAX_BATCH_SIZE`].
        /// Returns [`MarketplaceError::ListingNotFound`] if the listing does not exist.
        /// Returns [`MarketplaceError::ListingStillActive`] if the listing is still open.
        pub fn sweep_offers(
            env: Env,
            listing_id: u64,
            buyers: Vec<Address>,
        ) -> Result<u32, MarketplaceError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(MarketplaceError::NotInitialized);
            }
            Self::check_batch_len(buyers.len())?;

            let listing: Listing = env
                .storage()
                .persistent()
                .get(&DataKey::Listing(listing_id))
                .ok_or(MarketplaceError::ListingNotFound)?;
            if listing.active {
                return Err(MarketplaceError::ListingStillActive);
            }

            Self::acquire_lock(&env)?;

            let tok = token::Client::new(&env, &listing.payment_token);
            let mut refunded: u32 = 0;
            for buyer in buyers.iter() {
                let offer_key = DataKey::Offer(listing_id, buyer.clone());
                let Some(amount) = env.storage().persistent().get::<_, i128>(&offer_key) else {
                    continue;
                };

                // Clear the offer before transferring so a duplicate buyer in `buyers`
                // cannot be refunded twice.
                env.storage().persistent().remove(&offer_key);
                tok.transfer(&env.current_contract_address(), &buyer, &amount);

                events::offer_refunded(&env, listing_id, &buyer, amount);
                refunded = refunded.saturating_add(1);
            }

            Self::release_lock(&env);
            bump_instance(&env);
            Ok(refunded)
        }

        // -----------------------------------------------------------------
        // Queries
        // -----------------------------------------------------------------
        /// Returns [`MarketplaceError::InvalidOfferAmount`] if `amount <= 0`.
        /// Returns [`MarketplaceError::InvalidExpiry`] if `expires_at` is not in the future.
        pub fn make_collection_offer(
            env: Env,
            buyer: Address,
            nft_contract: Address,
            amount: i128,
            expires_at: u32,
        ) -> Result<u64, MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;
            if amount <= 0 {
                return Err(MarketplaceError::InvalidOfferAmount);
            }
            if expires_at <= env.ledger().sequence() {
                return Err(MarketplaceError::InvalidExpiry);
            }

            buyer.require_auth();

            let id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextCollectionOfferId)
                .unwrap_or(0);

            token::Client::new(&env, &payment_token).transfer(
                &buyer,
                &env.current_contract_address(),
                &amount,
            );

            let offer = CollectionOffer {
                buyer: buyer.clone(),
                nft_contract: nft_contract.clone(),
                amount,
                expires_at,
            };
            env.storage()
                .persistent()
                .set(&DataKey::CollectionOffer(id), &offer);
            env.storage()
                .instance()
                .set(&DataKey::NextCollectionOfferId, &id.saturating_add(1));
            bump_collection_offer(&env, id);
            bump_instance(&env);

            events::collection_offer_made(&env, id, &buyer, &nft_contract, amount);
            Ok(id)
        }

        /// Accept a collection floor offer by selling `token_id` of the offer's collection.
        ///
        /// `seller` must own `token_id` and must have approved this marketplace as a spender on
        /// the NFT contract. The escrowed amount (minus royalty) goes to the seller, the royalty
        /// portion to the royalty recipient(s), and the NFT is transferred to the offer's buyer.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::CollectionOfferNotFound`] if the offer does not exist
        /// (never made, already accepted, or cancelled).
        /// Returns [`MarketplaceError::CollectionOfferExpired`] if the offer's expiry has passed.
        /// Returns [`MarketplaceError::NotAuthorized`] if `seller` is the offer's buyer.
        pub fn accept_collection_offer(
            env: Env,
            seller: Address,
            offer_id: u64,
            token_id: u32,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            seller.require_auth();

            let key = DataKey::CollectionOffer(offer_id);
            let offer: CollectionOffer = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(MarketplaceError::CollectionOfferNotFound)?;
            if env.ledger().sequence() > offer.expires_at {
                return Err(MarketplaceError::CollectionOfferExpired);
            }
            if offer.buyer == seller {
                return Err(MarketplaceError::NotAuthorized);
            }

            // Checks-effects-interactions: clear the offer before external calls.
            env.storage().persistent().remove(&key);
            bump_instance(&env);

            Self::settle_sale(
                &env,
                &payment_token,
                &env.current_contract_address(),
                &seller,
                &offer.buyer,
                &offer.nft_contract,
                token_id,
                offer.amount,
            );

            events::collection_offer_accepted(&env, offer_id, &seller, token_id, offer.amount);
            Ok(())
        }

        /// Cancel a collection floor offer, refunding the escrowed amount to the buyer. Allowed
        /// at any time, including after expiry, so escrowed funds can never be trapped.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::CollectionOfferNotFound`] if the offer does not exist.
        /// Returns [`MarketplaceError::NotAuthorized`] if `buyer` did not make the offer.
        pub fn cancel_collection_offer(
            env: Env,
            buyer: Address,
            offer_id: u64,
        ) -> Result<(), MarketplaceError> {
            let payment_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::PaymentToken)
                .ok_or(MarketplaceError::NotInitialized)?;

            buyer.require_auth();

            let key = DataKey::CollectionOffer(offer_id);
            let offer: CollectionOffer = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(MarketplaceError::CollectionOfferNotFound)?;
            if offer.buyer != buyer {
                return Err(MarketplaceError::NotAuthorized);
            }

            env.storage().persistent().remove(&key);
            bump_instance(&env);

            token::Client::new(&env, &payment_token).transfer(
                &env.current_contract_address(),
                &buyer,
                &offer.amount,
            );

            events::collection_offer_cancelled(&env, offer_id, &buyer);
            Ok(())
        }

        /// Return a collection floor offer, or `None` if it does not exist.
        pub fn get_collection_offer(env: Env, offer_id: u64) -> Option<CollectionOffer> {
            env.storage()
                .persistent()
                .get(&DataKey::CollectionOffer(offer_id))
        }

        /// Set (or clear, with an empty `splits`) the marketplace-wide multi-recipient royalty
        /// split. Each entry is `(recipient, bps)`; the entries' basis points sum to the total
        /// marketplace royalty. When set, it supersedes the single `royalty_bps`/
        /// `royalty_recipient` configured at `initialize`. A royalty reported by the NFT
        /// contract's `royalty_info` still takes priority over both.
        ///
        /// # Errors
        ///
        /// Returns [`MarketplaceError::NotInitialized`] if not initialized.
        /// Returns [`MarketplaceError::NotAuthorized`] if `admin` is not the marketplace admin.
        /// Returns [`MarketplaceError::InvalidRoyalty`] if there are more than
        /// [`MAX_ROYALTY_RECIPIENTS`] entries, any entry has `0` bps, or the total exceeds 10 000.
        pub fn set_royalty_splits(
            env: Env,
            admin: Address,
            splits: Vec<(Address, u32)>,
        ) -> Result<(), MarketplaceError> {
            let stored_admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(MarketplaceError::NotInitialized)?;
            if admin != stored_admin {
                return Err(MarketplaceError::NotAuthorized);
            }

            admin.require_auth();

            if splits.len() > MAX_ROYALTY_RECIPIENTS {
                return Err(MarketplaceError::InvalidRoyalty);
            }
            let mut total_bps: u32 = 0;
            for (_, bps) in splits.iter() {
                if bps == 0 {
                    return Err(MarketplaceError::InvalidRoyalty);
                }
                total_bps = total_bps
                    .checked_add(bps)
                    .ok_or(MarketplaceError::InvalidRoyalty)?;
            }
            if total_bps > 10_000 {
                return Err(MarketplaceError::InvalidRoyalty);
            }

            if splits.is_empty() {
                env.storage().instance().remove(&DataKey::RoyaltySplits);
            } else {
                env.storage()
                    .instance()
                    .set(&DataKey::RoyaltySplits, &splits);
            }
            bump_instance(&env);

            events::royalty_splits_set(&env, &admin, total_bps);
            Ok(())
        }

        /// Return the marketplace-wide royalty split, or an empty vector if none is set.
        pub fn get_royalty_splits(env: Env) -> Vec<(Address, u32)> {
            env.storage()
                .instance()
                .get(&DataKey::RoyaltySplits)
                .unwrap_or_else(|| Vec::new(&env))
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
        /// Ghost listings — active listings whose seller no longer owns the NFT — are
        /// filtered out.
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
                    && Self::seller_owns(&env, &listing)
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

        // -----------------------------------------------------------------
        // Internal helpers
        // -----------------------------------------------------------------

        fn admin(env: &Env) -> Result<Address, MarketplaceError> {
            env.storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(MarketplaceError::NotInitialized)
        }

        fn royalty_config(env: &Env) -> Result<(u32, Address), MarketplaceError> {
            let royalty_recipient: Address = env
                .storage()
                .instance()
                .get(&DataKey::RoyaltyRecipient)
                .ok_or(MarketplaceError::NotInitialized)?;
            let royalty_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RoyaltyBps)
                .unwrap_or(0);
            Ok((royalty_bps, royalty_recipient))
        }

        fn write_token_allowed(env: &Env, token: &Address, allowed: bool) {
            let key = DataKey::AllowedToken(token.clone());
            if allowed {
                env.storage().persistent().set(&key, &true);
                env.storage().persistent().extend_ttl(
                    &key,
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
            } else {
                env.storage().persistent().remove(&key);
            }
        }

        fn check_batch_len(len: u32) -> Result<(), MarketplaceError> {
            if len == 0 {
                return Err(MarketplaceError::EmptyBatch);
            }
            if len > MAX_BATCH_SIZE {
                return Err(MarketplaceError::BatchTooLarge);
            }
            Ok(())
        }

        /// Take the reentrancy guard. Soroban's host already rejects re-entry into the
        /// same contract; this is defence in depth for flows that move tokens and NFTs
        /// through external contracts. If the call later fails, the host reverts the
        /// storage write, so the guard can never stay stuck.
        fn acquire_lock(env: &Env) -> Result<(), MarketplaceError> {
            if env.storage().instance().has(&DataKey::Locked) {
                return Err(MarketplaceError::Reentrant);
            }
            env.storage().instance().set(&DataKey::Locked, &true);
            Ok(())
        }

        fn release_lock(env: &Env) {
            env.storage().instance().remove(&DataKey::Locked);
        }

        /// Whether the listing's seller still owns the listed NFT. A failing or
        /// panicking `owner_of` (e.g. the token was burned) counts as "not owned".
        fn seller_owns(env: &Env, listing: &Listing) -> bool {
            matches!(
                NftClient::new(env, &listing.nft_contract).try_owner_of(&listing.token_id),
                Ok(Ok(owner)) if owner == listing.seller
            )
        }

        /// Resolve the royalty amount and recipient for a token sale.
        /// Resolve the royalty payments for a token sale as `(recipient, amount)` pairs.
        ///
        /// Priority order:
        /// 1. Per-token/collection royalty from the NFT contract (`royalty_info`), if set.
        /// 2. Marketplace-wide multi-recipient split (`set_royalty_splits`), if set.
        /// 3. Marketplace-wide single royalty (from initialize).
        ///
        /// Each split share is floored independently; rounding dust stays with the seller.
        fn resolve_royalty(
            env: &Env,
            nft_contract: &Address,
            token_id: u32,
            sale_price: i128,
        ) -> Vec<(Address, i128)> {
            let mut payments = Vec::new(env);
            if let Some(nft_royalty) = NftClient::new(env, nft_contract)
                .royalty_info(&token_id, &sale_price)
            {
                payments.push_back((nft_royalty.recipient, nft_royalty.amount));
                return payments;
            }

            let splits: Vec<(Address, u32)> = match env
                .storage()
                .instance()
                .get::<_, Vec<(Address, u32)>>(&DataKey::RoyaltySplits)
            {
                Some(splits) if !splits.is_empty() => splits,
                _ => {
                    let bps: u32 = env
                        .storage()
                        .instance()
                        .get(&DataKey::RoyaltyBps)
                        .unwrap_or(0);
                    let mut single = Vec::new(env);
                    if let Some(recipient) = env
                        .storage()
                        .instance()
                        .get::<_, Address>(&DataKey::RoyaltyRecipient)
                    {
                        single.push_back((recipient, bps));
                    }
                    single
                }
            };

            for (recipient, bps) in splits.iter() {
                #[allow(clippy::arithmetic_side_effects, clippy::integer_division)] // bps validated <= 10_000
                let amount = (sale_price * i128::from(bps)) / 10_000;
                payments.push_back((recipient, amount));
            }
            payments
        }

        /// Pay `price` from `payer` — the seller's share to `seller` and royalties to their
        /// recipients — then transfer `token_id` of `nft_contract` from `seller` to `nft_recipient`.
        #[allow(clippy::too_many_arguments)]
        fn settle_sale(
            env: &Env,
            payment_token: &Address,
            payer: &Address,
            seller: &Address,
            nft_recipient: &Address,
            nft_contract: &Address,
            token_id: u32,
            price: i128,
        ) {
            let royalties = Self::resolve_royalty(env, nft_contract, token_id, price);
            let mut royalty_total: i128 = 0;
            for (_, amount) in royalties.iter() {
                royalty_total = royalty_total.saturating_add(amount);
            }
            #[allow(clippy::arithmetic_side_effects)]
            let seller_amount = price - royalty_total;

            let tok = token::Client::new(env, payment_token);
            tok.transfer(payer, seller, &seller_amount);
            for (recipient, amount) in royalties.iter() {
                if amount > 0 {
                    tok.transfer(payer, &recipient, &amount);
                }
            }

            // Transfer the NFT from seller to the buyer.
            NftClient::new(env, nft_contract).transfer_from(
                &env.current_contract_address(),
                seller,
                nft_recipient,
                &token_id,
            );
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;
