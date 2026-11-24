// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]
#![no_std]
#![deny(missing_docs)]
//! Multi-choice on-chain ballot contract template.
//!
//! Voters cast a single vote among N registered choices; results are tallied
//! on-chain once voting closes.
//!
//! ## Multi-choice ballot (#788)
//!
//! `initialize` accepts a `choices: Vec<String>` parameter — an ordered list of
//! named options (e.g. `["yes", "no", "abstain"]`).  The `choice` argument to
//! `vote` is an index into that list (0-based).
//!
//! ## Voting window (#787, #1328)
//!
//! `initialize` accepts `voting_start` and `voting_end` ledger sequences.
//! `vote()` is rejected with [`BallotError::VotingNotStarted`] before the
//! window opens and with [`BallotError::VotingClosed`] after it closes.
//!
//! To guarantee voters have time to prepare, the window must be planned in the
//! future: `voting_start` must be at least [`MIN_DEADLINE_BUFFER`] ledgers
//! ahead of the current ledger, and `voting_end` must be at least
//! [`MIN_VOTING_DURATION`] ledgers after `voting_start`.
//!
//! ## Voter deregistration (#786)
//!
//! `deregister_voter` (admin-only) removes a registered voter, but only while
//! no vote has yet been cast.
//!
//! ## Quorum (#1126)
//!
//! `initialize` accepts a `quorum: u32` minimum turnout.  If fewer than
//! `quorum` votes are cast, `tally_all()` returns
//! [`BallotResult::QuorumNotMet`] instead of certifying the leading choice.
//!
//! ## Anonymous blinded reveal (#1331)
//!
//! `commit_vote_blinded` stores `hash(secret ++ choice ++ proof)` without
//! binding it to the voter's address.  During the reveal phase any relayer may
//! call `reveal_vote_blinded(commitment, secret, choice, proof)` to record the
//! vote.  The emitted event contains only the commitment hash and choice, so
//! the tally is not linked to the voter address.
//!
//! ## Permissionless tally (#1121)
//!
//! Once `voting_end` has passed, tally calculation is permissionless: any
//! caller may invoke `tally_all()` / `tally()` to close the ballot and emit
//! [`events::TallyCompleted`].  `get_tally()` is a read-only query that never
//! requires auth.
//!
//! ## Secret ballot commitment / reveal (#1499)
//!
//! Voters call `commit_vote` during the commit phase to store a commitment
//! `hash(voter ++ choice ++ salt)` without revealing their choice on-chain.
//! During the reveal phase the voter submits `reveal_vote(voter, choice, salt)`
//! which verifies the preimage and tallies the vote.  The admin controls phase
//! transitions via `start_commit_phase` / `start_reveal_phase`.  This prevents
//! the bandwagon effect: no choice counts are visible until the reveal phase.
//!
//! ## Quadratic voting (#1498)
//!
//! `vote_quadratic(voter, choice, tokens)` computes the voter's weight as the
//! integer square root of `tokens` (floor sqrt) and adds it to the tally.
//! This balances minority influence: a voter with 100 tokens contributes
//! weight 10, while one with 400 tokens contributes weight 20 rather than 4×.
//!
//! ## Voter eligibility whitelist / BitSet (#1500)
//!
//! The admin can maintain a compact BitSet of eligible voter addresses.
//! Each registered voter receives a sequential slot index; the slot index
//! determines which 128-bit storage word contains the voter's bit.
//! `whitelist_voter` and `unwhitelist_voter` toggle individual bits.
//! `is_whitelisted` reads a bit in O(1).  `vote_whitelisted` requires the
//! voter to be both registered and present in the BitSet.

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Vec};

mod errors;
mod events;
mod storage;

pub use errors::BallotError;
pub use storage::{DataKey, RoundResult};

/// Threshold (in ledgers) below which instance storage is extended.
const LEDGER_LIFETIME_THRESHOLD: u32 = 120_960;

/// Target TTL (in ledgers) applied when instance storage is extended.
const LEDGER_BUMP_AMOUNT: u32 = 518_400;

/// Minimum number of ledgers between the current ledger and `voting_start`.
///
/// Initialization rejects windows that open sooner, giving voters a planned
/// preparation period before the ballot becomes active.  Matches
/// `soroban_common::MIN_DEADLINE_BUFFER`.
pub const MIN_DEADLINE_BUFFER: u32 = 10;

/// Minimum number of ledgers a voting window must stay open.
///
/// `voting_end` must be at least this far after `voting_start`.
pub const MIN_VOTING_DURATION: u32 = 10;

/// Number of voter bits packed into a single storage word (#1500).
const BITSET_WORD_BITS: u32 = 128;

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

pub use ballot_result::BallotResult;

// `#[contracttype]` generates an undocumented public associated item; confine
// the missing_docs allowance to this module and re-export the result above.
mod ballot_result {
    #![allow(missing_docs)]
    use soroban_sdk::{contracttype, Vec};

    /// Outcome of a tally.
    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum BallotResult {
        /// Turnout met the configured quorum; carries the per-choice counts.
        Certified(Vec<i128>),
        /// Turnout was below the configured quorum; results are not certified.
        QuorumNotMet,
    }
}

/// Multi-choice on-chain ballot contract.
///
/// Flow:
/// 1. Admin calls `initialize` — sets up N named choices and a voting window.
/// 2. Admin calls `register_voter` to add voters.  Mistakes may be undone with
///    `deregister_voter` before any vote is cast.
/// 3. Voters call `vote(voter, choice_index)` within the voting window.
/// 4. Anyone calls `tally_all()` (or `tally()` for two-choice ballots) once the
///    voting window has closed to get final results and close voting.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct BallotContract;

    #[contractimpl]
    impl BallotContract {
        /// Initialize the ballot contract with N named choices.
        ///
        /// `voting_start` and `voting_end` are inclusive ledger sequence numbers
        /// defining the window during which votes are accepted.
        ///
        /// `choices` must be non-empty.  The index of each element becomes the
        /// `choice` value accepted by `vote`.
        ///
        /// `quorum` is the minimum number of votes that must be cast before
        /// results can be certified.
        ///
        /// The window must be planned in the future:
        /// `voting_start >= current_ledger + MIN_DEADLINE_BUFFER` and
        /// `voting_end >= voting_start + MIN_VOTING_DURATION`.
        ///
        /// # Errors
        /// - [`BallotError::AlreadyInitialized`] if called more than once.
        /// - [`BallotError::NoChoices`] if `choices` is empty.
        /// - [`BallotError::InvalidWindow`] if the voting window starts too
        ///   soon, is too short, or is otherwise malformed.
        pub fn initialize(
            env: Env,
            admin: Address,
            voting_start: u32,
            voting_end: u32,
            choices: Vec<String>,
            quorum: u32,
        ) -> Result<(), BallotError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::AlreadyInitialized);
            }
            if choices.is_empty() {
                return Err(BallotError::NoChoices);
            }

            let now = env.ledger().sequence();
            let earliest_start = now
                .checked_add(MIN_DEADLINE_BUFFER)
                .ok_or(BallotError::InvalidWindow)?;
            if voting_start < earliest_start {
                return Err(BallotError::InvalidWindow);
            }

            let earliest_end = voting_start
                .checked_add(MIN_VOTING_DURATION)
                .ok_or(BallotError::InvalidWindow)?;
            if voting_end < earliest_end || voting_start >= voting_end {
                return Err(BallotError::InvalidWindow);
            }

            admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::VotingActive, &true);
            env.storage()
                .instance()
                .set(&DataKey::VotingStart, &voting_start);
            env.storage()
                .instance()
                .set(&DataKey::VotingEnd, &voting_end);
            env.storage().instance().set(&DataKey::TotalVotes, &0i128);
            env.storage().instance().set(&DataKey::Quorum, &quorum);
            env.storage().instance().set(&DataKey::CommitPhase, &false);
            env.storage().instance().set(&DataKey::RevealPhase, &false);
            env.storage()
                .instance()
                .set(&DataKey::RankedVoteCount, &0u32);
            // Initialise the BitSet capacity counter to 0 words (#1500).
            env.storage()
                .instance()
                .set(&DataKey::WhitelistCapacity, &0u32);
            env.storage()
                .instance()
                .set(&DataKey::WhitelistNextSlot, &0u32);

            let choice_count = choices.len();
            env.storage().instance().set(&DataKey::Choices, &choices);
            for i in 0..choice_count {
                env.storage()
                    .instance()
                    .set(&DataKey::ChoiceVotes(i), &0i128);
            }

            // Backward-compat counters: slot 0 -> NoVotes, slot 1 -> YesVotes.
            env.storage().instance().set(&DataKey::YesVotes, &0i128);
            env.storage().instance().set(&DataKey::NoVotes, &0i128);

            bump(&env);
            events::initialized(&env, &admin);
            Ok(())
        }

        /// Admin registers a voter for the ballot.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        pub fn register_voter(env: Env, voter: Address) -> Result<(), BallotError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(BallotError::NotInitialized)?;
            admin.require_auth();

            let voter_key = DataKey::RegisteredVoter(voter.clone());
            env.storage().persistent().set(&voter_key, &true);
            env.storage().persistent().extend_ttl(
                &voter_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            bump(&env);
            events::voter_registered(&env, &voter);
            Ok(())
        }

        /// Admin deregisters a voter, correcting a registration mistake.
        ///
        /// Only allowed before any vote has been cast.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::VotingAlreadyStarted`] if at least one vote has been cast.
        /// - [`BallotError::NotRegistered`] if the voter is not currently registered.
        pub fn deregister_voter(env: Env, voter: Address) -> Result<(), BallotError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(BallotError::NotInitialized)?;
            admin.require_auth();

            let total_votes: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            if total_votes > 0 {
                return Err(BallotError::VotingAlreadyStarted);
            }

            let is_registered: bool = env
                .storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter.clone()))
                .unwrap_or(false);
            if !is_registered {
                return Err(BallotError::NotRegistered);
            }

            env.storage()
                .persistent()
                .remove(&DataKey::RegisteredVoter(voter.clone()));

            bump(&env);
            events::voter_deregistered(&env, &voter);
            Ok(())
        }

        /// Voter casts their vote.
        ///
        /// `choice` is a 0-based index into the `choices` list supplied at
        /// `initialize`.  For two-choice ballots the conventional mapping is
        /// `0 = no`, `1 = yes`.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::VotingNotStarted`] before the window opens.
        /// - [`BallotError::VotingClosed`] after the window closes.
        /// - [`BallotError::NotRegistered`] if the voter is not registered.
        /// - [`BallotError::AlreadyVoted`] if the voter has already voted.
        /// - [`BallotError::InvalidChoice`] if `choice` is out of range.
        pub fn vote(env: Env, voter: Address, choice: u32) -> Result<(), BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }

            let voting_active: bool = env
                .storage()
                .instance()
                .get(&DataKey::VotingActive)
                .unwrap_or(false);
            if !voting_active {
                return Err(BallotError::VotingClosed);
            }

            let voting_start: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingStart)
                .unwrap_or(0);
            let voting_end: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingEnd)
                .unwrap_or(0);
            let now = env.ledger().sequence();
            if now < voting_start {
                return Err(BallotError::VotingNotStarted);
            }
            if now > voting_end {
                return Err(BallotError::VotingClosed);
            }

            voter.require_auth();

            let is_registered: bool = env
                .storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter.clone()))
                .unwrap_or(false);
            if !is_registered {
                return Err(BallotError::NotRegistered);
            }

            let voted_key = DataKey::HasVoted(voter.clone());
            let has_voted: bool = env.storage().persistent().get(&voted_key).unwrap_or(false);
            if has_voted {
                return Err(BallotError::AlreadyVoted);
            }

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;
            if choice >= choices.len() {
                return Err(BallotError::InvalidChoice);
            }

            Self::record_vote(&env, choice);

            env.storage().persistent().set(&voted_key, &true);
            env.storage().persistent().extend_ttl(
                &voted_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            bump(&env);
            events::voted(&env, &voter, choice);
            Ok(())
        }

        // ── Issue #1500: voter eligibility whitelist / BitSet ───────────────

        /// Admin adds a voter address to the compact BitSet whitelist (#1500).
        ///
        /// Each address is assigned a sequential slot index on first
        /// whitelisting.  The slot determines which 128-bit storage word
        /// holds the voter's bit (`word = slot / 128`, `bit = slot % 128`).
        /// Re-whitelisting an already-whitelisted address is a no-op.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        pub fn whitelist_voter(env: Env, voter: Address) -> Result<(), BallotError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(BallotError::NotInitialized)?;
            admin.require_auth();

            // Assign a slot if this address hasn't been seen before.
            let slot_key = DataKey::WhitelistSlot(voter.clone());
            let slot: u32 = if let Some(s) = env.storage().persistent().get(&slot_key) {
                s
            } else {
                let next: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::WhitelistNextSlot)
                    .unwrap_or(0);
                env.storage()
                    .instance()
                    .set(&DataKey::WhitelistNextSlot, &(next + 1));
                // Grow the capacity counter if we've entered a new word.
                let needed_words = (next / BITSET_WORD_BITS) + 1;
                let current_cap: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::WhitelistCapacity)
                    .unwrap_or(0);
                if needed_words > current_cap {
                    env.storage()
                        .instance()
                        .set(&DataKey::WhitelistCapacity, &needed_words);
                }
                env.storage().persistent().set(&slot_key, &next);
                env.storage().persistent().extend_ttl(
                    &slot_key,
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
                next
            };

            // Set the bit in the appropriate word.
            let word_idx = slot / BITSET_WORD_BITS;
            let bit_pos = slot % BITSET_WORD_BITS;
            let word_key = DataKey::WhitelistWord(word_idx);
            let word: u128 = env.storage().instance().get(&word_key).unwrap_or(0u128);
            let updated = word | (1u128 << bit_pos);
            env.storage().instance().set(&word_key, &updated);

            bump(&env);
            events::voter_whitelisted(&env, &voter, slot);
            Ok(())
        }

        /// Admin removes a voter address from the compact BitSet whitelist (#1500).
        ///
        /// Clears the voter's bit.  The slot index is retained so that future
        /// re-whitelisting reuses the same bit position.  The voter is not
        /// deregistered.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        /// - [`BallotError::NotWhitelisted`] if the voter is not currently whitelisted.
        pub fn unwhitelist_voter(env: Env, voter: Address) -> Result<(), BallotError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(BallotError::NotInitialized)?;
            admin.require_auth();

            if !Self::check_whitelisted(&env, &voter) {
                return Err(BallotError::NotWhitelisted);
            }

            let slot: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::WhitelistSlot(voter.clone()))
                .unwrap_or_else(|| panic!("slot missing for whitelisted voter"));

            let word_idx = slot / BITSET_WORD_BITS;
            let bit_pos = slot % BITSET_WORD_BITS;
            let word_key = DataKey::WhitelistWord(word_idx);
            let word: u128 = env.storage().instance().get(&word_key).unwrap_or(0u128);
            let updated = word & !(1u128 << bit_pos);
            env.storage().instance().set(&word_key, &updated);

            bump(&env);
            events::voter_unwhitelisted(&env, &voter);
            Ok(())
        }

        /// Returns `true` if `voter` is currently set in the eligibility
        /// whitelist BitSet (#1500).
        pub fn is_whitelisted(env: Env, voter: Address) -> bool {
            Self::check_whitelisted(&env, &voter)
        }

        /// Vote that requires the voter to be on the eligibility whitelist
        /// in addition to being registered (#1500).
        ///
        /// All other voting rules (window, single-vote, choice range) apply
        /// identically to [`BallotContract::vote`].
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::VotingNotStarted`] / [`BallotError::VotingClosed`]
        /// - [`BallotError::NotRegistered`] if the voter is not registered.
        /// - [`BallotError::NotWhitelisted`] if the voter is not in the BitSet.
        /// - [`BallotError::AlreadyVoted`]
        /// - [`BallotError::InvalidChoice`]
        pub fn vote_whitelisted(env: Env, voter: Address, choice: u32) -> Result<(), BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }

            let voting_active: bool = env
                .storage()
                .instance()
                .get(&DataKey::VotingActive)
                .unwrap_or(false);
            if !voting_active {
                return Err(BallotError::VotingClosed);
            }

            let voting_start: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingStart)
                .unwrap_or(0);
            let voting_end: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingEnd)
                .unwrap_or(0);
            let now = env.ledger().sequence();
            if now < voting_start {
                return Err(BallotError::VotingNotStarted);
            }
            if now > voting_end {
                return Err(BallotError::VotingClosed);
            }

            voter.require_auth();

            let is_registered: bool = env
                .storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter.clone()))
                .unwrap_or(false);
            if !is_registered {
                return Err(BallotError::NotRegistered);
            }

            if !Self::check_whitelisted(&env, &voter) {
                return Err(BallotError::NotWhitelisted);
            }

            let voted_key = DataKey::HasVoted(voter.clone());
            let has_voted: bool = env.storage().persistent().get(&voted_key).unwrap_or(false);
            if has_voted {
                return Err(BallotError::AlreadyVoted);
            }

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;
            if choice >= choices.len() {
                return Err(BallotError::InvalidChoice);
            }

            Self::record_vote(&env, choice);

            env.storage().persistent().set(&voted_key, &true);
            env.storage().persistent().extend_ttl(
                &voted_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            bump(&env);
            events::voted(&env, &voter, choice);
            Ok(())
        }

        // ── Issue #1498: quadratic voting ───────────────────────────────────

        /// Cast a quadratic-weighted vote (#1498).
        ///
        /// The voter provides a `tokens` value (e.g. their token balance).
        /// The vote weight applied to the tally is `floor(sqrt(tokens))`.
        /// This compresses the influence of large token holders: someone with
        /// 100 tokens contributes weight 10, not 100.
        ///
        /// All other voting rules (window, registration, single-vote, choice
        /// range) apply identically to [`BallotContract::vote`].
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::VotingNotStarted`] / [`BallotError::VotingClosed`]
        /// - [`BallotError::NotRegistered`]
        /// - [`BallotError::AlreadyVoted`]
        /// - [`BallotError::InvalidChoice`]
        /// - [`BallotError::ZeroTokenBalance`] if `tokens` is zero.
        pub fn vote_quadratic(
            env: Env,
            voter: Address,
            choice: u32,
            tokens: u64,
        ) -> Result<(), BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }

            if tokens == 0 {
                return Err(BallotError::ZeroTokenBalance);
            }

            let voting_active: bool = env
                .storage()
                .instance()
                .get(&DataKey::VotingActive)
                .unwrap_or(false);
            if !voting_active {
                return Err(BallotError::VotingClosed);
            }

            let voting_start: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingStart)
                .unwrap_or(0);
            let voting_end: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingEnd)
                .unwrap_or(0);
            let now = env.ledger().sequence();
            if now < voting_start {
                return Err(BallotError::VotingNotStarted);
            }
            if now > voting_end {
                return Err(BallotError::VotingClosed);
            }

            voter.require_auth();

            let is_registered: bool = env
                .storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter.clone()))
                .unwrap_or(false);
            if !is_registered {
                return Err(BallotError::NotRegistered);
            }

            let voted_key = DataKey::HasVoted(voter.clone());
            let has_voted: bool = env.storage().persistent().get(&voted_key).unwrap_or(false);
            if has_voted {
                return Err(BallotError::AlreadyVoted);
            }

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;
            if choice >= choices.len() {
                return Err(BallotError::InvalidChoice);
            }

            let weight = isqrt_u64(tokens);
            Self::record_vote_weighted(&env, choice, i128::from(weight));

            env.storage().persistent().set(&voted_key, &true);
            env.storage().persistent().extend_ttl(
                &voted_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            bump(&env);
            events::voted_quadratic(&env, &voter, choice, weight);
            Ok(())
        }

        /// Returns the quadratic weight for a given token count (#1498).
        ///
        /// This is `floor(sqrt(tokens))`.  Useful for clients that want to
        /// preview the weight before casting a vote.
        pub fn quadratic_weight(tokens: u64) -> u64 {
            isqrt_u64(tokens)
        }

        // ── Issue #1499: secret ballot commit / reveal ──────────────────────

        /// Open the commit phase of the secret ballot (#1499).
        ///
        /// During the commit phase voters call [`BallotContract::commit_vote`]
        /// to submit `hash(voter ++ choice ++ salt)` without revealing their
        /// choice.  No choice counts are visible at this stage.
        pub fn start_commit_phase(env: Env) {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .unwrap_or_else(|| panic!("not initialized"));
            admin.require_auth();
            env.storage().instance().set(&DataKey::CommitPhase, &true);
            env.storage().instance().set(&DataKey::RevealPhase, &false);
            env.storage().instance().set(&DataKey::VotingActive, &true);
        }

        /// Close the commit phase and open the reveal phase (#1499).
        ///
        /// During the reveal phase voters call [`BallotContract::reveal_vote`]
        /// with their `(choice, salt)` preimage.  The contract verifies the
        /// hash and tallies the vote.
        pub fn start_reveal_phase(env: Env) {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .unwrap_or_else(|| panic!("not initialized"));
            admin.require_auth();
            env.storage().instance().set(&DataKey::CommitPhase, &false);
            env.storage().instance().set(&DataKey::RevealPhase, &true);
        }

        /// Phase 1 (secret ballot): commit `hash(voter ++ choice ++ salt)` (#1499).
        ///
        /// The voter submits a commitment during the commit window.  No choice
        /// information is recorded — only the opaque hash — so the ballot
        /// remains secret until the reveal phase.
        pub fn commit_vote(env: Env, voter: Address, commitment: BytesN<32>) {
            voter.require_auth();
            Self::require_registered(&env, &voter);
            Self::require_commit_phase(&env);
            let key = DataKey::Commitment(voter.clone());
            if env.storage().persistent().has(&key) {
                panic!("voter has already committed a ballot");
            }
            env.storage().persistent().set(&key, &commitment);
            events::vote_committed(&env, &voter, &commitment);
        }

        /// Phase 2 (secret ballot): reveal `(choice, salt)` and tally (#1499).
        ///
        /// The voter submits the preimage `(choice, salt)`.  The contract
        /// computes `sha256(voter_bytes ++ choice_be4 ++ salt)` and checks it
        /// against the stored commitment.  On success the choice is tallied.
        /// On mismatch the call panics, so the commitment is not consumed.
        pub fn reveal_vote(env: Env, voter: Address, choice: u32, salt: Bytes) {
            voter.require_auth();
            Self::require_registered(&env, &voter);
            Self::require_reveal_phase(&env);

            let key = DataKey::Commitment(voter.clone());
            let commitment: BytesN<32> = env
                .storage()
                .persistent()
                .get(&key)
                .unwrap_or_else(|| panic!("no commitment found for voter"));

            let mut preimage = Bytes::new(&env);
            preimage.append(&voter.clone().to_string().to_bytes());
            preimage.extend_from_array(&choice.to_be_bytes());
            preimage.append(&salt);
            let computed = env.crypto().sha256(&preimage);
            if computed.to_bytes() != commitment {
                panic!("revealed ballot does not match commitment");
            }

            env.storage().persistent().remove(&key);
            env.storage()
                .persistent()
                .set(&DataKey::Revealed(voter.clone()), &true);

            Self::record_vote(&env, choice);
            events::vote_revealed(&env, &voter, choice);
        }

        /// Phase 1 (blinded): commit `hash(secret ++ choice ++ proof)` (#1331).
        ///
        /// The commitment is not bound to the voter's address, so a relayer can
        /// later reveal it without linking the choice to the voter on-chain.
        /// The voter must still be registered and authorize the commit.
        pub fn commit_vote_blinded(env: Env, voter: Address, commitment: BytesN<32>) {
            voter.require_auth();
            Self::require_registered(&env, &voter);
            Self::require_commit_phase(&env);

            let voter_key = DataKey::Commitment(voter.clone());
            if env.storage().persistent().has(&voter_key) {
                panic!("voter has already committed a ballot");
            }

            let hash_key = DataKey::CommitmentHash(commitment.clone());
            if env.storage().persistent().has(&hash_key) {
                panic!("commitment hash already registered");
            }

            env.storage().persistent().set(&voter_key, &commitment);
            env.storage().persistent().set(&hash_key, &true);

            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::CommitCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::CommitCount, &(count + 1));

            events::blinded_committed(&env, &commitment);
        }

        /// Phase 2 (blinded): permissionless reveal for any valid preimage (#1331).
        ///
        /// A relayer submits `(secret, choice, proof)` for a commitment created
        /// by [`BallotContract::commit_vote_blinded`].  The contract verifies
        /// `hash(secret ++ choice ++ proof) == commitment`, records the vote,
        /// and emits an event containing only the commitment hash and choice.
        /// The caller does not need to be registered and no voter address is
        /// associated with the tally.
        pub fn reveal_vote_blinded(
            env: Env,
            commitment: BytesN<32>,
            secret: Bytes,
            choice: u32,
            proof: Bytes,
        ) {
            Self::require_reveal_phase(&env);

            let hash_key = DataKey::CommitmentHash(commitment.clone());
            if !env.storage().persistent().has(&hash_key) {
                panic!("no blinded commitment for hash");
            }

            let revealed_key = DataKey::RevealedCommitment(commitment.clone());
            if env.storage().persistent().has(&revealed_key) {
                panic!("commitment already revealed");
            }

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .unwrap_or_else(|| panic!("not initialized"));
            if choice >= choices.len() {
                panic!("invalid vote choice");
            }

            let mut preimage = Bytes::new(&env);
            preimage.append(&secret);
            preimage.extend_from_array(&choice.to_be_bytes());
            preimage.append(&proof);
            let computed = env.crypto().sha256(&preimage);
            if computed.to_bytes() != commitment {
                panic!("revealed ballot does not match commitment");
            }

            env.storage().persistent().remove(&hash_key);
            env.storage().persistent().set(&revealed_key, &true);

            Self::record_vote(&env, choice);
            events::vote_revealed_blinded(&env, &commitment, choice);
        }

        /// Submit a ranked-choice ballot.
        ///
        /// `preferences` is an ordered list of choice indices, most preferred
        /// first.  Each index must be unique and within the range of configured
        /// choices.  A voter may only submit one ranked ballot.
        pub fn vote_ranked(env: Env, voter: Address, preferences: Vec<u32>) {
            voter.require_auth();
            Self::require_registered(&env, &voter);
            Self::require_voting_active(&env);

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .unwrap_or_else(|| Vec::new(&env));
            let num_choices = choices.len();

            if preferences.is_empty() {
                panic!("ranked ballot must contain at least one preference");
            }

            let mut seen: Vec<u32> = Vec::new(&env);
            for pref in preferences.iter() {
                if pref >= num_choices {
                    panic!("ranked choice index out of range");
                }
                if seen.contains(pref) {
                    panic!("ranked ballot contains duplicate choice index");
                }
                seen.push_back(pref);
            }

            let vote_key = DataKey::RankedVote(voter.clone());
            if env.storage().persistent().has(&vote_key) {
                panic!("voter has already submitted a ranked ballot");
            }
            env.storage().persistent().set(&vote_key, &preferences);

            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RankedVoteCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::RankedVoteCount, &(count + 1));

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + 1));

            if let Some(first) = preferences.get(0) {
                events::voted(&env, &voter, first);
            }
        }

        /// Run instant-runoff elimination and return round-by-round results.
        ///
        /// Ballots are stored per voter and the contract does not persist an
        /// enumerable voter list, so no rounds can be computed on-chain without
        /// an explicit ballot feed.  The entry point is retained for API
        /// compatibility and returns an empty result set.
        pub fn tally_ranked(env: Env) -> Vec<RoundResult> {
            let _ = env;
            Vec::new(&env)
        }

        /// Read-only tally query.  Never requires auth and does not close the
        /// ballot.
        ///
        /// Returns per-choice vote counts in declaration order.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        pub fn get_tally(env: Env) -> Result<Vec<i128>, BallotError> {
            Self::read_counts(&env)
        }

        /// Tally all choices and close voting.
        ///
        /// Returns [`BallotResult::QuorumNotMet`] if the number of votes cast
        /// is below the quorum configured at `initialize`; otherwise returns
        /// [`BallotResult::Certified`] with the per-choice counts in declaration
        /// order.
        ///
        /// Once `voting_end` has passed this is permissionless: any caller may
        /// close the ballot.  Before the deadline the admin must authorize it.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if called before `voting_end` by a
        ///   non-admin.
        pub fn tally_all(env: Env) -> Result<BallotResult, BallotError> {
            Self::require_closing_auth(&env)?;

            let results = Self::read_counts(&env)?;
            let total_votes: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            let quorum: u32 = env.storage().instance().get(&DataKey::Quorum).unwrap_or(0);

            env.storage().instance().set(&DataKey::VotingActive, &false);
            bump(&env);
            events::tally_completed(&env, &results);

            if total_votes < quorum as i128 {
                return Ok(BallotResult::QuorumNotMet);
            }

            Ok(BallotResult::Certified(results))
        }

        /// Backward-compatible two-choice tally helper.
        ///
        /// Returns `(choice[1] votes, choice[0] votes)` i.e. `(yes, no)` and
        /// closes the ballot.  Permissionless once `voting_end` has passed.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if called before `voting_end` by a
        ///   non-admin.
        pub fn tally(env: Env) -> Result<(i128, i128), BallotError> {
            Self::require_closing_auth(&env)?;

            let yes: i128 = env
                .storage()
                .instance()
                .get(&DataKey::YesVotes)
                .unwrap_or(0i128);
            let no: i128 = env
                .storage()
                .instance()
                .get(&DataKey::NoVotes)
                .unwrap_or(0i128);

            env.storage().instance().set(&DataKey::VotingActive, &false);
            bump(&env);
            events::tally_result(&env, yes, no);
            Ok((yes, no))
        }

        /// Yes-vote counter (binary ballots only).
        pub fn get_yes_votes(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::YesVotes)
                .unwrap_or(0i128)
        }

        /// No-vote counter (binary ballots only).
        pub fn get_no_votes(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::NoVotes)
                .unwrap_or(0i128)
        }

        /// Total number of votes cast.
        pub fn total_votes(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128)
        }

        // ── Internal helpers ────────────────────────────────────────────────

        fn read_counts(env: &Env) -> Result<Vec<i128>, BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }
            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;

            let mut results: Vec<i128> = Vec::new(env);
            for i in 0..choices.len() {
                let count: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ChoiceVotes(i))
                    .unwrap_or(0i128);
                results.push_back(count);
            }
            Ok(results)
        }

        fn record_vote(env: &Env, choice: u32) {
            Self::record_vote_weighted(env, choice, 1i128);
        }

        fn record_vote_weighted(env: &Env, choice: u32, amount: i128) {
            let choice_key = DataKey::ChoiceVotes(choice);
            let current: i128 = env.storage().instance().get(&choice_key).unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&choice_key, &(current + amount));

            if choice == 0 {
                let no: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::NoVotes)
                    .unwrap_or(0i128);
                env.storage()
                    .instance()
                    .set(&DataKey::NoVotes, &(no + amount));
            } else if choice == 1 {
                let yes: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::YesVotes)
                    .unwrap_or(0i128);
                env.storage()
                    .instance()
                    .set(&DataKey::YesVotes, &(yes + amount));
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + amount));
        }

        fn require_closing_auth(env: &Env) -> Result<(), BallotError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(BallotError::NotInitialized)?;
            let voting_end: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingEnd)
                .unwrap_or(0);

            if env.ledger().sequence() <= voting_end {
                admin.require_auth();
            }
            Ok(())
        }

        fn require_registered(env: &Env, voter: &Address) {
            let registered: bool = env
                .storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter.clone()))
                .unwrap_or(false);
            if !registered {
                panic!("voter is not registered");
            }
        }

        fn require_voting_active(env: &Env) {
            let active: bool = env
                .storage()
                .instance()
                .get(&DataKey::VotingActive)
                .unwrap_or(false);
            if !active {
                panic!("voting is not active");
            }
        }

        fn require_commit_phase(env: &Env) {
            let active: bool = env
                .storage()
                .instance()
                .get(&DataKey::CommitPhase)
                .unwrap_or(false);
            if !active {
                panic!("commit phase is not active");
            }
        }

        fn require_reveal_phase(env: &Env) {
            let active: bool = env
                .storage()
                .instance()
                .get(&DataKey::RevealPhase)
                .unwrap_or(false);
            if !active {
                panic!("reveal phase is not active");
            }
        }

        /// Read the whitelist bit for `voter` from the compact BitSet (#1500).
        fn check_whitelisted(env: &Env, voter: &Address) -> bool {
            let slot_key = DataKey::WhitelistSlot(voter.clone());
            let slot: u32 = match env.storage().persistent().get(&slot_key) {
                Some(s) => s,
                None => return false,
            };
            let word_idx = slot / BITSET_WORD_BITS;
            let bit_pos = slot % BITSET_WORD_BITS;
            let word_key = DataKey::WhitelistWord(word_idx);
            let word: u128 = env.storage().instance().get(&word_key).unwrap_or(0u128);
            (word >> bit_pos) & 1 == 1
        }
    }
}

/// Integer square root (floor) for quadratic voting weight (#1498).
///
/// Returns the largest integer `r` such that `r * r <= n`.
/// Uses Newton's method with u128 arithmetic to avoid overflow on large inputs.
fn isqrt_u64(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    // Start from the ceiling of sqrt(n) using the bit-length heuristic.
    let mut x = n as u128;
    let mut y = (x + 1) / 2;
    let n128 = n as u128;
    while y < x {
        x = y;
        y = (x + n128 / x) / 2;
    }
    x as u64
}

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Env, String, Vec,
    };

    const START_LEDGER: u32 = 10;
    const VOTING_START: u32 = 100;
    const VOTING_END: u32 = 200;

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.sequence_number = START_LEDGER;
        });
        env
    }

    fn setup(env: &Env, quorum: u32) -> (Address, BallotContractClient<'_>) {
        let admin = Address::generate(env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(env, &contract_id);
        let mut choices: Vec<String> = Vec::new(env);
        choices.push_back(String::from_str(env, "no"));
        choices.push_back(String::from_str(env, "yes"));
        let result = client.try_initialize(&admin, &VOTING_START, &VOTING_END, &choices, &quorum);
        assert!(result.is_ok());
        (admin, client)
    }

    // -----------------------------------------------------------------------
    // Existing tests (quorum, blinded reveal, voting window)
    // -----------------------------------------------------------------------

    #[test]
    fn tally_certifies_when_quorum_met() {
        let env = make_env();
        let (_admin, client) = setup(&env, 2);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        client.register_voter(&v1);
        client.register_voter(&v2);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&v1, &1u32);
        client.vote(&v2, &0u32);

        match client.tally_all() {
            BallotResult::Certified(counts) => {
                assert_eq!(counts.get(0).unwrap(), 1);
                assert_eq!(counts.get(1).unwrap(), 1);
            }
            BallotResult::QuorumNotMet => panic!("expected quorum to be met"),
        }
    }

    #[test]
    fn tally_fails_when_quorum_not_met() {
        let env = make_env();
        let (_admin, client) = setup(&env, 3);
        let v1 = Address::generate(&env);
        client.register_voter(&v1);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&v1, &1u32);

        assert_eq!(client.tally_all(), BallotResult::QuorumNotMet);
    }

    // -----------------------------------------------------------------------
    // Issue #1331 — anonymous blinded reveal
    // -----------------------------------------------------------------------

    fn blinded_commitment(env: &Env, secret: &Bytes, choice: u32, proof: &Bytes) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(secret);
        preimage.extend_from_array(&choice.to_be_bytes());
        preimage.append(proof);
        env.crypto().sha256(&preimage).to_bytes()
    }

    #[test]
    fn anonymous_relayer_can_reveal_blinded_commitment() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);
        let choice = 1u32;

        client.register_voter(&voter);
        client.start_commit_phase();

        let commitment = blinded_commitment(&env, &secret, choice, &proof);
        client.commit_vote_blinded(&voter, &commitment);
        client.start_reveal_phase();

        client.reveal_vote_blinded(&commitment, &secret, &choice, &proof);

        assert_eq!(client.get_yes_votes(), 1);
        assert_eq!(client.total_votes(), 1);
        let counts = client.get_tally();
        assert_eq!(counts.get(0).unwrap(), 0);
        assert_eq!(counts.get(1).unwrap(), 1);
    }

    #[test]
    fn anonymous_reveal_rejects_invalid_preimage() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let wrong_secret = Bytes::from_array(&env, &[9u8, 9, 9, 9]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);
        let choice = 1u32;

        client.register_voter(&voter);
        client.start_commit_phase();
        let commitment = blinded_commitment(&env, &secret, choice, &proof);
        client.commit_vote_blinded(&voter, &commitment);
        client.start_reveal_phase();

        let result = client.try_reveal_vote_blinded(&commitment, &wrong_secret, &choice, &proof);
        assert!(result.is_err());
    }

    #[test]
    fn anonymous_reveal_cannot_be_replayed() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);
        let choice = 1u32;

        client.register_voter(&voter);
        client.start_commit_phase();
        let commitment = blinded_commitment(&env, &secret, choice, &proof);
        client.commit_vote_blinded(&voter, &commitment);
        client.start_reveal_phase();

        client.reveal_vote_blinded(&commitment, &secret, &choice, &proof);
        let result = client.try_reveal_vote_blinded(&commitment, &secret, &choice, &proof);
        assert!(result.is_err());
    }

    #[test]
    fn anonymous_reveal_rejected_outside_reveal_phase() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);
        let choice = 1u32;

        client.register_voter(&voter);
        client.start_commit_phase();
        let commitment = blinded_commitment(&env, &secret, choice, &proof);
        client.commit_vote_blinded(&voter, &commitment);

        let result = client.try_reveal_vote_blinded(&commitment, &secret, &choice, &proof);
        assert!(result.is_err());
    }

    #[test]
    fn anonymous_reveal_rejects_unknown_commitment() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);
        let choice = 1u32;

        client.start_commit_phase();
        client.start_reveal_phase();

        let commitment = blinded_commitment(&env, &secret, choice, &proof);
        let result = client.try_reveal_vote_blinded(&commitment, &secret, &choice, &proof);
        assert!(result.is_err());
    }

    #[test]
    fn anonymous_commit_requires_registered_voter() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let outsider = Address::generate(&env);
        let secret = Bytes::from_array(&env, &[1u8, 2, 3, 4]);
        let proof = Bytes::from_array(&env, &[5u8, 6, 7, 8]);

        client.start_commit_phase();
        let commitment = blinded_commitment(&env, &secret, 1u32, &proof);
        let result = client.try_commit_vote_blinded(&outsider, &commitment);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Issue #1328 — enforce a future voting window at initialization
    // -----------------------------------------------------------------------

    #[test]
    fn initialize_rejects_past_voting_start() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        env.ledger().with_mut(|le| le.sequence_number = 500);
        let result = client.try_initialize(&admin, &400u32, &600u32, &choices, &0u32);
        assert!(matches!(result, Err(Ok(BallotError::InvalidWindow))));
    }

    #[test]
    fn initialize_rejects_start_inside_buffer() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        let now = 500u32;
        env.ledger().with_mut(|le| le.sequence_number = now);
        let start = now + MIN_DEADLINE_BUFFER - 1;
        let end = start + MIN_VOTING_DURATION;
        let result = client.try_initialize(&admin, &start, &end, &choices, &0u32);
        assert!(matches!(result, Err(Ok(BallotError::InvalidWindow))));
    }

    #[test]
    fn initialize_accepts_start_at_buffer_boundary() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        let now = 500u32;
        env.ledger().with_mut(|le| le.sequence_number = now);
        let start = now + MIN_DEADLINE_BUFFER;
        let end = start + MIN_VOTING_DURATION;
        let result = client.try_initialize(&admin, &start, &end, &choices, &0u32);
        assert!(result.is_ok());
    }

    #[test]
    fn initialize_rejects_short_voting_duration() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        let now = 500u32;
        env.ledger().with_mut(|le| le.sequence_number = now);
        let start = now + MIN_DEADLINE_BUFFER;
        let end = start + MIN_VOTING_DURATION - 1;
        let result = client.try_initialize(&admin, &start, &end, &choices, &0u32);
        assert!(matches!(result, Err(Ok(BallotError::InvalidWindow))));
    }

    #[test]
    fn initialize_accepts_minimum_voting_duration() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        let now = 500u32;
        env.ledger().with_mut(|le| le.sequence_number = now);
        let start = now + MIN_DEADLINE_BUFFER;
        let end = start + MIN_VOTING_DURATION;
        let result = client.try_initialize(&admin, &start, &end, &choices, &0u32);
        assert!(result.is_ok());
    }

    #[test]
    fn initialize_rejects_non_increasing_window() {
        let env = make_env();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices = Vec::<String>::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));

        let now = 500u32;
        env.ledger().with_mut(|le| le.sequence_number = now);
        let start = now + MIN_DEADLINE_BUFFER;
        let result = client.try_initialize(&admin, &start, &start, &choices, &0u32);
        assert!(matches!(result, Err(Ok(BallotError::InvalidWindow))));
    }

    // -----------------------------------------------------------------------
    // Issue #1498 — quadratic voting
    // -----------------------------------------------------------------------

    /// `isqrt_u64` returns the correct floor square root.
    #[test]
    fn isqrt_correctness() {
        assert_eq!(isqrt_u64(0), 0);
        assert_eq!(isqrt_u64(1), 1);
        assert_eq!(isqrt_u64(3), 1);
        assert_eq!(isqrt_u64(4), 2);
        assert_eq!(isqrt_u64(99), 9);
        assert_eq!(isqrt_u64(100), 10);
        assert_eq!(isqrt_u64(101), 10);
        assert_eq!(isqrt_u64(400), 20);
        assert_eq!(isqrt_u64(u64::MAX), 4_294_967_295u64);
    }

    /// A voter with 100 tokens contributes weight 10 (not 100).
    #[test]
    fn quadratic_vote_weight_is_sqrt_of_tokens() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        // 100 tokens → weight 10
        client.vote_quadratic(&voter, &1u32, &100u64);

        assert_eq!(client.get_yes_votes(), 10);
        assert_eq!(client.total_votes(), 10);
    }

    /// Two voters with different token balances: linear comparison vs quadratic.
    ///
    /// Linear: voter_a (400 tokens) = 4× voter_b (100 tokens).
    /// Quadratic: voter_a (sqrt(400)=20) = 2× voter_b (sqrt(100)=10).
    #[test]
    fn quadratic_balances_minority_influence() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter_a = Address::generate(&env);
        let voter_b = Address::generate(&env);
        client.register_voter(&voter_a);
        client.register_voter(&voter_b);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        // voter_a: 400 tokens → weight 20 (choice yes = index 1)
        client.vote_quadratic(&voter_a, &1u32, &400u64);
        // voter_b: 100 tokens → weight 10 (choice no = index 0)
        client.vote_quadratic(&voter_b, &0u32, &100u64);

        // Linear ratio would be 400:100 = 4:1; quadratic is 20:10 = 2:1.
        assert_eq!(client.get_yes_votes(), 20);
        assert_eq!(client.get_no_votes(), 10);
        assert_eq!(client.total_votes(), 30);
    }

    /// Zero tokens is rejected with `ZeroTokenBalance`.
    #[test]
    fn quadratic_vote_rejects_zero_tokens() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_vote_quadratic(&voter, &1u32, &0u64);
        assert!(matches!(result, Err(Ok(BallotError::ZeroTokenBalance))));
    }

    /// Unregistered voter cannot cast a quadratic vote.
    #[test]
    fn quadratic_vote_requires_registration() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let outsider = Address::generate(&env);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_vote_quadratic(&outsider, &1u32, &100u64);
        assert!(matches!(result, Err(Ok(BallotError::NotRegistered))));
    }

    /// A voter cannot cast two quadratic votes.
    #[test]
    fn quadratic_vote_prevents_double_voting() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        client.vote_quadratic(&voter, &1u32, &100u64);
        let result = client.try_vote_quadratic(&voter, &0u32, &100u64);
        assert!(matches!(result, Err(Ok(BallotError::AlreadyVoted))));
    }

    // -----------------------------------------------------------------------
    // Issue #1499 — secret ballot commit / reveal
    // -----------------------------------------------------------------------

    fn commit_hash(env: &Env, voter: &Address, choice: u32, salt: &Bytes) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&voter.clone().to_string().to_bytes());
        preimage.extend_from_array(&choice.to_be_bytes());
        preimage.append(salt);
        env.crypto().sha256(&preimage).to_bytes()
    }

    /// Votes are hidden during the commit phase: no choice counts are visible.
    ///
    /// This directly verifies the "vote secrecy before tally phase" acceptance
    /// criterion from issue #1499.
    #[test]
    fn commit_phase_conceals_vote_choices() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);

        client.start_commit_phase();

        let salt = Bytes::from_array(&env, &[0xdeu8, 0xad, 0xbe, 0xef]);
        let choice = 1u32; // "yes"
        let commitment = commit_hash(&env, &voter, choice, &salt);
        client.commit_vote(&voter, &commitment);

        // During the commit phase, no counts are recorded yet.
        let counts = client.get_tally();
        assert_eq!(counts.get(0).unwrap(), 0, "no votes should be 0 during commit phase");
        assert_eq!(counts.get(1).unwrap(), 0, "yes votes should be 0 during commit phase");
        assert_eq!(client.total_votes(), 0, "total votes should be 0 during commit phase");
    }

    /// After the reveal phase, the commitment is verified and vote tallied.
    #[test]
    fn reveal_phase_tallies_vote_after_hash_verification() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);

        client.start_commit_phase();
        let salt = Bytes::from_array(&env, &[0xdeu8, 0xad, 0xbe, 0xef]);
        let choice = 1u32;
        let commitment = commit_hash(&env, &voter, choice, &salt);
        client.commit_vote(&voter, &commitment);

        client.start_reveal_phase();
        client.reveal_vote(&voter, &choice, &salt);

        assert_eq!(client.get_yes_votes(), 1);
        assert_eq!(client.total_votes(), 1);
    }

    /// A wrong salt causes the reveal to be rejected.
    #[test]
    fn reveal_rejects_mismatched_commitment_hash() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);

        client.start_commit_phase();
        let salt = Bytes::from_array(&env, &[0x01u8, 0x02, 0x03, 0x04]);
        let wrong_salt = Bytes::from_array(&env, &[0xffu8, 0xff, 0xff, 0xff]);
        let choice = 1u32;
        let commitment = commit_hash(&env, &voter, choice, &salt);
        client.commit_vote(&voter, &commitment);

        client.start_reveal_phase();
        let result = client.try_reveal_vote(&voter, &choice, &wrong_salt);
        assert!(result.is_err(), "mismatched salt must be rejected");
    }

    /// A voter without a commitment cannot reveal.
    #[test]
    fn reveal_requires_prior_commitment() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);

        client.start_commit_phase();
        client.start_reveal_phase();
        let salt = Bytes::from_array(&env, &[0x01u8, 0x02, 0x03, 0x04]);
        let result = client.try_reveal_vote(&voter, &1u32, &salt);
        assert!(result.is_err(), "reveal without prior commitment must fail");
    }

    // -----------------------------------------------------------------------
    // Issue #1500 — voter eligibility whitelist / compact BitSet
    // -----------------------------------------------------------------------

    /// A newly whitelisted voter is visible via `is_whitelisted`.
    #[test]
    fn whitelist_voter_sets_bit() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        assert!(!client.is_whitelisted(&voter));
        client.whitelist_voter(&voter);
        assert!(client.is_whitelisted(&voter));
    }

    /// Unwhitelisting clears the bit.
    #[test]
    fn unwhitelist_voter_clears_bit() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        client.whitelist_voter(&voter);
        assert!(client.is_whitelisted(&voter));

        client.unwhitelist_voter(&voter);
        assert!(!client.is_whitelisted(&voter));
    }

    /// Attempting to unwhitelist a voter not in the BitSet returns an error.
    #[test]
    fn unwhitelist_non_whitelisted_voter_errors() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        let result = client.try_unwhitelist_voter(&voter);
        assert!(matches!(result, Err(Ok(BallotError::NotWhitelisted))));
    }

    /// Multiple voters receive distinct bit slots.
    #[test]
    fn multiple_voters_get_distinct_slots() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);

        client.whitelist_voter(&v1);
        client.whitelist_voter(&v2);
        client.whitelist_voter(&v3);

        assert!(client.is_whitelisted(&v1));
        assert!(client.is_whitelisted(&v2));
        assert!(client.is_whitelisted(&v3));

        client.unwhitelist_voter(&v2);

        assert!(client.is_whitelisted(&v1));
        assert!(!client.is_whitelisted(&v2));
        assert!(client.is_whitelisted(&v3));
    }

    /// `vote_whitelisted` succeeds for a registered + whitelisted voter.
    #[test]
    fn vote_whitelisted_succeeds_for_eligible_voter() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        client.whitelist_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        client.vote_whitelisted(&voter, &1u32);
        assert_eq!(client.total_votes(), 1);
        assert_eq!(client.get_yes_votes(), 1);
    }

    /// `vote_whitelisted` fails for a registered but not whitelisted voter.
    #[test]
    fn vote_whitelisted_rejects_non_whitelisted_voter() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        // voter is registered but NOT whitelisted
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_vote_whitelisted(&voter, &1u32);
        assert!(matches!(result, Err(Ok(BallotError::NotWhitelisted))));
    }

    /// `vote_whitelisted` fails for an unregistered voter even if whitelisted.
    #[test]
    fn vote_whitelisted_rejects_unregistered_voter() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.whitelist_voter(&voter);
        // voter is whitelisted but NOT registered
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_vote_whitelisted(&voter, &1u32);
        assert!(matches!(result, Err(Ok(BallotError::NotRegistered))));
    }

    /// Removing a voter from the whitelist after registration blocks their vote.
    #[test]
    fn unwhitelisting_voter_blocks_whitelisted_vote() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);
        client.register_voter(&voter);
        client.whitelist_voter(&voter);
        client.unwhitelist_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_vote_whitelisted(&voter, &1u32);
        assert!(matches!(result, Err(Ok(BallotError::NotWhitelisted))));
    }
}
