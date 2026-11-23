#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::as_conversions
)]
#![cfg(test)]

use std::format;
use std::vec;
use std::{format, vec, vec::Vec};

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

use crate::{AuctionContract, AuctionContractClient, AuctionError};
use crate::dutch::price_at;
use crate::{AuctionContract, AuctionContractClient, AuctionError, DataKey, DutchConfig};

fn setup_auction<'a>(
    env: &'a Env,
    start_price: i128,
    min_increment: i128,
) -> (AuctionContractClient<'a>, Address, Address, Address) {
    let seller = Address::generate(env);
    let sac_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(sac_admin);
    let token_addr = sac.address();

    let auction_addr = env.register_contract(None, AuctionContract);
    let client = AuctionContractClient::new(env, &auction_addr);

    let deadline = env.ledger().sequence() + 1000;
    client.start(
        &seller,
        &token_addr,
        &start_price,
        &min_increment,
        &deadline,
        &None,
        &0,
        &u32::MAX,
        &0,
        &0,
        &None,
        &None,
    );

    (client, seller, token_addr, auction_addr)
}

// ---------------------------------------------------------------------------
// Issue #1073 — stateful model-based balance conservation harness
//
// A random auction configuration and a random interleaving of `start`, `bid`,
// `withdraw`, `extend` (ledger advance), `cancel` and `end` calls are applied
// both to the real contract and to a plain-Rust reference model. After every
// step the harness asserts:
//
// * Balance conservation:
//   `contract_token_balance >= highest_bid + sum(pending_refunds)`, where
//   `highest_bid` counts only while it is still escrowed (auction live).
//   The model additionally pins this to exact equality.
// * Model agreement: every call succeeds or fails exactly as the model
//   predicts, and highest bid, deadline, flags and pending refunds match.
// * Refund accessibility: every `withdraw` pays out exactly the refund the
//   model says the bidder is owed.
// * Zero trapped funds: once the auction is settled or cancelled and every
//   participant has withdrawn, the contract holds no tokens.
// ---------------------------------------------------------------------------

const BIDDERS: usize = 4;
const BIDDER_FUNDS: i128 = 1_000_000_000;

#[derive(Clone, Debug)]
struct Params {
    start_price: i128,
    min_increment: i128,
    duration: u32,
    reserve_price: Option<i128>,
    extension_window: u32,
    /// Ledgers past the initial deadline that extensions may reach.
    max_extension: u32,
    grace: u32,
    fee: i128,
}

prop_compose! {
    fn params_strategy()(
        start_price in 1i128..=1_000,
        min_increment in 1i128..=100,
        duration in 20u32..=80,
        reserve_price in proptest::option::of(1i128..=3_000),
        extension_window in 0u32..=10,
        max_extension in 0u32..=30,
        grace in 0u32..=30,
        fee in 0i128..=500,
    ) -> Params {
        Params {
            start_price,
            min_increment,
            duration,
            reserve_price,
            extension_window,
            max_extension,
            grace,
            fee,
        }
    }
}

#[derive(Clone, Debug)]
enum Action {
    /// Call `start` again on the live auction (must be rejected).
    Start,
    /// Bid `min_required + raise` (valid unless the auction is closed).
    Bid { bidder: usize, raise: i128 },
    /// Bid one unit below the minimum (must be rejected).
    LowBid { bidder: usize },
    /// Claim any pending refund.
    Withdraw { bidder: usize },
    /// Advance the ledger, driving deadline expiry, anti-sniping extensions
    /// and the end of the cancellation grace window.
    Extend { ledgers: u32 },
    /// Seller cancellation (plain or grace-window with compensation).
    Cancel,
    /// Settle the auction.
    End,
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        1 => Just(Action::Start),
        6 => (0..BIDDERS, 0i128..=500).prop_map(|(bidder, raise)| Action::Bid { bidder, raise }),
        1 => (0..BIDDERS).prop_map(|bidder| Action::LowBid { bidder }),
        3 => (0..BIDDERS).prop_map(|bidder| Action::Withdraw { bidder }),
        3 => (1u32..=15).prop_map(|ledgers| Action::Extend { ledgers }),
        1 => Just(Action::Cancel),
        1 => Just(Action::End),
    ]
}

/// Reference model of the auction's accounting.
struct Model {
    params: Params,
    ledger: u32,
    start_ledger: u32,
    deadline: u32,
    max_deadline: u32,
    highest_bid: i128,
    highest_bidder: Option<usize>,
    pending: [i128; BIDDERS],
    settled: bool,
    cancelled: bool,
}

impl Model {
    fn new(params: Params, start_ledger: u32, deadline: u32) -> Self {
        let highest_bid = params.start_price - 1;
        let max_deadline = deadline + params.max_extension;
        Self {
            params,
            max_deadline,
            ledger: start_ledger,
            start_ledger,
            deadline,
            highest_bid,
            highest_bidder: None,
            pending: [0; BIDDERS],
            settled: false,
            cancelled: false,
        }
    }

    fn min_required(&self) -> i128 {
        if self.highest_bid < self.params.start_price {
            self.params.start_price
        } else {
            self.highest_bid + self.params.min_increment
        }
    }

    /// The top bid, while the contract still holds it in escrow.
    fn escrowed_bid(&self) -> i128 {
        if self.highest_bidder.is_some() && !self.settled && !self.cancelled {
            self.highest_bid
        } else {
            0
        }
    }

    /// Everything the contract owes: the escrowed top bid plus all refunds.
    fn liabilities(&self) -> i128 {
        self.escrowed_bid() + self.pending.iter().sum::<i128>()
    }

    fn bid(&mut self, bidder: usize, amount: i128) -> Result<(), AuctionError> {
        if amount <= 0 {
            return Err(AuctionError::InvalidAmount);
        }
        if self.cancelled || self.ledger > self.deadline {
            return Err(AuctionError::AuctionEnded);
        }
        if amount < self.min_required() {
            return Err(AuctionError::BidTooLow);
        }
        if let Some(prev) = self.highest_bidder {
            self.pending[prev] += self.highest_bid;
        }
        self.highest_bidder = Some(bidder);
        self.highest_bid = amount;

        let window = self.params.extension_window;
        if window > 0 && self.deadline.saturating_sub(self.ledger) <= window {
            self.deadline = self.deadline.saturating_add(window).min(self.max_deadline);
        }
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AuctionError> {
        if self.settled || self.cancelled {
            return Err(AuctionError::AlreadyEnded);
        }
        if let Some(top) = self.highest_bidder {
            let grace = self.params.grace;
            if grace == 0 || self.ledger > self.start_ledger.saturating_add(grace) {
                return Err(AuctionError::BidAlreadyPlaced);
            }
            self.pending[top] += self.highest_bid + self.params.fee;
            self.highest_bidder = None;
        }
        self.cancelled = true;
        Ok(())
    }

    fn end(&mut self) -> Result<(), AuctionError> {
        if self.cancelled {
            return Err(AuctionError::AlreadyEnded);
        }
        if self.ledger <= self.deadline {
            return Err(AuctionError::AuctionNotEnded);
        }
        if self.settled {
            return Err(AuctionError::AlreadyEnded);
        }
        self.settled = true;
        Ok(())
    }

    fn withdraw(&mut self, bidder: usize) -> Result<i128, AuctionError> {
        let owed = self.pending[bidder];
        if owed <= 0 {
            return Err(AuctionError::NothingToWithdraw);
        }
        self.pending[bidder] = 0;
        Ok(owed)
    }
}

fn code(e: AuctionError) -> u32 {
    e as u32
}

/// Collapse a `try_*` client result into `Ok(())` or the contract error code.
fn outcome<T, C: core::fmt::Debug, I: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<AuctionError, I>>,
) -> Result<(), u32> {
    match result {
        Ok(Ok(_)) => Ok(()),
        Err(Ok(e)) => Err(code(e)),
        Ok(Err(c)) => panic!("return value conversion failed: {c:?}"),
        Err(Err(i)) => panic!("host-level invocation failure: {i:?}"),
    }
}

fn check_invariants(
    client: &AuctionContractClient,
    token: &TokenClient,
    seller: &Address,
    bidders: &[Address],
    model: &Model,
    total_minted: i128,
) -> Result<(), TestCaseError> {
    let contract_balance = token.balance(&client.address);
    let info = client.get_info();
    let cancelled = client.is_cancelled();

    // Balance conservation, computed purely from the contract's own state.
    let live = !info.settled && !cancelled && info.highest_bidder.is_some();
    let highest_bid_liability = if live { info.highest_bid } else { 0 };
    let pending_sum: i128 = bidders.iter().map(|b| client.get_pending(b)).sum();
    prop_assert!(
        contract_balance >= highest_bid_liability + pending_sum,
        "balance {} < highest_bid {} + pending {}",
        contract_balance,
        highest_bid_liability,
        pending_sum
    );

    // The contract agrees with the reference model.
    prop_assert_eq!(info.highest_bid, model.highest_bid);
    prop_assert_eq!(info.deadline, model.deadline);
    // Anti-sniping extensions never push the deadline past the cap.
    prop_assert_eq!(info.max_deadline, model.max_deadline);
    prop_assert!(info.deadline <= info.max_deadline);
    prop_assert_eq!(info.settled, model.settled);
    prop_assert_eq!(cancelled, model.cancelled);
    prop_assert_eq!(
        info.highest_bidder,
        model.highest_bidder.map(|i| bidders[i].clone())
    );
    for (i, bidder) in bidders.iter().enumerate() {
        prop_assert_eq!(client.get_pending(bidder), model.pending[i]);
    }

    // Exact conservation: the contract holds precisely what it owes.
    prop_assert_eq!(contract_balance, model.liabilities());

    // No tokens are created or destroyed across participants.
    let circulating = bidders.iter().map(|b| token.balance(b)).sum::<i128>()
        + token.balance(seller)
        + contract_balance;
    prop_assert_eq!(circulating, total_minted);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Stateful property: balance conservation, refund accessibility and zero
    /// trapped funds hold across random interleaved action sequences.
    /// Closes #1073.
    #[test]
    fn prop_stateful_balance_conservation(
        params in params_strategy(),
        actions in proptest::collection::vec(action_strategy(), 1..=40),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let seller = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_addr = sac.address();
        let sac_client = StellarAssetClient::new(&env, &token_addr);
        let token = TokenClient::new(&env, &token_addr);

        let bidders: Vec<Address> = (0..BIDDERS).map(|_| Address::generate(&env)).collect();
        for bidder in &bidders {
            sac_client.mint(bidder, &BIDDER_FUNDS);
        }
        if params.fee > 0 {
            sac_client.mint(&seller, &params.fee);
        }
        let total_minted = BIDDER_FUNDS * BIDDERS as i128 + params.fee;

        let client =
            AuctionContractClient::new(&env, &env.register_contract(None, AuctionContract));
        let start_ledger = env.ledger().sequence();
        let deadline = start_ledger + params.duration;
        client.start(
            &seller,
            &token_addr,
            &params.start_price,
            &params.min_increment,
            &deadline,
            &params.reserve_price,
            &params.extension_window,
            &(deadline + params.max_extension),
            &params.grace,
            &params.fee,
        );

        let mut model = Model::new(params.clone(), start_ledger, deadline);
        check_invariants(&client, &token, &seller, &bidders, &model, total_minted)?;

        for action in actions {
            match action {
                Action::Start => {
                    let got = outcome(client.try_start(
                        &seller,
                        &token_addr,
                        &params.start_price,
                        &params.min_increment,
                        &deadline,
                        &params.reserve_price,
                        &params.extension_window,
                        &(deadline + params.max_extension),
                        &params.grace,
                        &params.fee,
                    ));
                    prop_assert_eq!(got, Err(code(AuctionError::AlreadyInitialized)));
                }
                Action::Bid { bidder, raise } => {
                    let amount = model.min_required() + raise;
                    let expected = model.bid(bidder, amount).map_err(code);
                    let got = outcome(client.try_bid(&bidders[bidder], &amount));
                    prop_assert_eq!(got, expected, "bid {} by bidder {}", amount, bidder);
                }
                Action::LowBid { bidder } => {
                    let amount = model.min_required() - 1;
                    let expected = model.bid(bidder, amount).map_err(code);
                    prop_assert!(expected.is_err());
                    let got = outcome(client.try_bid(&bidders[bidder], &amount));
                    prop_assert_eq!(got, expected, "low bid {} by bidder {}", amount, bidder);
                }
                Action::Withdraw { bidder } => {
                    let before = token.balance(&bidders[bidder]);
                    let expected = model.withdraw(bidder);
                    let got = outcome(client.try_withdraw(&bidders[bidder]));
                    prop_assert_eq!(got, expected.map(|_| ()).map_err(code));
                    // Refund accessibility: exactly the owed refund is paid out.
                    let paid = token.balance(&bidders[bidder]) - before;
                    prop_assert_eq!(paid, expected.unwrap_or(0));
                }
                Action::Extend { ledgers } => {
                    model.ledger += ledgers;
                    let ledger = model.ledger;
                    env.ledger().with_mut(|l| l.sequence_number = ledger);
                }
                Action::Cancel => {
                    let expected = model.cancel().map_err(code);
                    let got = outcome(client.try_cancel(&seller));
                    prop_assert_eq!(got, expected);
                }
                Action::End => {
                    let expected = model.end().map_err(code);
                    let got = outcome(client.try_end());
                    prop_assert_eq!(got, expected);
                }
            }
            check_invariants(&client, &token, &seller, &bidders, &model, total_minted)?;
        }

        // Drive the auction to completion and let every participant claim.
        if !model.settled && !model.cancelled {
            model.ledger = model.ledger.max(model.deadline + 1);
            let ledger = model.ledger;
            env.ledger().with_mut(|l| l.sequence_number = ledger);
            let expected = model.end().map_err(code);
            prop_assert_eq!(outcome(client.try_end()), expected);
            prop_assert!(expected.is_ok());
        }
        for (i, bidder) in bidders.iter().enumerate() {
            if model.pending[i] > 0 {
                let before = token.balance(bidder);
                let owed = model.withdraw(i).unwrap();
                client.withdraw(bidder);
                prop_assert_eq!(token.balance(bidder) - before, owed);
            }
        }
        check_invariants(&client, &token, &seller, &bidders, &model, total_minted)?;

        // Zero trapped funds.
        prop_assert_eq!(model.liabilities(), 0);
        prop_assert_eq!(token.balance(&client.address), 0, "funds trapped in contract");
    }

    /// Property: Total funds withdrawn by bidders can never exceed total funds deposited via bids
    /// Closes #961 – auction bid/refund accounting invariant
    #[test]
    fn prop_auction_total_out_never_exceeds_total_in(
        bids in proptest::collection::vec(100i128..=10_000i128, 1..=10),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let (client, seller, token_addr, auction_addr) = setup_auction(&env, 50i128, 10i128);

        let mut bidders = vec![];
        let mut total_deposited = 0i128;

        // Place bids
        for bid_amount in bids {
            let bidder = Address::generate(&env);
            StellarAssetClient::new(&env, &token_addr).mint(&bidder, &bid_amount);

            let result = client.try_bid(&bidder, &bid_amount);
            if result.is_ok() {
                total_deposited += bid_amount;
                bidders.push(bidder);
            }
        }

        // Withdraw all pending refunds
        let mut total_withdrawn = 0i128;
        for bidder in &bidders {
            let pending = client.get_pending(bidder);
            if pending > 0 {
                let balance_before = TokenClient::new(&env, &token_addr).balance(bidder);
                let withdraw_result = client.try_withdraw(bidder);
                if withdraw_result.is_ok() {
                    let balance_after = TokenClient::new(&env, &token_addr).balance(bidder);
                    total_withdrawn += balance_after - balance_before;
                }
            }
        }

        // End auction and withdraw winner
        env.ledger().with_mut(|l| l.sequence_number += 1001);
        let _ = client.try_end();

        // Check final balances
        for bidder in &bidders {
            let pending = client.get_pending(bidder);
            if pending > 0 {
                let balance_before = TokenClient::new(&env, &token_addr).balance(bidder);
                let _ = client.try_withdraw(bidder);
                let balance_after = TokenClient::new(&env, &token_addr).balance(bidder);
                total_withdrawn += balance_after - balance_before;
            }
        }

        // Invariant: total withdrawn <= total deposited
        prop_assert!(total_withdrawn <= total_deposited,
            "Refunds exceeded deposits: deposited={}, withdrawn={}",
            total_deposited, total_withdrawn);

        // Invariant: contract balance + total_withdrawn + seller_payout = total_deposited
        let contract_balance = soroban_sdk::token::Client::new(&env, &token_addr).balance(&auction_addr);
        let seller_payout = soroban_sdk::token::Client::new(&env, &token_addr).balance(&seller);
        prop_assert_eq!(contract_balance + total_withdrawn + seller_payout, total_deposited,
        // Invariant: contract balance + total_withdrawn = total_deposited
        let contract_balance = TokenClient::new(&env, &token_addr).balance(&auction_addr);
        prop_assert_eq!(contract_balance + total_withdrawn, total_deposited,
            "Balance accounting mismatch");
    }

    /// Property: Pending refunds never go negative
    #[test]
    fn prop_pending_refunds_never_negative(
        bid_amounts in proptest::collection::vec(50i128..=1_000i128, 2..=5),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let (client, _seller, token_addr, _) = setup_auction(&env, 10i128, 5i128);

        let mut bidders = vec![];
        for bid_amount in bid_amounts {
            let bidder = Address::generate(&env);
            StellarAssetClient::new(&env, &token_addr).mint(&bidder, &bid_amount);

            if client.try_bid(&bidder, &bid_amount).is_ok() {
                bidders.push(bidder);
            }
        }

        // Check all pending amounts are non-negative
        for bidder in &bidders {
            let pending = client.get_pending(bidder);
            prop_assert!(pending >= 0, "Negative pending refund detected: {}", pending);
        }
    }

    /// Property: Highest bid always increases or stays the same
    #[test]
    fn prop_highest_bid_monotonic(
        increments in proptest::collection::vec(10i128..=100i128, 1..=10),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let start_price = 100i128;
        let (client, _seller, token_addr, _) = setup_auction(&env, start_price, 10i128);

        let mut prev_highest = start_price - 1;
        let mut current_bid = start_price;

        for increment in increments {
            let bidder = Address::generate(&env);
            StellarAssetClient::new(&env, &token_addr).mint(&bidder, &current_bid);

            if client.try_bid(&bidder, &current_bid).is_ok() {
                let info = client.get_info();
                prop_assert!(info.highest_bid >= prev_highest,
                    "Highest bid decreased: {} -> {}", prev_highest, info.highest_bid);
                prev_highest = info.highest_bid;
                current_bid = info.highest_bid + increment;
            }
        }
    }

    /// Property: Reserve price prevents seller payout when not met
    #[test]
    fn prop_reserve_price_enforcement(
        start_price in 100i128..=1_000i128,
        reserve_price in 500i128..=2_000i128,
        bid_amount in 100i128..=3_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let seller = Address::generate(&env);
        let bidder = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        let auction_addr = env.register_contract(None, AuctionContract);
        let client = AuctionContractClient::new(&env, &auction_addr);

        let deadline = env.ledger().sequence() + 100;
        let result = client.try_start(
            &seller,
            &token_addr,
            &start_price,
            &10i128,
            &deadline,
            &Some(reserve_price),
            &0,
            &u32::MAX,
            &0,
            &0,
            &None,
            &None,
        );

        if result.is_err() {
            return Ok(());
        }

        if bid_amount >= start_price {
            StellarAssetClient::new(&env, &token_addr).mint(&bidder, &bid_amount);
            let _ = client.try_bid(&bidder, &bid_amount);
        }

        // End auction
        env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
        let _ = client.try_end();

        let seller_balance = TokenClient::new(&env, &token_addr).balance(&seller);

        if bid_amount >= reserve_price && bid_amount >= start_price {
            // Reserve met: seller should receive funds
            prop_assert!(seller_balance > 0 || bid_amount == 0,
                "Seller didn't receive payment when reserve was met");
        } else {
            // Reserve not met: seller should receive nothing
            prop_assert_eq!(seller_balance, 0,
                "Seller received payment when reserve was not met");
        }
    }
}

proptest! {
    /// Issue #1071: the Dutch price never increases over time, stays within
    /// `[floor_price, start_price]`, and clamps at both ends of the schedule.
    #[test]
    fn prop_dutch_price_monotonic_and_clamped(
        (start_price, floor_price) in (1i128..=i128::MAX).prop_flat_map(|s| (Just(s), 0i128..s)),
        start_ledger in 0u32..=1_000_000,
        duration_ledgers in 1u32..=1_000_000,
        t1 in 0u32..=3_000_000,
        dt in 0u32..=3_000_000,
    ) {
        let cfg = DutchConfig { start_price, floor_price, start_ledger, duration_ledgers };
        let t2 = t1.saturating_add(dt);

        // The split-division formula never overflows for valid schedules.
        let p1 = price_at(&cfg, t1).unwrap();
        let p2 = price_at(&cfg, t2).unwrap();

        prop_assert!(p2 <= p1, "price increased: {} -> {}", p1, p2);
        prop_assert!(p1 >= floor_price && p1 <= start_price);
        if t1 <= start_ledger {
            prop_assert_eq!(p1, start_price);
        }
        if t1 >= start_ledger + duration_ledgers {
            prop_assert_eq!(p1, floor_price);
        }
    }

    /// Issue #1071: for values where the textbook formula cannot overflow, the
    /// contract's price matches it exactly.
    #[test]
    fn prop_dutch_price_matches_reference_formula(
        (start_price, floor_price) in (1i128..=1_000_000_000_000i128)
            .prop_flat_map(|s| (Just(s), 0i128..s)),
        duration_ledgers in 1u32..=100_000,
        elapsed in 0u32..=100_000,
    ) {
        let cfg = DutchConfig { start_price, floor_price, start_ledger: 0, duration_ledgers };
        let expected = if elapsed >= duration_ledgers {
            floor_price
        } else {
            start_price
                - (start_price - floor_price) * i128::from(elapsed) / i128::from(duration_ledgers)
        };
        prop_assert_eq!(price_at(&cfg, elapsed).unwrap(), expected);
    }

    /// Issue #1070: bidding near `i128::MAX` returns `AuctionError::Overflow`
    /// (or a normal validation error) instead of trapping the host.
    #[test]
    fn prop_bid_near_i128_max_returns_error_not_panic(
        headroom in 0i128..=1_000_000i128,
        min_increment in 1i128..=2_000_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let start_price = i128::MAX - headroom;
        let (client, _seller, token_addr, _) = setup_auction(&env, start_price, min_increment);

        let first = Address::generate(&env);
        StellarAssetClient::new(&env, &token_addr).mint(&first, &start_price);
        client.bid(&first, &start_price);

        let second = Address::generate(&env);
        match start_price.checked_add(min_increment) {
            None => {
                prop_assert_eq!(
                    client.try_bid(&second, &i128::MAX),
                    Err(Ok(AuctionError::Overflow))
                );
            }
            Some(required) => {
                prop_assert_eq!(
                    client.try_bid(&second, &(required - 1)),
                    Err(Ok(AuctionError::BidTooLow))
                );
            }
        }
        prop_assert_eq!(client.get_info().highest_bid, start_price);
    }

    /// Issue #1070: queueing a refund onto a pending balance near `i128::MAX`
    /// returns `AuctionError::Overflow` exactly when the sum would overflow.
    #[test]
    fn prop_pending_refund_near_i128_max(existing in (i128::MAX - 2_000)..=i128::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let (client, _seller, token_addr, auction_addr) = setup_auction(&env, 1_000, 100);
        let b1 = Address::generate(&env);
        let b2 = Address::generate(&env);
        StellarAssetClient::new(&env, &token_addr).mint(&b1, &1_000);
        StellarAssetClient::new(&env, &token_addr).mint(&b2, &1_100);
        client.bid(&b1, &1_000);

        env.as_contract(&auction_addr, || {
            env.storage()
                .persistent()
                .set(&DataKey::Pending(b1.clone()), &existing);
        });

        let result = client.try_bid(&b2, &1_100);
        if existing.checked_add(1_000).is_none() {
            prop_assert_eq!(result, Err(Ok(AuctionError::Overflow)));
            prop_assert_eq!(client.get_pending(&b1), existing);
        } else {
            prop_assert!(result.is_ok());
            prop_assert_eq!(client.get_pending(&b1), existing + 1_000);
        }
    }

    /// Issue #1068: mixing `bid` and `bid_with_credit` keeps the contract exactly
    /// solvent: its balance equals the highest bid plus all pending refunds.
    #[test]
    fn prop_bid_with_credit_preserves_accounting(
        steps in proptest::collection::vec((0i128..=500i128, any::<bool>()), 1..=12),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.sequence_number = 100);

        let (client, _seller, token_addr, auction_addr) = setup_auction(&env, 100, 10);
        let token = soroban_sdk::token::Client::new(&env, &token_addr);
        let bidders = [Address::generate(&env), Address::generate(&env)];
        for bidder in &bidders {
            StellarAssetClient::new(&env, &token_addr).mint(bidder, &1_000_000);
        }

        let mut next_min = 100i128;
        for (i, (extra, use_credit)) in steps.into_iter().enumerate() {
            let bidder = &bidders[i % 2];
            let amount = next_min + extra;
            if use_credit {
                client.bid_with_credit(bidder, &amount);
            } else {
                client.bid(bidder, &amount);
            }
            next_min = amount + 10;

            let owed = amount + client.get_pending(&bidders[0]) + client.get_pending(&bidders[1]);
            prop_assert_eq!(token.balance(&auction_addr), owed);
            let total = token.balance(&bidders[0]) + token.balance(&bidders[1]) + owed;
            prop_assert_eq!(total, 2_000_000);
        }
    }
}
