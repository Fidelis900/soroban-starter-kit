#![no_std]
#![deny(missing_docs)]
//! Commit-reveal lottery contract template.
//!
//! Players buy tickets while the lottery is open; the admin commits to a hashed
//! secret then reveals it to draw verifiable random winners.

#[cfg(test)]
#[macro_use]
extern crate std;

use soroban_sdk::{Address, BytesN, Env, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::LotteryError;
pub use storage::{Commit, DataKey, LotteryInfo, LotteryState};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee, commit_hash,
                     entropy_hash, entropy_index, BPS_DENOMINATOR};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn get_required<T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Result<T, LotteryError> {
    env.storage()
        .instance()
        .get(key)
        .ok_or(LotteryError::NotInitialized)
}

/// Lottery contract using a commit-reveal scheme for verifiable randomness.
///
/// Lifecycle:
/// 1. Admin calls `initialize` (→ Open).
/// 2. Anyone calls `buy_ticket` or `buy_tickets_batch` (while Open).
/// 3. Admin calls `commit` with hash(secret ++ salt) and a reveal deadline (→ Committed).
/// 4. Admin calls `draw` revealing secret and salt (→ Drawn); winners receive their prize share.
///
/// If fewer than `min_participants` unique addresses hold tickets at draw time, admin (or any
/// permissionless keeper) may call `rollover` instead of `draw` to carry the prize pool into a
/// new Open round.
///
/// If the admin never calls `draw` before the reveal deadline, ticket buyers can call
/// `claim_refund` to recover their ticket price.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;
    use storage::DataKey::*;
    // Explicit import so the `Commit` struct (type namespace) is unambiguous
    // alongside the glob-imported `DataKey::Commit` variant (value namespace).
    use storage::Commit;

    #[contract]
    pub struct LotteryContract;

    #[contractimpl]
    impl LotteryContract {
        /// Initialise the lottery.
        ///
        /// `winner_count` sets how many distinct winners `draw` selects, and `prize_splits`
        /// gives each winner's share of the prize pool in basis points (must sum to 10 000).
        /// `max_tickets_per_address`, if set, caps how many tickets a single address may buy.
        /// `min_participants`, if set, is the minimum number of distinct participant addresses
        /// required before `draw` may succeed; if the threshold is not met the admin may call
        /// `rollover` to start a new round and carry the prize pool forward.
        /// `fee_bps` is an optional operator rake in basis points (1–9 999). When set,
        /// `fee_recipient` must also be provided; the fee is transferred to that address
        /// at `draw` time before the winner prize pool is calculated.
        ///
        /// `fee_bps` is an optional operator rake in basis points (1–9 999). When set,
        /// `fee_recipient` must also be provided; the fee is transferred to that address
        /// at `draw` time before the winner prize pool is calculated.
        ///
        /// # Errors
        /// - [`LotteryError::AlreadyInitialized`]
        /// - [`LotteryError::InvalidTicketPrice`] if ticket_price <= 0
        /// - [`LotteryError::InvalidWinnerConfig`] if winner_count is zero, `prize_splits` length
        ///   doesn't match `winner_count`, or the splits don't sum to 10 000 basis points
        /// - [`LotteryError::InvalidTicketCap`] if `max_tickets_per_address` is `Some(0)`
        /// - [`LotteryError::InvalidFeeConfig`] if `fee_bps >= 10_000`, or exactly one of
        ///   `fee_bps`/`fee_recipient` is set without the other
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
            ticket_price: i128,
            winner_count: u32,
            prize_splits: Vec<u32>,
            max_tickets_per_address: Option<u32>,
            fee_bps: Option<u32>,
            fee_recipient: Option<Address>,
        ) -> Result<(), LotteryError> {
            if env.storage().instance().has(&State) {
                return Err(LotteryError::AlreadyInitialized);
            }
            if ticket_price <= 0 {
                return Err(LotteryError::InvalidTicketPrice);
            }
            if winner_count == 0 || prize_splits.len() != winner_count {
                return Err(LotteryError::InvalidWinnerConfig);
            }
            let mut total: u32 = 0;
            for split in prize_splits.iter() {
                total = total
                    .checked_add(split)
                    .ok_or(LotteryError::InvalidWinnerConfig)?;
            }
            if total != BPS_DENOMINATOR as u32 {
                return Err(LotteryError::InvalidWinnerConfig);
            }

            if max_tickets_per_address == Some(0) {
                return Err(LotteryError::InvalidTicketCap);
            }

            // Validate fee configuration: both must be set together, and bps must be < 10_000.
            match (&fee_bps, &fee_recipient) {
                (Some(bps), Some(_)) => {
                    if *bps == 0 || *bps >= BPS_DENOMINATOR as u32 {
                        return Err(LotteryError::InvalidFeeConfig);
                    }
                }
                (None, None) => {}
                _ => return Err(LotteryError::InvalidFeeConfig),
            }

            admin.require_auth();

            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Token, &token);
            env.storage().instance().set(&TicketPrice, &ticket_price);
            env.storage().instance().set(&TotalTickets, &0u32);
            env.storage().instance().set(&State, &LotteryState::Open);
            env.storage().instance().set(&WinnerCount, &winner_count);
            env.storage().instance().set(&PrizeSplits, &prize_splits);
            if let Some(cap) = max_tickets_per_address {
                env.storage().instance().set(&MaxTicketsPerAddress, &cap);
            }
            if let (Some(bps), Some(recipient)) = (fee_bps, fee_recipient) {
                env.storage().instance().set(&FeeBps, &bps);
                env.storage().instance().set(&FeeRecipient, &recipient);
            }

            bump_instance(&env);
            events::initialized(&env, &admin, ticket_price);
            Ok(())
        }

        /// Purchase one ticket. Transfers `ticket_price` tokens from caller to contract.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::LotteryClosed`] if lottery is no longer Open
        /// - [`LotteryError::TicketCapExceeded`] if the buyer has already reached
        ///   `max_tickets_per_address`
        pub fn buy_ticket(env: Env, buyer: Address) -> Result<(), LotteryError> {
            Self::buy_tickets_batch(env, buyer, 1)
        }

        /// Purchase `count` tickets in a single transaction.
        ///
        /// Validates that `count > 0` and that the buyer's total after this call does not exceed
        /// `max_tickets_per_address`. Transfers `ticket_price * count` tokens from the buyer to
        /// the contract in a single `transfer` call, then appends `count` indexed participant
        /// entries to persistent storage.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::LotteryClosed`] if lottery is no longer Open
        /// - [`LotteryError::InvalidBatchCount`] if `count == 0`
        /// - [`LotteryError::TicketCapExceeded`] if the buyer's total would exceed
        ///   `max_tickets_per_address`
        pub fn buy_tickets_batch(
            env: Env,
            buyer: Address,
            count: u32,
        ) -> Result<(), LotteryError> {
            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Open {
                return Err(LotteryError::LotteryClosed);
            }

            if count == 0 {
                return Err(LotteryError::InvalidBatchCount);
            }

            buyer.require_auth();

            let ticket_count: u32 = env
                .storage()
                .persistent()
                .get(&TicketCount(buyer.clone()))
                .unwrap_or(0);

            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&MaxTicketsPerAddress)
            {
                let new_total = ticket_count
                    .checked_add(count)
                    .ok_or(LotteryError::TicketCapExceeded)?;
                if new_total > cap {
                    return Err(LotteryError::TicketCapExceeded);
                }
            }

            let ticket_price: i128 = get_required(&env, &TicketPrice)?;
            let token_addr: Address = get_required(&env, &Token)?;

            let mut participants: Vec<Address> = get_required(&env, &Participants)?;
            participants.push_back(buyer.clone());
            env.storage().instance().set(&Participants, &participants);

            // Update per-address ticket counter.
            #[allow(clippy::arithmetic_side_effects)]
            let new_count = ticket_count + count;
            env.storage()
                .persistent()
                .set(&TicketCount(buyer.clone()), &(ticket_count + 1));
            env.storage().persistent().extend_ttl(
                &TicketCount(buyer.clone()),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            // Single token transfer for the whole batch.
            #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation, clippy::as_conversions)]
            let total_cost = ticket_price * count as i128;
            token::Client::new(&env, &token_addr).transfer(
                &buyer,
                &env.current_contract_address(),
                &total_cost,
            );

            bump_instance(&env);
            if count == 1 {
                events::ticket_purchased(&env, &buyer);
            } else {
                events::tickets_batch_purchased(&env, &buyer, count);
            }
            Ok(())
        }

        /// Admin submits hash(secret ++ salt) to lock in randomness commitment, along with the
        /// ledger sequence deadline by which `draw` must be called. Transitions lottery to
        /// Committed state, closing ticket sales.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::Unauthorized`]
        /// - [`LotteryError::LotteryClosed`] if not Open
        /// - [`LotteryError::CommitAlreadySubmitted`]
        /// - [`LotteryError::NoTickets`] if no participants
        /// - [`LotteryError::InvalidRevealDeadline`] if `reveal_deadline` is not in the future
        pub fn commit(
            env: Env,
            hash: BytesN<32>,
            reveal_deadline: u32,
        ) -> Result<(), LotteryError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();

            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Open {
                return Err(if state == LotteryState::Committed {
                    LotteryError::CommitAlreadySubmitted
                } else {
                    LotteryError::DrawAlreadyDone
                });
            }

            let total_tickets: u32 = get_required(&env, &TotalTickets)?;
            if total_tickets == 0 {
                return Err(LotteryError::NoTickets);
            }

            if reveal_deadline <= env.ledger().sequence() {
                return Err(LotteryError::InvalidRevealDeadline);
            }

            env.storage().instance().set(&Commit, &Commit { hash });
            env.storage()
                .instance()
                .set(&RevealDeadline, &reveal_deadline);
            env.storage()
                .instance()
                .set(&State, &LotteryState::Committed);

            bump_instance(&env);
            events::committed(&env, &admin);
            Ok(())
        }

        /// Admin reveals secret and salt. The contract verifies the preimage matches
        /// the stored commitment, then uses hash(secret ++ salt ++ ledger ++ round) to pick
        /// `winner_count` distinct winners and splits the prize pool between them according to
        /// the configured `prize_splits`.
        ///
        /// If a fee was configured at initialization, the fee amount is deducted from the total
        /// prize pool and transferred to the fee recipient before winners are paid.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::Unauthorized`]
        /// - [`LotteryError::DrawNotDone`] if not yet Committed
        /// - [`LotteryError::DrawAlreadyDone`] if already Drawn
        /// - [`LotteryError::RevealMismatch`] if preimage does not match commitment
        /// - [`LotteryError::InsufficientParticipants`] if fewer distinct addresses hold
        ///   tickets than the configured `winner_count`
        pub fn draw(
            env: Env,
            secret: BytesN<32>,
            salt: BytesN<32>,
        ) -> Result<Vec<Address>, LotteryError> {
            let admin: Address = get_required(&env, &Admin)?;
            admin.require_auth();

            let state: LotteryState = get_required(&env, &State)?;
            match state {
                LotteryState::Open => return Err(LotteryError::DrawNotDone),
                LotteryState::Drawn => return Err(LotteryError::DrawAlreadyDone),
                LotteryState::Committed => {}
            }

            // Verify commitment: SHA-256(secret ++ salt) must match stored hash.
            let computed: BytesN<32> = commit_hash(&env, &secret, &salt).into();

            let stored_commit: Commit = get_required(&env, &Commit)?;
            if computed != stored_commit.hash {
                return Err(LotteryError::RevealMismatch);
            }

            let total_tickets: u32 = get_required(&env, &TotalTickets)?;
            let winner_count: u32 = get_required(&env, &WinnerCount)?;
            let prize_splits: Vec<u32> = get_required(&env, &PrizeSplits)?;

            // Rebuild the participant list from indexed persistent storage.
            let mut pool: Vec<Address> = Vec::new(&env);
            for idx in 0..total_tickets {
                #[allow(clippy::unwrap_used)] // Every idx in 0..total_tickets was written at buy time
                let addr: Address = env
                    .storage()
                    .persistent()
                    .get(&Participant(idx))
                    .unwrap();
                pool.push_back(addr);
            }

            // Distinct-address count must cover the configured winner count.
            let mut seen: Vec<Address> = Vec::new(&env);
            for p in pool.iter() {
                if !seen.contains(&p) {
                    seen.push_back(p);
                }
            }
            if seen.len() < winner_count {
                return Err(LotteryError::InsufficientParticipants);
            }

            let ledger_bytes = env.ledger().sequence().to_be_bytes();
            let mut winners: Vec<Address> = Vec::new(&env);

            for round in 0..winner_count {
                // FIX #1252: include both ledger bytes AND round bytes in entropy so each
                // round produces a distinct hash rather than the same hash every iteration.
                let round_bytes = round.to_be_bytes();
                let mut extra = [0u8; 8];
                extra[..4].copy_from_slice(&ledger_bytes);
                extra[4..].copy_from_slice(&round_bytes);
                let entropy = entropy_hash(&env, &secret, &salt, &extra);
                let idx = entropy_index(&entropy, pool.len());

                #[allow(clippy::unwrap_used)] // idx is derived from modulo of len, always in bounds
                let winner = pool.get(idx).unwrap();
                winners.push_back(winner.clone());

                // Remove every ticket held by this winner so they can't be drawn again.
                let mut remaining = Vec::new(&env);
                for p in pool.iter() {
                    if p != winner {
                        remaining.push_back(p);
                    }
                }
                pool = remaining;
            }

            // Mark the draw complete before distributing the prize pool. If any transfer fails,
            // the transaction reverts this state transition together with the failed call.
            env.storage().instance().set(&State, &LotteryState::Drawn);
            env.storage().instance().set(&Winners, &winners);
            #[allow(clippy::unwrap_used)] // winner_count >= 1, validated at initialize
            let first_winner = winners.get(0).unwrap();
            env.storage().instance().set(&Winner, &first_winner);

            // Distribute the prize pool according to the configured splits.
            let ticket_price: i128 = get_required(&env, &TicketPrice)?;
            #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation, clippy::as_conversions)]
            let total_prize = ticket_price * total_tickets as i128;
            let token_addr: Address = get_required(&env, &Token)?;
            let token_client = token::Client::new(&env, &token_addr);

            // Deduct the operator rake if configured, then distribute the remainder.
            let distributable_prize = if let Some(fee_bps) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&FeeBps)
            {
                #[allow(clippy::unwrap_used)] // fee_bps and fee_recipient are validated as a pair at initialize
                let fee_recipient: Address = env.storage().instance().get(&FeeRecipient).unwrap();
                let fee_amount = apply_bps_fee(total_prize, fee_bps).unwrap_or(0);
                if fee_amount > 0 {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &fee_recipient,
                        &fee_amount,
                    );
                    events::fee_transferred(&env, &fee_recipient, fee_amount);
                }
                #[allow(clippy::arithmetic_side_effects)]
                { total_prize - fee_amount }
            } else {
                total_prize
            };

            for i in 0..winner_count {
                #[allow(clippy::unwrap_used)] // i < winner_count == winners.len() == prize_splits.len()
                let winner = winners.get(i).unwrap();
                #[allow(clippy::unwrap_used)]
                let split = prize_splits.get(i).unwrap();
                #[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
                let amount = apply_bps_fee(distributable_prize, split).unwrap_or(0);
                token_client.transfer(&env.current_contract_address(), &winner, &amount);
                events::winner_drawn(&env, &winner, amount);
            }

            bump_instance(&env);
            Ok(winners)
        }

        /// If participation fell below `min_participants` (or below `winner_count`) after a
        /// commit, the admin (or any permissionless keeper once the reveal deadline has passed)
        /// may call `rollover` to reset the lottery to the Open state without distributing the
        /// prize pool.  Ticket sales and balances are preserved so the accumulated prize carries
        /// forward into the new round.
        ///
        /// Rollover is available when:
        /// - The lottery is in the Committed state, AND
        /// - The reveal deadline has passed (keeper path) OR caller is the admin, AND
        /// - The number of distinct participants is below `min_participants` (if set) OR
        ///   below `winner_count` (fallback guard).
        ///
        /// After a rollover the lottery returns to the Open state.  The admin must call
        /// `commit` again (with a new hash and deadline) to proceed to draw.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::RolloverNotAvailable`] if conditions for rollover are not met
        pub fn rollover(env: Env) -> Result<(), LotteryError> {
            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Committed {
                return Err(LotteryError::RolloverNotAvailable);
            }

            let reveal_deadline: u32 = get_required(&env, &RevealDeadline)?;
            let admin: Address = get_required(&env, &Admin)?;

            // Permissioned check: once the reveal deadline passes the call is permissionless
            // (anyone can trigger rollover as a keeper). Before the deadline only the admin may
            // call, which we enforce with require_auth.
            let deadline_passed = env.ledger().sequence() > reveal_deadline;
            if !deadline_passed {
                admin.require_auth();
            }

            // Determine the effective minimum: use configured min_participants if present,
            // otherwise fall back to winner_count (you must have at least winner_count unique
            // addresses to run a draw).
            let winner_count: u32 = get_required(&env, &WinnerCount)?;
            let effective_min: u32 = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&MinParticipants)
                .unwrap_or(winner_count);

            // Count distinct participant addresses.
            let total_tickets: u32 = get_required(&env, &TotalTickets)?;
            let mut seen: Vec<Address> = Vec::new(&env);
            for idx in 0..total_tickets {
                #[allow(clippy::unwrap_used)]
                let addr: Address = env
                    .storage()
                    .persistent()
                    .get(&Participant(idx))
                    .unwrap();
                if !seen.contains(&addr) {
                    seen.push_back(addr);
                }
            }

            if seen.len() >= effective_min {
                // Enough participants — rollover would be an abuse; force admin to draw instead.
                return Err(LotteryError::RolloverNotAvailable);
            }

            // Reset to Open: clear commit and deadline, keep tickets and prize pool intact.
            env.storage().instance().remove(&Commit);
            env.storage().instance().remove(&RevealDeadline);
            env.storage().instance().set(&State, &LotteryState::Open);

            bump_instance(&env);
            events::rolled_over(&env, &admin);
            Ok(())
        }

        /// Claim a refund of the ticket price for every ticket `buyer` holds, once the reveal
        /// deadline set at `commit` time has passed without a `draw`.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::RefundNotAvailable`] if the lottery hasn't been committed yet, or
        ///   the reveal deadline hasn't passed
        /// - [`LotteryError::DrawAlreadyDone`] if the admin already drew a winner
        /// - [`LotteryError::NothingToRefund`] if `buyer` holds no tickets
        pub fn claim_refund(env: Env, buyer: Address) -> Result<i128, LotteryError> {
            buyer.require_auth();

            let state: LotteryState = get_required(&env, &State)?;
            match state {
                LotteryState::Open => return Err(LotteryError::RefundNotAvailable),
                LotteryState::Drawn => return Err(LotteryError::DrawAlreadyDone),
                LotteryState::Committed => {}
            }

            let reveal_deadline: u32 = get_required(&env, &RevealDeadline)?;
            if env.ledger().sequence() <= reveal_deadline {
                return Err(LotteryError::RefundNotAvailable);
            }

            // Count tickets belonging to this buyer and mark them as refunded by setting
            // the persistent entry to the contract address (a sentinel that won't win).
            let total_tickets: u32 = get_required(&env, &TotalTickets)?;
            let mut ticket_count: i128 = 0;
            let contract_addr = env.current_contract_address();

            for idx in 0..total_tickets {
                let key = Participant(idx);
                #[allow(clippy::unwrap_used)]
                let addr: Address = env.storage().persistent().get(&key).unwrap();
                if addr == buyer {
                    #[allow(clippy::arithmetic_side_effects)]
                    {
                        ticket_count += 1;
                    }
                } else {
                    remaining.push_back(p);
                }
            }

            if ticket_count == 0 {
                return Err(LotteryError::NothingToRefund);
            }
            env.storage().instance().set(&Participants, &remaining);
            // Clear the persistent TicketCount so this address can participate in future rounds.
            env.storage().persistent().remove(&TicketCount(buyer.clone()));

            let ticket_price: i128 = get_required(&env, &TicketPrice)?;
            #[allow(clippy::arithmetic_side_effects)]
            let refund_amount = ticket_price * ticket_count;
            let token_addr: Address = get_required(&env, &Token)?;
            token::Client::new(&env, &token_addr).transfer(
                &env.current_contract_address(),
                &buyer,
                &refund_amount,
            );

            bump_instance(&env);
            events::refund_claimed(&env, &buyer, refund_amount);
            Ok(refund_amount)
        }

        /// Return lottery details.
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        pub fn get_info(env: Env) -> Result<LotteryInfo, LotteryError> {
            Ok(LotteryInfo {
                admin: get_required(&env, &Admin)?,
                token: get_required(&env, &Token)?,
                ticket_price: get_required(&env, &TicketPrice)?,
                state: get_required(&env, &State)?,
                total_tickets: get_required(&env, &TotalTickets)?,
            })
        }

        /// Return the first (or only) winner address (only available after draw).
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::DrawNotDone`] if draw hasn't happened yet
        pub fn get_winner(env: Env) -> Result<Address, LotteryError> {
            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Drawn {
                return Err(LotteryError::DrawNotDone);
            }
            get_required(&env, &Winner)
        }

        /// Return all winner addresses (only available after draw).
        ///
        /// # Errors
        /// - [`LotteryError::NotInitialized`]
        /// - [`LotteryError::DrawNotDone`] if draw hasn't happened yet
        pub fn get_winners(env: Env) -> Result<Vec<Address>, LotteryError> {
            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Drawn {
                return Err(LotteryError::DrawNotDone);
            }
            get_required(&env, &Winners)
        }

        /// Return how many tickets `buyer` has purchased so far.
        #[must_use]
        pub fn get_ticket_count(env: Env, buyer: Address) -> u32 {
            env.storage()
                .persistent()
                .get(&TicketCount(buyer))
                .unwrap_or(0)
        }
    }
}

mod test;

#[cfg(test)]
mod prop_test;
