#![no_std]
#![deny(missing_docs)]
//! Commit-reveal lottery contract template.
//!
//! Players buy tickets while the lottery is open; the admin commits to a hashed
//! secret then reveals it to draw verifiable random winners.

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
/// 2. Anyone calls `buy_ticket` (while Open).
/// 3. Admin calls `commit` with hash(secret ++ salt) and a reveal deadline (→ Committed).
/// 4. Admin calls `draw` revealing secret and salt (→ Drawn); winners receive their prize share.
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
        /// `max_tickets_per_address`, if set, caps how many tickets a single address
        /// may buy in total across repeated calls to `buy_ticket`.
        ///
        /// # Errors
        /// - [`LotteryError::AlreadyInitialized`]
        /// - [`LotteryError::InvalidTicketPrice`] if ticket_price <= 0
        /// - [`LotteryError::InvalidWinnerConfig`] if winner_count is zero, `prize_splits` length
        ///   doesn't match `winner_count`, or the splits don't sum to 10 000 basis points
        /// - [`LotteryError::InvalidTicketCap`] if `max_tickets_per_address` is `Some(0)`
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
            ticket_price: i128,
            winner_count: u32,
            prize_splits: Vec<u32>,
            max_tickets_per_address: Option<u32>,
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
            admin.require_auth();

            env.storage().instance().set(&Admin, &admin);
            env.storage().instance().set(&Token, &token);
            env.storage().instance().set(&TicketPrice, &ticket_price);
            env.storage()
                .instance()
                .set(&Participants, &Vec::<Address>::new(&env));
            env.storage().instance().set(&State, &LotteryState::Open);
            env.storage().instance().set(&WinnerCount, &winner_count);
            env.storage().instance().set(&PrizeSplits, &prize_splits);
            if let Some(cap) = max_tickets_per_address {
                env.storage().instance().set(&MaxTicketsPerAddress, &cap);
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
            let state: LotteryState = get_required(&env, &State)?;
            if state != LotteryState::Open {
                return Err(LotteryError::LotteryClosed);
            }

            buyer.require_auth();

            let ticket_count: u32 = env
                .storage()
                .persistent()
                .get(&TicketCount(&buyer))
                .unwrap_or(0);
            if let Some(cap) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&MaxTicketsPerAddress)
            {
                if ticket_count >= cap {
                    return Err(LotteryError::TicketCapExceeded);
                }
            }

            let ticket_price: i128 = get_required(&env, &TicketPrice)?;
            let token_addr: Address = get_required(&env, &Token)?;

            let mut participants: Vec<Address> = get_required(&env, &Participants)?;
            participants.push_back(&buyer);
            env.storage().instance().set(&Participants, &participants);

            env.storage()
                .persistent()
                .set(&TicketCount(&buyer), &(ticket_count + 1));
            env.storage().persistent().extend_ttl(
                &TicketCount(&buyer),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            token::Client::new(&env, &token_addr).transfer(
                &buyer,
                &env.current_contract_address(),
                &ticket_price,
            );

            bump_instance(&env);
            events::ticket_purchased(&env, &buyer);
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

            let participants: Vec<Address> = get_required(&env, &Participants)?;
            if participants.is_empty() {
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

            let participants: Vec<Address> = get_required(&env, &Participants)?;
            let winner_count: u32 = get_required(&env, &WinnerCount)?;
            let prize_splits: Vec<u32> = get_required(&env, &PrizeSplits)?;

            // Distinct-address count must cover the configured winner count.
            let mut seen: Vec<Address> = Vec::new(&env);
            for p in participants.iter() {
                if !seen.contains(&p) {
                    seen.push_back(&p);
                }
            }
            if seen.len() < winner_count {
                return Err(LotteryError::InsufficientParticipants);
            }

            let ledger_bytes = env.ledger().sequence().to_be_bytes();
            let mut pool: Vec<Address> = participants.clone();
            let mut winners: Vec<Address> = Vec::new(&env);

            for round in 0..winner_count {
                // Derive this round's winner index from hash(secret ++ salt ++ ledger ++ round).
                let round_bytes = round.to_be_bytes();
                let entropy = entropy_hash(&env, &secret, &salt, &ledger_bytes);
                let idx = entropy_index(&entropy, pool.len());

                #[allow(clippy::unwrap_used)] // idx is derived from modulo of len, always in bounds
                let winner = pool.get(idx).unwrap();
                winners.push_back(&winner);

                // Remove every ticket held by this winner so they can't be drawn again.
                let mut remaining = Vec::new(&env);
                for p in pool.iter() {
                    if p != winner {
                        remaining.push_back(&p);
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
            let total_prize = ticket_price * participants.len() as i128;
            let token_addr: Address = get_required(&env, &Token)?;
            let token_client = token::Client::new(&env, &token_addr);

            for i in 0..winner_count {
                #[allow(clippy::unwrap_used)] // i < winner_count == winners.len() == prize_splits.len()
                let winner = winners.get(i).unwrap();
                #[allow(clippy::unwrap_used)]
                let split = prize_splits.get(i).unwrap();
                #[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
                let amount = apply_bps_fee(total_prize, split).unwrap_or(0);
                token_client.transfer(&env.current_contract_address(), &winner, &amount);
                events::winner_drawn(&env, &winner, amount);
            }

            bump_instance(&env);
            Ok(winners)
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

            let participants: Vec<Address> = get_required(&env, &Participants)?;
            let mut ticket_count: i128 = 0;
            let mut remaining: Vec<Address> = Vec::new(&env);
            for p in participants.iter() {
                if p == buyer {
                    #[allow(clippy::arithmetic_side_effects)] // bounded by ticket sales, no overflow risk
                    {
                        ticket_count += 1;
                    }
                } else {
                    remaining.push_back(&p);
                }
            }
            if ticket_count == 0 {
                return Err(LotteryError::NothingToRefund);
            }
            env.storage().instance().set(&Participants, &remaining);

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
                participants: get_required(&env, &Participants)?,
            })
        }

        /// Return the first (or only) winner address (only available after draw).
        /// Return the winner address (only available after draw).
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
