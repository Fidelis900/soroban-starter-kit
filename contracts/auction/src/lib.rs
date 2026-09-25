#![no_std]
#![deny(missing_docs)]
//! Sealed-deadline auction contract template.
//!
//! A seller lists an item; bidders submit increasing bids until the deadline,
//! outbid bidders reclaim their funds, and anyone can settle the auction once
//! the deadline passes.
//!
//! # Anti-sniping (issue #784)
//! An optional `extension_window` can be configured at `start`. When a bid
//! arrives within `extension_window` ledgers of the current `deadline`, the
//! deadline is extended by `extension_window` ledgers (preventing last-second
//! snipe bids that leave no time to counter-bid).
//!
//! # Seller cancellation (issues #785, #1072)
//! The seller may call `cancel` at any time before a bid has been placed.
//!
//! Once a bid exists, cancellation is only possible inside the optional
//! cancellation grace window configured at `start`
//! (`current_ledger <= start_ledger + cancellation_grace_ledgers`). A
//! grace-window cancellation with a live top bidder requires the seller to pay
//! `cancellation_fee` into the contract; the top bidder's full bid **plus** the
//! fee are credited to their pending refund (claimable via `withdraw`) and an
//! `AuctionCancelledWithCompensation` event is emitted.
//!
//! Cancellation state machine:
//!
//! ```text
//!   Active ── cancel (no bids) ─────────────────────────▶ Cancelled
//!     │                                                   (no transfer)
//!     │ bid
//!     ▼
//!   Active+Bids ── cancel, ledger <= start + grace ─────▶ Cancelled
//!     │            (seller pays fee into contract)        (Pending(top) += bid + fee)
//!     │
//!     ├── cancel, grace = 0 or ledger > start + grace ──▶ Err(BidAlreadyPlaced)
//!     │
//!     │ ledger > deadline
//!     ▼
//!   end ────────────────────────────────────────────────▶ Settled
//! ```
//!
//! A cancelled auction rejects further `bid` (`AuctionEnded`) and `end`
//! (`AlreadyEnded`) calls; outbid and compensated bidders always recover their
//! funds with `withdraw`.
//!
//! # Reserve price (issue #783)
//! An optional `reserve_price` can be set at `start`. When set, `end()` will
//! only transfer to the seller if `highest_bid >= reserve_price`; otherwise
//! the highest bidder's funds are returned.
//!
//! # Checked arithmetic (issue #1070)
//! Every bid, refund, credit, and price computation uses checked operations
//! and returns [`AuctionError::Overflow`] instead of trapping the host.
//!
//! # Custodial NFT escrow (issue #1069)
//! `start` and `start_dutch` accept an optional `nft_contract` / `token_id`
//! pair. The NFT is pulled into the contract at start, delivered to the winner
//! on a successful settlement, and returned to the seller on cancellation, no
//! bids, or an unmet reserve.
//!
//! # Counter-bidding with refund credit (issue #1068)
//! `bid_with_credit` lets an outbid bidder apply their pending refund toward a
//! new bid, transferring only `total_bid - pending_credit` from their balance.
//!
//! # Dutch auction (issue #1071)
//! `start_dutch` configures a descending-price schedule. `get_current_price`
//! returns the linearly decaying price and `buy` settles immediately at that
//! price.

#[cfg(test)]
extern crate std;

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod dutch;
mod errors;
mod events;
mod nft;
mod storage;

pub use errors::AuctionError;
pub use events::AuctionCancelledWithCompensation;
pub use storage::{AuctionInfo, DataKey};
pub use storage::{AuctionInfo, DataKey, DutchConfig};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn get_instance<T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Result<T, AuctionError> {
    env.storage()
        .instance()
        .get(key)
        .ok_or(AuctionError::NotInitialized)
}

fn get_flag(env: &Env, key: &DataKey) -> bool {
    env.storage().instance().get(key).unwrap_or(false)
}

fn dutch_config(env: &Env) -> Option<DutchConfig> {
    env.storage().instance().get(&DataKey::DutchConfig)
}

fn pending_of(env: &Env, bidder: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Pending(bidder.clone()))
        .unwrap_or(0)
}

/// Store `amount` as `bidder`'s pending refund, removing the entry when it is zero.
fn set_pending(env: &Env, bidder: &Address, amount: i128) {
    let key = DataKey::Pending(bidder.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        env.storage().persistent().extend_ttl(
            &key,
            LEDGER_LIFETIME_THRESHOLD,
            LEDGER_BUMP_AMOUNT,
        );
    }
}

/// Reject a half-specified NFT escrow before any state is written.
fn validate_nft_params(
    nft_contract: &Option<Address>,
    token_id: &Option<u32>,
) -> Result<(), AuctionError> {
    if nft_contract.is_some() == token_id.is_some() {
        Ok(())
    } else {
        Err(AuctionError::InvalidNftParams)
    }
}

/// Pull the NFT from `seller` into the auction contract (custodial escrow).
fn escrow_nft(env: &Env, seller: &Address, nft_contract: Option<Address>, token_id: Option<u32>) {
    if let (Some(nft_contract), Some(token_id)) = (nft_contract, token_id) {
        env.storage()
            .instance()
            .set(&DataKey::NftContract, &nft_contract);
        env.storage()
            .instance()
            .set(&DataKey::NftTokenId, &token_id);
        nft::NftClient::new(env, &nft_contract).transfer(
            seller,
            &env.current_contract_address(),
            &token_id,
        );
        events::nft_escrowed(env, &nft_contract, token_id);
    }
}

/// Deliver the escrowed NFT (if any) to `to`. Callers guarantee this runs at
/// most once by setting `Settled` / `Cancelled` before calling it.
fn release_nft(env: &Env, to: &Address) {
    let nft_contract: Option<Address> = env.storage().instance().get(&DataKey::NftContract);
    let token_id: Option<u32> = env.storage().instance().get(&DataKey::NftTokenId);
    if let (Some(nft_contract), Some(token_id)) = (nft_contract, token_id) {
        nft::NftClient::new(env, &nft_contract).transfer(
            &env.current_contract_address(),
            to,
            &token_id,
        );
        events::nft_released(env, to, token_id);
    }
}

/// Shared English-auction bidding path for `bid` and `bid_with_credit`.
///
/// All balances are computed with checked arithmetic and all storage effects
/// are applied before the single token transfer (checks-effects-interactions).
fn place_bid(
    env: &Env,
    bidder: &Address,
    amount: i128,
    use_credit: bool,
) -> Result<(), AuctionError> {
    if amount <= 0 {
        return Err(AuctionError::InvalidAmount);
    }
    if dutch_config(env).is_some() {
        return Err(AuctionError::WrongMode);
    }
    if get_flag(env, &DataKey::Cancelled) {
        return Err(AuctionError::AuctionEnded);
    }

    let mut deadline: u32 = get_instance(env, &DataKey::Deadline)?;
    if env.ledger().sequence() > deadline {
        return Err(AuctionError::AuctionEnded);
    }

    let highest_bid: i128 = get_instance(env, &DataKey::HighestBid)?;
    let min_increment: i128 = get_instance(env, &DataKey::MinIncrement)?;
    let start_price: i128 = get_instance(env, &DataKey::StartPrice)?;

    // First bid must be >= start_price; subsequent bids must be >= highest_bid + min_increment
    let min_required = if highest_bid < start_price {
        start_price
    } else {
        highest_bid
            .checked_add(min_increment)
            .ok_or(AuctionError::Overflow)?
    };

    if amount < min_required {
        return Err(AuctionError::BidTooLow);
    }

    bidder.require_auth();

    // Queue previous highest bidder's refund
    let prev_bidder: Option<Address> = env.storage().instance().get(&DataKey::HighestBidder);
    if let Some(prev) = prev_bidder {
        let new_pending = pending_of(env, &prev)
            .checked_add(highest_bid)
            .ok_or(AuctionError::Overflow)?;
        set_pending(env, &prev, new_pending);
    }

    // Apply pending refund credit (read after queueing so a bidder raising their
    // own winning bid can also offset it).
    let mut required_transfer = amount;
    if use_credit {
        let credit = pending_of(env, bidder);
        let credit_used = credit.min(amount);
        required_transfer = amount.checked_sub(credit_used).ok_or(AuctionError::Overflow)?;
        let remaining = credit.checked_sub(credit_used).ok_or(AuctionError::Overflow)?;
        set_pending(env, bidder, remaining);
        events::credit_applied(env, bidder, credit_used, required_transfer);
    }

    env.storage()
        .instance()
        .set(&DataKey::HighestBidder, bidder);
    env.storage().instance().set(&DataKey::HighestBid, &amount);

    // Anti-sniping: extend deadline if bid arrives within the extension window.
    let extension_window: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ExtensionWindow)
        .unwrap_or(0u32);
    if extension_window > 0 {
        let current_ledger = env.ledger().sequence();
        if deadline.saturating_sub(current_ledger) <= extension_window {
            let max_deadline: u32 = env
                .storage()
                .instance()
                .get(&DataKey::MaxDeadline)
                .unwrap_or(u32::MAX);
            let extended = deadline.saturating_add(extension_window);
            let new_deadline = extended.min(max_deadline);
            if new_deadline > deadline {
                deadline = new_deadline;
                env.storage().instance().set(&DataKey::Deadline, &deadline);
                events::deadline_extended(env, deadline);
            }
            if extended > max_deadline {
                events::max_deadline_reached(env, max_deadline);
            }
        }
    }

    bump_instance(env);

    if required_transfer > 0 {
        let token: Address = get_instance(env, &DataKey::Token)?;
        token::Client::new(env, &token).transfer(
            bidder,
            &env.current_contract_address(),
            &required_transfer,
        );
    }

    events::bid_placed(env, bidder, amount);
    Ok(())
}

/// English and Dutch auction contract.
///
/// English lifecycle:
/// - Seller calls `start` to set the token, starting price, minimum bid increment,
///   deadline, an optional `reserve_price`, an optional anti-sniping extension window,
///   and an optional custodial NFT.
/// - Bidders call `bid` (or `bid_with_credit`) with increasing amounts. The previous
///   highest bidder's funds are held as a pending refund, collectable via `withdraw`
///   or reusable via `bid_with_credit`.
/// - If no bid has been placed, the seller may call `cancel` to abort the auction.
///   After bids exist, the seller may still cancel inside the configured
///   cancellation grace window by compensating the top bidder with
///   `cancellation_fee` (see the crate-level docs for the full state machine).
/// - After the deadline, anyone calls `end` to settle. If `highest_bid >= reserve_price`
///   (or no reserve is set) the seller receives the winning bid and the winner receives
///   the escrowed NFT; otherwise funds are returned to the highest bidder and the NFT
///   to the seller. If no bids were placed the NFT is returned to the seller.
/// - Outbid bidders call `withdraw` to recover their pending refund at any time.
///
/// Dutch lifecycle:
/// - Seller calls `start_dutch` with a descending price schedule.
/// - The first buyer to call `buy` with `max_price >= get_current_price()` wins
///   immediately; payment goes straight to the seller and the NFT to the buyer.
/// - The seller may `cancel` while the item is unsold.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct AuctionContract;

    #[contractimpl]
    impl AuctionContract {
        /// Start an English auction.
        ///
        /// `reserve_price` is optional. Pass `None` (or `0`) for no reserve. When set,
        /// `end()` will only transfer to the seller if `highest_bid >= reserve_price`;
        /// otherwise the highest bidder's funds are returned.
        ///
        /// `extension_window` is the number of ledgers added to the deadline when a
        /// bid arrives within that many ledgers of the current deadline (anti-sniping).
        /// Pass `0` to disable anti-sniping.
        ///
        /// `max_deadline` caps anti-sniping extensions: the deadline is never
        /// extended past this ledger. Must be `>= deadline`; pass `u32::MAX` for
        /// no cap.
        ///
        /// `cancellation_grace_ledgers` is the number of ledgers after `start` during
        /// which the seller may cancel even after bids have been placed. Pass `0` to
        /// disable (cancel is then only possible before the first bid).
        ///
        /// `cancellation_fee` is the compensation, in `token` units, that the seller
        /// pays the top bidder when cancelling inside the grace window. Must be `>= 0`.
        /// `nft_contract` / `token_id` optionally make the auction custodial: the NFT is
        /// transferred from `seller` into this contract (the seller's authorization of
        /// `start` covers the nested NFT `transfer`). Pass `None` for both to auction an
        /// off-chain or non-custodial item.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::AlreadyInitialized`] if already started.
        /// - [`AuctionError::InvalidAmount`] if `start_price` or `min_increment` <= 0,
        ///   or `cancellation_fee` < 0.
        /// - [`AuctionError::InvalidDeadline`] if `deadline` <= current ledger or
        ///   `max_deadline` < `deadline`.
        /// - [`AuctionError::InvalidNftParams`] if only one of `nft_contract` / `token_id`
        ///   is supplied.
        #[allow(clippy::too_many_arguments)]
        pub fn start(
            env: Env,
            seller: Address,
            token: Address,
            start_price: i128,
            min_increment: i128,
            deadline: u32,
            reserve_price: Option<i128>,
            extension_window: u32,
            max_deadline: u32,
            cancellation_grace_ledgers: u32,
            cancellation_fee: i128,
            nft_contract: Option<Address>,
            token_id: Option<u32>,
        ) -> Result<(), AuctionError> {
            if env.storage().instance().has(&DataKey::Seller) {
                return Err(AuctionError::AlreadyInitialized);
            }
            if start_price <= 0 || min_increment <= 0 || cancellation_fee < 0 {
                return Err(AuctionError::InvalidAmount);
            }
            if deadline <= env.ledger().sequence() || max_deadline < deadline {
                return Err(AuctionError::InvalidDeadline);
            }
            validate_nft_params(&nft_contract, &token_id)?;
            // highest_bid starts at start_price - 1 so the first bid must be >= start_price
            let initial_highest = start_price.checked_sub(1).ok_or(AuctionError::Overflow)?;

            seller.require_auth();

            env.storage().instance().set(&DataKey::Seller, &seller);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage()
                .instance()
                .set(&DataKey::StartPrice, &start_price);
            env.storage()
                .instance()
                .set(&DataKey::MinIncrement, &min_increment);
            env.storage().instance().set(&DataKey::Deadline, &deadline);
            env.storage()
                .instance()
                .set(&DataKey::ExtensionWindow, &extension_window);
            env.storage()
                .instance()
                .set(&DataKey::MaxDeadline, &max_deadline);
            env.storage()
                .instance()
                .set(&DataKey::HighestBid, &initial_highest);
            env.storage().instance().set(&DataKey::Settled, &false);
            env.storage().instance().set(&DataKey::Cancelled, &false);
            env.storage()
                .instance()
                .set(&DataKey::StartLedger, &env.ledger().sequence());
            env.storage()
                .instance()
                .set(&DataKey::CancellationGraceLedgers, &cancellation_grace_ledgers);
            env.storage()
                .instance()
                .set(&DataKey::CancellationFee, &cancellation_fee);

            if let Some(rp) = reserve_price {
                env.storage().instance().set(&DataKey::ReservePrice, &rp);
            }

            bump_instance(&env);
            escrow_nft(&env, &seller, nft_contract, token_id);
            events::started(&env, &seller, start_price, deadline);
            Ok(())
        }

        /// Start a Dutch (descending price) auction (issue #1071).
        ///
        /// The price falls linearly from `start_price` at `start_ledger` to
        /// `floor_price` at `start_ledger + duration_ledgers` and stays at
        /// `floor_price` afterwards. See [`AuctionContract::get_current_price`].
        ///
        /// `nft_contract` / `token_id` behave as in [`AuctionContract::start`].
        ///
        /// # Errors
        ///
        /// - [`AuctionError::AlreadyInitialized`] if already started.
        /// - [`AuctionError::InvalidAmount`] unless `0 <= floor_price < start_price`.
        /// - [`AuctionError::InvalidDeadline`] if `duration_ledgers == 0` or
        ///   `start_ledger` is before the current ledger.
        /// - [`AuctionError::Overflow`] if `start_ledger + duration_ledgers` overflows.
        /// - [`AuctionError::InvalidNftParams`] if only one of `nft_contract` / `token_id`
        ///   is supplied.
        #[allow(clippy::too_many_arguments)]
        pub fn start_dutch(
            env: Env,
            seller: Address,
            token: Address,
            start_price: i128,
            floor_price: i128,
            start_ledger: u32,
            duration_ledgers: u32,
            nft_contract: Option<Address>,
            token_id: Option<u32>,
        ) -> Result<(), AuctionError> {
            if env.storage().instance().has(&DataKey::Seller) {
                return Err(AuctionError::AlreadyInitialized);
            }
            if floor_price < 0 || start_price <= floor_price {
                return Err(AuctionError::InvalidAmount);
            }
            if duration_ledgers == 0 || start_ledger < env.ledger().sequence() {
                return Err(AuctionError::InvalidDeadline);
            }
            let end_ledger = start_ledger
                .checked_add(duration_ledgers)
                .ok_or(AuctionError::Overflow)?;
            validate_nft_params(&nft_contract, &token_id)?;

            seller.require_auth();

            let cfg = DutchConfig {
                start_price,
                floor_price,
                start_ledger,
                duration_ledgers,
            };

            env.storage().instance().set(&DataKey::Seller, &seller);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage()
                .instance()
                .set(&DataKey::StartPrice, &start_price);
            // English-only fields are stored with neutral values so `get_info` works.
            env.storage()
                .instance()
                .set(&DataKey::MinIncrement, &0i128);
            env.storage()
                .instance()
                .set(&DataKey::Deadline, &end_ledger);
            env.storage().instance().set(&DataKey::HighestBid, &0i128);
            env.storage().instance().set(&DataKey::Settled, &false);
            env.storage().instance().set(&DataKey::Cancelled, &false);
            env.storage().instance().set(&DataKey::DutchConfig, &cfg);

            bump_instance(&env);
            escrow_nft(&env, &seller, nft_contract, token_id);
            events::dutch_started(
                &env,
                &seller,
                start_price,
                floor_price,
                start_ledger,
                duration_ledgers,
            );
            Ok(())
        }

            // Queue previous highest bidder's refund
            let prev_bidder: Option<Address> =
                env.storage().instance().get(&DataKey::HighestBidder);
            if let Some(prev) = prev_bidder {
                let pending: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Pending(prev.clone()))
                    .unwrap_or(0);
                let new_pending = pending + highest_bid;
                env.storage()
                    .persistent()
                    .set(&DataKey::Pending(prev.clone()), &new_pending);
                env.storage().persistent().extend_ttl(
                    &DataKey::Pending(prev.clone()),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
                events::outbid(&env, &prev, highest_bid, amount);
                events::refund_queued(&env, &prev, highest_bid);
        /// Return the current Dutch auction price (issue #1071).
        ///
        /// `start_price - (start_price - floor_price) * (now - start_ledger) / duration_ledgers`,
        /// equal to `start_price` before `start_ledger` and clamped to `floor_price`
        /// from `start_ledger + duration_ledgers` onwards.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::WrongMode`] if this is an English auction.
        pub fn get_current_price(env: Env) -> Result<i128, AuctionError> {
            get_instance::<Address>(&env, &DataKey::Seller)?; // ensure initialized
            let cfg = dutch_config(&env).ok_or(AuctionError::WrongMode)?;
            dutch::price_at(&cfg, env.ledger().sequence())
        }

        /// Buy the item in a Dutch auction at the current price (issue #1071).
        ///
        /// Settles atomically: the auction is marked settled before any external
        /// call, the current price is paid directly from `buyer` to the seller, and
        /// the escrowed NFT (if any) is delivered to `buyer`. `max_price` protects
        /// the buyer against paying more than intended.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::WrongMode`] if this is an English auction.
        /// - [`AuctionError::AuctionEnded`] if already sold or cancelled.
        /// - [`AuctionError::AuctionNotStarted`] if before `start_ledger`.
        /// - [`AuctionError::BidTooLow`] if `max_price` < current price.
        pub fn buy(env: Env, buyer: Address, max_price: i128) -> Result<i128, AuctionError> {
            let seller: Address = get_instance(&env, &DataKey::Seller)?;
            let cfg = dutch_config(&env).ok_or(AuctionError::WrongMode)?;
            if get_flag(&env, &DataKey::Settled) || get_flag(&env, &DataKey::Cancelled) {
                return Err(AuctionError::AuctionEnded);
            }
            let now = env.ledger().sequence();
            if now < cfg.start_ledger {
                return Err(AuctionError::AuctionNotStarted);
            }
            let price = dutch::price_at(&cfg, now)?;
            if max_price < price {
                return Err(AuctionError::BidTooLow);
            }

            buyer.require_auth();

            // Effects before interactions: a re-entrant `buy` would see `Settled`.
            env.storage().instance().set(&DataKey::Settled, &true);
            env.storage()
                .instance()
                .set(&DataKey::HighestBidder, &buyer);
            env.storage().instance().set(&DataKey::HighestBid, &price);
            bump_instance(&env);

            if price > 0 {
                let token: Address = get_instance(&env, &DataKey::Token)?;
                token::Client::new(&env, &token).transfer(&buyer, &seller, &price);
            }
            release_nft(&env, &buyer);

            events::dutch_bought(&env, &buyer, price);
            Ok(price)
        }

        /// Place a bid. The bid must be at least `highest_bid + min_increment`.
        /// The previous highest bidder's funds are queued as a pending refund.
        ///
        /// If the bid arrives within `extension_window` ledgers of the deadline,
        /// the deadline is extended by `extension_window` ledgers (anti-sniping).
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::WrongMode`] if this is a Dutch auction.
        /// - [`AuctionError::AuctionEnded`] if the deadline has passed or auction is cancelled.
        /// - [`AuctionError::BidTooLow`] if `amount` < current highest bid + min_increment.
        /// - [`AuctionError::Overflow`] if the minimum required bid or the queued refund
        ///   overflows `i128`.
        pub fn bid(env: Env, bidder: Address, amount: i128) -> Result<(), AuctionError> {
            place_bid(&env, &bidder, amount, false)
        }

        /// Place a bid of `total_bid`, funding it first from the bidder's pending
        /// refund balance (issue #1068).
        ///
        /// Only `total_bid - pending_credit` is transferred from `bidder` (nothing if
        /// the credit covers the whole bid). Any unused credit stays in
        /// `Pending(bidder)` and remains withdrawable. Validation, anti-sniping, and
        /// refund queueing are identical to [`AuctionContract::bid`].
        ///
        /// # Errors
        ///
        /// Same as [`AuctionContract::bid`].
        pub fn bid_with_credit(
            env: Env,
            bidder: Address,
            total_bid: i128,
        ) -> Result<(), AuctionError> {
            place_bid(&env, &bidder, total_bid, true)
        }

        /// Cancel the auction. Only callable by the seller.
        ///
        /// - Before any bid: always allowed; nothing is transferred (emits `cancelled`).
        /// - After a bid: only allowed while the grace window is enabled and
        ///   `current_ledger <= start_ledger + cancellation_grace_ledgers`. The seller
        ///   transfers `cancellation_fee` into the contract, and the top bidder's pending
        ///   refund is credited with `highest_bid + cancellation_fee`, claimable via
        ///   `withdraw`. Emits `cancelled_with_compensation` carrying
        ///   [`AuctionCancelledWithCompensation`].
        /// Cancel the auction. Only callable by the seller and only before any bid
        /// has been placed (English) or before the item is bought (Dutch). An
        /// escrowed NFT is returned to the seller.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::NotAuthorized`] if the caller is not the seller.
        /// - [`AuctionError::AlreadyEnded`] if the auction is already settled or cancelled.
        /// - [`AuctionError::BidAlreadyPlaced`] if a bid exists and the grace window is
        ///   disabled or has elapsed.
        /// - [`AuctionError::InvalidAmount`] if crediting the refund would overflow.
        pub fn cancel(env: Env, seller: Address) -> Result<(), AuctionError> {
            let stored_seller: Address = get_instance(&env, &DataKey::Seller)?;
            if seller != stored_seller {
                return Err(AuctionError::NotAuthorized);
            }

            let settled: bool = get_instance(&env, &DataKey::Settled)?;
            if settled || get_flag(&env, &DataKey::Cancelled) {
                return Err(AuctionError::AlreadyEnded);
            }

            let highest_bidder: Option<Address> =
                env.storage().instance().get(&DataKey::HighestBidder);

            let Some(top_bidder) = highest_bidder else {
                // No bids yet: plain cancellation, nothing to refund.
                seller.require_auth();
                env.storage().instance().set(&DataKey::Cancelled, &true);
                bump_instance(&env);
                events::cancelled(&env, &seller);
                return Ok(());
            };

            // A bid exists: only allowed inside the cancellation grace window.
            let grace: u32 = env
                .storage()
                .instance()
                .get(&DataKey::CancellationGraceLedgers)
                .unwrap_or(0);
            let start_ledger: u32 = env
                .storage()
                .instance()
                .get(&DataKey::StartLedger)
                .unwrap_or(0);
            if grace == 0 || env.ledger().sequence() > start_ledger.saturating_add(grace) {
            // Reject if a bid has already been placed (highest_bidder is set).
            let has_bidder: bool = env.storage().instance().has(&DataKey::HighestBidder);
            let start_price: i128 = get_instance(&env, &DataKey::StartPrice)?;
            let highest_bid: i128 = get_instance(&env, &DataKey::HighestBid)?;
            // highest_bid is initialised below start_price; if it equals start_price or
            // higher, at least one real bid was placed.
            if has_bidder || highest_bid >= start_price {
                return Err(AuctionError::BidAlreadyPlaced);
            }

            seller.require_auth();

            let highest_bid: i128 = get_instance(&env, &DataKey::HighestBid)?;
            let fee: i128 = env
                .storage()
                .instance()
                .get(&DataKey::CancellationFee)
                .unwrap_or(0);

            let pending: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pending(top_bidder.clone()))
                .unwrap_or(0);
            let new_pending = highest_bid
                .checked_add(fee)
                .and_then(|credit| pending.checked_add(credit))
                .ok_or(AuctionError::InvalidAmount)?;

            // The seller funds the compensation before it becomes withdrawable.
            if fee > 0 {
                let token: Address = get_instance(&env, &DataKey::Token)?;
                token::Client::new(&env, &token).transfer(
                    &seller,
                    &env.current_contract_address(),
                    &fee,
                );
            }

            env.storage()
                .persistent()
                .set(&DataKey::Pending(top_bidder.clone()), &new_pending);
            env.storage().persistent().extend_ttl(
                &DataKey::Pending(top_bidder.clone()),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            // The escrowed bid is now a pending refund, so the top bidder no longer
            // holds the lead; clearing it rules out any later settlement to the seller.
            env.storage().instance().remove(&DataKey::HighestBidder);
            env.storage().instance().set(&DataKey::Cancelled, &true);

            bump_instance(&env);
            events::cancelled_with_compensation(&env, &seller, &top_bidder, fee);
            release_nft(&env, &seller);
            events::cancelled(&env, &seller);
            Ok(())
        }

        /// Settle an English auction after the deadline.
        ///
        /// - If no bids were placed, emits `ended_no_bids`, and any escrowed NFT is
        ///   returned to the seller.
        /// - If a reserve price was set and `highest_bid < reserve_price`, the highest
        ///   bidder's funds are returned, the NFT goes back to the seller, and the item is
        ///   left unsold (emits `ended_reserve_not_met`).
        /// - Otherwise the seller receives the winning bid and the winner receives the
        ///   NFT in the same transaction (emits `ended`).
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::WrongMode`] if this is a Dutch auction.
        /// - [`AuctionError::AuctionNotEnded`] if the deadline has not passed.
        /// - [`AuctionError::AlreadyEnded`] if already settled or cancelled.
        pub fn end(env: Env) -> Result<(), AuctionError> {
            let seller: Address = get_instance(&env, &DataKey::Seller)?;
            if dutch_config(&env).is_some() {
                return Err(AuctionError::WrongMode);
            }

            let cancelled: bool = env
                .storage()
                .instance()
                .get(&DataKey::Cancelled)
                .unwrap_or(false);
            if cancelled {
                return Err(AuctionError::AlreadyEnded);
            }

            let deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() <= deadline {
                return Err(AuctionError::AuctionNotEnded);
            }

            let settled: bool = get_instance(&env, &DataKey::Settled)?;
            if settled {
                return Err(AuctionError::AlreadyEnded);
            }

            env.storage().instance().set(&DataKey::Settled, &true);

            let start_price: i128 = get_instance(&env, &DataKey::StartPrice)?;
            let highest_bid: i128 = get_instance(&env, &DataKey::HighestBid)?;
            let winner: Option<Address> = env.storage().instance().get(&DataKey::HighestBidder);

            bump_instance(&env);

            // No bids at all
            let Some(winner) = winner.filter(|_| highest_bid >= start_price) else {
                release_nft(&env, &seller);
                events::ended_no_bids(&env);
                return Ok(());
            };

            let token: Address = get_instance(&env, &DataKey::Token)?;

            // Reserve price check
            let reserve_price: Option<i128> = env.storage().instance().get(&DataKey::ReservePrice);
            if let Some(rp) = reserve_price {
                if highest_bid < rp {
                    // Return funds to the highest bidder and the item to the seller
                    token::Client::new(&env, &token).transfer(
                        &env.current_contract_address(),
                        &winner,
                        &highest_bid,
                    );
                    release_nft(&env, &seller);
                    events::ended_reserve_not_met(&env, &winner, highest_bid, rp);
                    return Ok(());
                }
            }

            // Payment and NFT delivery happen in the same invocation, so either both
            // succeed or the whole settlement reverts.
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &seller,
                &highest_bid,
            );
            release_nft(&env, &winner);

            events::ended(&env, &winner, highest_bid);
            Ok(())
        }

        /// Withdraw a pending refund (available for outbid bidders).
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NothingToWithdraw`] if caller has no pending refund.
        pub fn withdraw(env: Env, bidder: Address) -> Result<(), AuctionError> {
            bidder.require_auth();

            let pending = pending_of(&env, &bidder);
            if pending <= 0 {
                return Err(AuctionError::NothingToWithdraw);
            }

            env.storage()
                .persistent()
                .remove(&DataKey::Pending(bidder.clone()));

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &bidder,
                &pending,
            );

            events::withdrawn(&env, &bidder, pending);
            Ok(())
        }

        /// Return a bidder's pending refund amount.
        #[must_use]
        pub fn get_pending(env: Env, bidder: Address) -> i128 {
            pending_of(&env, &bidder)
        }

        /// Return the Dutch auction schedule, or `None` for an English auction.
        #[must_use]
        pub fn get_dutch_config(env: Env) -> Option<DutchConfig> {
            dutch_config(&env)
        }

        /// Return auction details.
        #[must_use]
        pub fn get_info(env: Env) -> Result<AuctionInfo, AuctionError> {
            Ok(AuctionInfo {
                seller: get_instance(&env, &DataKey::Seller)?,
                token: get_instance(&env, &DataKey::Token)?,
                start_price: get_instance(&env, &DataKey::StartPrice)?,
                min_increment: get_instance(&env, &DataKey::MinIncrement)?,
                deadline: get_instance(&env, &DataKey::Deadline)?,
                highest_bid: get_instance(&env, &DataKey::HighestBid)?,
                highest_bidder: env.storage().instance().get(&DataKey::HighestBidder),
                settled: get_instance(&env, &DataKey::Settled)?,
                reserve_price: env.storage().instance().get(&DataKey::ReservePrice),
                extension_window: env
                    .storage()
                    .instance()
                    .get(&DataKey::ExtensionWindow)
                    .unwrap_or(0),
                max_deadline: env
                    .storage()
                    .instance()
                    .get(&DataKey::MaxDeadline)
                    .unwrap_or(u32::MAX),
                start_ledger: env
                    .storage()
                    .instance()
                    .get(&DataKey::StartLedger)
                    .unwrap_or(0),
                cancellation_grace_ledgers: env
                    .storage()
                    .instance()
                    .get(&DataKey::CancellationGraceLedgers)
                    .unwrap_or(0),
                cancellation_fee: env
                    .storage()
                    .instance()
                    .get(&DataKey::CancellationFee)
                    .unwrap_or(0),
                nft_contract: env.storage().instance().get(&DataKey::NftContract),
                nft_token_id: env.storage().instance().get(&DataKey::NftTokenId),
            })
        }

        /// Return `true` when the auction has been cancelled, `false` otherwise.
        ///
        /// This is a read-only query — no signer or admin authentication required.
        /// Use this instead of invoking `end` or `cancel` from monitoring scripts,
        /// which would submit a real transaction and potentially settle the auction.
        #[must_use]
        pub fn is_cancelled(env: Env) -> bool {
            get_flag(&env, &DataKey::Cancelled)
        }
    }
}

mod test;

#[cfg(test)]
mod prop_test;
