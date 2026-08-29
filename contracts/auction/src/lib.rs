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
//! # Cancel before first bid (issue #785)
//! The seller may call `cancel` at any time before a bid has been placed. Once
//! a bid exists the auction is irrevocable.
//!
//! # Reserve price (issue #783)
//! An optional `reserve_price` can be set at `start`. When set, `end()` will
//! only transfer to the seller if `highest_bid >= reserve_price`; otherwise
//! the highest bidder's funds are returned.

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::AuctionError;
pub use storage::{AuctionInfo, DataKey};

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

/// English auction contract.
///
/// Lifecycle:
/// - Seller calls `start` to set the token, starting price, minimum bid increment,
///   deadline, an optional `reserve_price`, and optional anti-sniping extension window.
/// - Bidders call `bid` with increasing amounts. The previous highest bidder's funds
///   are held as a pending refund, collectable via `withdraw`.
/// - If no bid has been placed, the seller may call `cancel` to abort the auction.
/// - After the deadline, anyone calls `end` to settle. If `highest_bid >= reserve_price`
///   (or no reserve is set) the seller receives the winning bid; otherwise funds are
///   returned to the highest bidder. If no bids were placed the auction ends with no
///   transfer.
/// - Outbid bidders call `withdraw` to recover their pending refund at any time.
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
        /// Start the auction.
        ///
        /// `reserve_price` is optional. Pass `None` (or `0`) for no reserve. When set,
        /// `end()` will only transfer to the seller if `highest_bid >= reserve_price`;
        /// otherwise the highest bidder's funds are returned.
        ///
        /// `extension_window` is the number of ledgers added to the deadline when a
        /// bid arrives within that many ledgers of the current deadline (anti-sniping).
        /// Pass `0` to disable anti-sniping.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::AlreadyInitialized`] if already started.
        /// - [`AuctionError::InvalidAmount`] if `start_price` or `min_increment` <= 0.
        /// - [`AuctionError::InvalidDeadline`] if `deadline` <= current ledger.
        pub fn start(
            env: Env,
            seller: Address,
            token: Address,
            start_price: i128,
            min_increment: i128,
            deadline: u32,
            reserve_price: Option<i128>,
            extension_window: u32,
        ) -> Result<(), AuctionError> {
            if env.storage().instance().has(&DataKey::Seller) {
                return Err(AuctionError::AlreadyInitialized);
            }
            if start_price <= 0 || min_increment <= 0 {
                return Err(AuctionError::InvalidAmount);
            }
            if deadline <= env.ledger().sequence() {
                return Err(AuctionError::InvalidDeadline);
            }

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
            // highest_bid starts at start_price - 1 so the first bid must be >= start_price
            env.storage()
                .instance()
                .set(&DataKey::HighestBid, &(start_price - 1));
            env.storage().instance().set(&DataKey::Settled, &false);
            env.storage().instance().set(&DataKey::Cancelled, &false);

            if let Some(rp) = reserve_price {
                env.storage().instance().set(&DataKey::ReservePrice, &rp);
            }

            bump_instance(&env);
            events::started(&env, &seller, start_price, deadline);
            Ok(())
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
        /// - [`AuctionError::AuctionEnded`] if the deadline has passed or auction is cancelled.
        /// - [`AuctionError::BidTooLow`] if `amount` < current highest bid + min_increment.
        pub fn bid(env: Env, bidder: Address, amount: i128) -> Result<(), AuctionError> {
            if amount <= 0 {
                return Err(AuctionError::InvalidAmount);
            }

            let cancelled: bool = env
                .storage()
                .instance()
                .get(&DataKey::Cancelled)
                .unwrap_or(false);
            if cancelled {
                return Err(AuctionError::AuctionEnded);
            }

            let mut deadline: u32 = get_instance(&env, &DataKey::Deadline)?;
            if env.ledger().sequence() > deadline {
                return Err(AuctionError::AuctionEnded);
            }

            let highest_bid: i128 = get_instance(&env, &DataKey::HighestBid)?;
            let min_increment: i128 = get_instance(&env, &DataKey::MinIncrement)?;
            let start_price: i128 = get_instance(&env, &DataKey::StartPrice)?;

            // First bid must be >= start_price; subsequent bids must be >= highest_bid + min_increment
            let min_required = if highest_bid < start_price {
                start_price
            } else {
                highest_bid + min_increment
            };

            if amount < min_required {
                return Err(AuctionError::BidTooLow);
            }

            bidder.require_auth();

            let token: Address = get_instance(&env, &DataKey::Token)?;
            token::Client::new(&env, &token).transfer(
                &bidder,
                &env.current_contract_address(),
                &amount,
            );

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
                    &DataKey::Pending(prev),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
            }

            env.storage()
                .instance()
                .set(&DataKey::HighestBidder, &bidder);
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
                    deadline = deadline.saturating_add(extension_window);
                    env.storage().instance().set(&DataKey::Deadline, &deadline);
                    events::deadline_extended(&env, deadline);
                }
            }

            bump_instance(&env);
            events::bid_placed(&env, &bidder, amount);
            Ok(())
        }

        /// Cancel the auction. Only callable by the seller and only before any bid
        /// has been placed.
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::NotAuthorized`] if the caller is not the seller.
        /// - [`AuctionError::BidAlreadyPlaced`] if at least one bid has already been placed.
        /// - [`AuctionError::AlreadyEnded`] if the auction is already settled or cancelled.
        pub fn cancel(env: Env, seller: Address) -> Result<(), AuctionError> {
            let stored_seller: Address = get_instance(&env, &DataKey::Seller)?;
            if seller != stored_seller {
                return Err(AuctionError::NotAuthorized);
            }

            let settled: bool = get_instance(&env, &DataKey::Settled)?;
            let cancelled: bool = env
                .storage()
                .instance()
                .get(&DataKey::Cancelled)
                .unwrap_or(false);
            if settled || cancelled {
                return Err(AuctionError::AlreadyEnded);
            }

            // Reject if a bid has already been placed (highest_bidder is set).
            let has_bidder: bool = env.storage().instance().has(&DataKey::HighestBidder);
            let start_price: i128 = get_instance(&env, &DataKey::StartPrice)?;
            let highest_bid: i128 = get_instance(&env, &DataKey::HighestBid)?;
            // highest_bid is initialised to start_price - 1; if it equals start_price or
            // higher, at least one real bid was placed.
            if has_bidder || highest_bid >= start_price {
                return Err(AuctionError::BidAlreadyPlaced);
            }

            seller.require_auth();

            env.storage().instance().set(&DataKey::Cancelled, &true);

            bump_instance(&env);
            events::cancelled(&env, &seller);
            Ok(())
        }

        /// Settle the auction after the deadline.
        ///
        /// - If no bids were placed, emits `ended_no_bids` and nothing is transferred.
        /// - If a reserve price was set and `highest_bid < reserve_price`, the highest bidder's
        ///   funds are queued as a pending refund (retrievable via `withdraw`) and the item is
        ///   left unsold (emits `ended_reserve_not_met`).
        /// - Otherwise the seller receives the winning bid (emits `ended`).
        ///
        /// # Errors
        ///
        /// - [`AuctionError::NotInitialized`] if not started.
        /// - [`AuctionError::AuctionNotEnded`] if the deadline has not passed.
        /// - [`AuctionError::AlreadyEnded`] if already settled.
        pub fn end(env: Env) -> Result<(), AuctionError> {
            get_instance::<Address>(&env, &DataKey::Seller)?; // ensure initialized

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
            if highest_bid < start_price || winner.is_none() {
                events::ended_no_bids(&env);
                return Ok(());
            }

            #[allow(clippy::unwrap_used)] // winner.is_none() is checked two lines above
            let winner = winner.unwrap();

            // Reserve price check
            let reserve_price: Option<i128> = env.storage().instance().get(&DataKey::ReservePrice);
            if let Some(rp) = reserve_price {
                if highest_bid < rp {
                    // Return funds to the highest bidder
                    let token: Address = get_instance(&env, &DataKey::Token)?;
                    token::Client::new(&env, &token).transfer(
                        &env.current_contract_address(),
                        &winner,
                        &highest_bid,
                    );
                    events::ended_reserve_not_met(&env, &winner, highest_bid, rp);
                    return Ok(());
                }
            }

            let seller: Address = get_instance(&env, &DataKey::Seller)?;
            let token: Address = get_instance(&env, &DataKey::Token)?;

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &seller,
                &highest_bid,
            );

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

            let pending: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Pending(bidder.clone()))
                .unwrap_or(0);
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
            env.storage()
                .persistent()
                .get(&DataKey::Pending(bidder))
                .unwrap_or(0)
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
            })
        }

        /// Return `true` when the auction has been cancelled, `false` otherwise.
        ///
        /// This is a read-only query — no signer or admin authentication required.
        /// Use this instead of invoking `end` or `cancel` from monitoring scripts,
        /// which would submit a real transaction and potentially settle the auction.
        #[must_use]
        pub fn is_cancelled(env: Env) -> bool {
            env.storage()
                .instance()
                .get(&DataKey::Cancelled)
                .unwrap_or(false)
        }
    }

mod test;

#[cfg(test)]
mod prop_test;
