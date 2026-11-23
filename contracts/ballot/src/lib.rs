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
//! ## Weighted voting (#1329)
//!
//! `register_voter` assigns a default weight of 1, while
//! `register_voter_with_weight` assigns a custom weight.  Every vote adds the
//! voter's weight to the selected choice, the binary counters, and the total
//! turnout used for quorum.
//!
//! ## Permissionless tally (#1121)
//!
//! Once `voting_end` has passed, tally calculation is permissionless: any
//! caller may invoke `tally_all()` / `tally()` to close the ballot and emit
//! [`events::TallyCompleted`].  `get_tally()` is a read-only query that never
//! requires auth.

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Vec};
use soroban_common::commit_hash;
use soroban_sdk::{Address, Bytes, BytesN, Env, String, Vec, contract, contractimpl, contracttype};

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


/// Target TTL (in ledgers) applied when instance storage is extended.
const LEDGER_BUMP_AMOUNT: u32 = 518_400;

/// Minimum number of ledgers between the current ledger and `voting_start`.
///
/// Initialization rejects windows that open sooner, giving voters a planned
/// preparation period before the ballot becomes active.  Matches
/// `soroban_common::MIN_DEADLINE_BUFFER`.
pub const MIN_DEADLINE_BUFFER: u32 = 10;


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

/// Policy used to resolve a tie between two or more leading choices (#1332).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TieBreaker {
    /// A tie blocks certification until a runoff breaks it.
    RunoffRequired,
    /// The admin selects one of the tied choices via `resolve_tie`.
    AdminDecidingVote,
}

/// Winner of a closed ballot (#1332).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BallotWinner {
    /// A single choice holds the strictly highest tally.
    Choice(u32),
    /// Two or more choices share the highest tally; carries their indices.
    Tied(Vec<u32>),
    /// No votes were cast.
    NoVotes,
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

                return Err(BallotError::InvalidWindow);
            }

            let earliest_end = voting_start
                .checked_add(MIN_VOTING_DURATION)
                .ok_or(BallotError::InvalidWindow)?;
            if voting_end < earliest_end || voting_start >= voting_end {
                return Err(BallotError::InvalidWindow);
            }

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

            // Backward-compat counters: slot 0 -> NoVotes, slot 1 -> YesVotes.
            env.storage().instance().set(&DataKey::YesVotes, &0i128);
            env.storage().instance().set(&DataKey::NoVotes, &0i128);

            bump(&env);
            events::initialized(&env, &admin);
            Ok(())
        }

        /// Admin registers a voter for the ballot with the default weight of 1.
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
            Self::register_with_weight(env, voter, 1)
        }

        /// Admin registers a voter with a custom voting weight (#1329).
        ///
        /// The weight is stored under [`DataKey::VoterWeight`] and applied to
        /// every vote cast by `voter`.  A weight of `0` is permitted and
        /// records no tally for that voter.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        pub fn register_voter_with_weight(
            env: Env,
            voter: Address,
            weight: u32,
        ) -> Result<(), BallotError> {
            Self::register_with_weight(env, voter, weight)
        }

        /// Returns the voting weight assigned to `voter` (defaults to 1).
        pub fn get_voter_weight(env: Env, voter: Address) -> u32 {
            Self::voter_weight(&env, &voter)
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

        /// Admin revokes a voter registration before voting opens (#1330).
        ///
        /// This is stricter than [`BallotContract::deregister_voter`]: it is
        /// only callable while `current_ledger < voting_start`, before the
        /// ballot enters its voting phase.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        /// - [`BallotError::VotingAlreadyStarted`] if the voting window has opened.
        /// - [`BallotError::NotRegistered`] if the voter is not currently registered.
        pub fn unregister_voter(env: Env, voter: Address) -> Result<(), BallotError> {
            env.storage()
                .persistent()
                .remove(&DataKey::VoterWeight(voter.clone()));

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

            let voting_start: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingStart)
                .ok_or(BallotError::NotInitialized)?;
            if env.ledger().sequence() >= voting_start {
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

        /// Returns whether `voter` is currently registered.
        pub fn is_registered(env: Env, voter: Address) -> bool {
            env.storage()
                .persistent()
                .get(&DataKey::RegisteredVoter(voter))
                .unwrap_or(false)
        }

        /// Voter casts their vote.
        ///
        /// `choice` is a 0-based index into the `choices` list supplied at
        /// `initialize`.  For two-choice ballots the conventional mapping is
        /// `0 = no`, `1 = yes`.  The voter's registered weight is added to the
        /// selected choice, the binary counters, and the total turnout.
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

            let weight = Self::voter_weight(&env, &voter);
            Self::record_vote(&env, choice, weight);
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

        /// Open the commit phase of the commit-reveal ballot.
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

        /// Close the commit phase and open the reveal phase.
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

        /// Phase 1: commit `hash(voter ++ choice ++ salt)` without revealing the choice.
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

        /// Open the commit phase of the commit-reveal ballot.
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

        /// Close the commit phase and open the reveal phase.
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

        /// Phase 1: commit `hash(voter ++ choice ++ salt)` without revealing the choice.
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

        /// Phase 2: reveal `(choice, salt)` and validate against the commitment.
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

        /// Phase 2: reveal `(choice, salt)` and validate against the commitment.
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

        /// Open the commit phase of the commit-reveal ballot.
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

        /// Close the commit phase and open the reveal phase.
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

        /// Phase 1: commit `hash(voter ++ choice ++ salt)` without revealing the choice.
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

        /// Phase 2: reveal `(choice, salt)` and validate against the commitment.
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

            let weight = Self::voter_weight(&env, &voter);
            Self::record_vote(&env, choice, weight);
            events::vote_revealed(&env, &voter, choice);
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

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + 1));

            if let Some(first) = preferences.get(0) {
                events::voted(&env, &voter, first);
            }
            let weight = Self::voter_weight(&env, &voter);
            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + i128::from(weight)));

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

        fn register_with_weight(env: Env, voter: Address, weight: u32) -> Result<(), BallotError> {
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

            let weight_key = DataKey::VoterWeight(voter.clone());
            env.storage().persistent().set(&weight_key, &weight);
            env.storage().persistent().extend_ttl(
                &weight_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            bump(&env);
            events::voter_registered(&env, &voter);
            Ok(())
        }

        fn voter_weight(env: &Env, voter: &Address) -> u32 {
            env.storage()
                .persistent()
                .get(&DataKey::VoterWeight(voter.clone()))
                .unwrap_or(1u32)
        }

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

        fn record_vote(env: &Env, choice: u32, weight: u32) {
            let amount = i128::from(weight);
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

        /// Open the commit phase of the commit-reveal ballot.
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

        /// Close the commit phase and open the reveal phase.
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

        /// Phase 1: commit `hash(voter ++ choice ++ salt)` without revealing the choice.
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

        /// Phase 2: reveal `(choice, salt)` and validate against the commitment.
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

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0);
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

        /// Total number of votes cast.
        pub fn total_votes(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128)
        }

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
            let choice_key = DataKey::ChoiceVotes(choice);
            let current: i128 = env.storage().instance().get(&choice_key).unwrap_or(0i128);
            env.storage().instance().set(&choice_key, &(current + 1));

            if choice == 0 {
                let no: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::NoVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::NoVotes, &(no + 1));
            } else if choice == 1 {
                let yes: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::YesVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::YesVotes, &(yes + 1));
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + 1));
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
        /// Configure how ties between leading choices are resolved (#1332).
        ///
        /// Admin-only.  Defaults to [`TieBreaker::RunoffRequired`], meaning a
        /// tied ballot cannot be certified until the tie is broken.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if the caller is not the admin.
        pub fn set_tie_breaker(env: Env, tie_breaker: TieBreaker) -> Result<(), BallotError> {
            let admin = ballot_admin(&env)?;
            admin.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::TieBreaker, &tie_breaker);
            Ok(())
        }

        /// Returns the configured tie-breaker policy.
        pub fn tie_breaker(env: Env) -> TieBreaker {
            env.storage()
                .instance()
                .get(&DataKey::TieBreaker)
                .unwrap_or(TieBreaker::RunoffRequired)
        }

        /// Admin selects one of the tied leading choices, allowing an
        /// `AdminDecidingVote` ballot to be certified (#1332).
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if the caller is not the admin.
        /// - [`BallotError::VotingNotClosed`] if voting is still open.
        /// - [`BallotError::InvalidTieResolution`] if `choice` is not among the
        ///   tied choices.
        pub fn resolve_tie(env: Env, choice: u32) -> Result<(), BallotError> {
            let admin = ballot_admin(&env)?;
            admin.require_auth();
            ballot_require_closed(&env)?;

            let tied = ballot_tied_choices(&env)?;
            if !tied.contains(choice) {
                return Err(BallotError::InvalidTieResolution);
            }
            env.storage()
                .instance()
                .set(&DataKey::TieResolution, &choice);
            Ok(())
        }

        /// Determines the winner of a closed ballot without mutating state
        /// (#1332).
        ///
        /// Returns [`BallotWinner::Tied`] with the tied choice indices when no
        /// single choice leads.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::VotingNotClosed`] if voting is still open.
        pub fn get_winner(env: Env) -> Result<BallotWinner, BallotError> {
            ballot_require_closed(&env)?;
            let tied = ballot_tied_choices(&env)?;
            if tied.is_empty() {
                return Ok(BallotWinner::NoVotes);
            }
            if tied.len() == 1 {
                return Ok(BallotWinner::Choice(tied.get(0).unwrap_or(0u32)));
            }
            Ok(BallotWinner::Tied(tied))
        }

        /// Certifies the final result once voting has closed (#1333).
        ///
        /// Snapshots the winning choice and the per-choice tally into an
        /// immutable `BytesN<32>` hash, stores it in instance storage, emits a
        /// `BallotCertified` event and returns the hash.  Certification is
        /// one-shot: a second call fails with
        /// [`BallotError::AlreadyCertified`].
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::AlreadyCertified`] if results are already certified.
        /// - [`BallotError::VotingNotClosed`] if voting is still open.
        /// - [`BallotError::QuorumNotMet`] if turnout is below the quorum.
        /// - [`BallotError::ResultsNotReady`] if no votes were cast.
        /// - [`BallotError::Tied`] if the leading choices are tied and the
        ///   configured tie-breaker has not resolved the tie.
        pub fn certify_results(env: Env) -> Result<BytesN<32>, BallotError> {
            if env.storage().instance().has(&DataKey::CertifiedHash) {
                return Err(BallotError::AlreadyCertified);
            }
            ballot_require_closed(&env)?;

            let counts = ballot_tally_counts(&env)?;
            let total = ballot_total_votes(&env);
            let quorum = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::Quorum)
                .unwrap_or(0u32);
            if total < quorum as i128 {
                return Err(BallotError::QuorumNotMet);
            }

            let tied = ballot_tied_choices(&env)?;
            let winner = if tied.is_empty() {
                return Err(BallotError::ResultsNotReady);
            } else if tied.len() == 1 {
                tied.get(0).unwrap_or(0u32)
            } else {
                match Self::tie_breaker(env.clone()) {
                    TieBreaker::AdminDecidingVote => {
                        let resolved: Option<u32> =
                            env.storage().instance().get(&DataKey::TieResolution);
                        match resolved {
                            Some(choice) if tied.contains(choice) => choice,
                            _ => return Err(BallotError::Tied),
                        }
                    }
                    TieBreaker::RunoffRequired => return Err(BallotError::Tied),
                }
            };

            let hash = ballot_result_hash(&env, winner, &counts);
            let now = env.ledger().sequence();
            env.storage()
                .instance()
                .set(&DataKey::CertifiedHash, &hash);
            env.storage()
                .instance()
                .set(&DataKey::CertifiedWinner, &winner);
            env.storage().instance().set(&DataKey::CertifiedAt, &now);
            events::ballot_certified(&env, &hash, winner, &counts);
            Ok(hash)
        }

        /// Returns `(hash, certified_at_ledger)` once results are certified.
        pub fn certified_results(env: Env) -> Option<(BytesN<32>, u32)> {
            let hash: Option<BytesN<32>> = env.storage().instance().get(&DataKey::CertifiedHash);
            hash.map(|h| {
                let at: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::CertifiedAt)
                    .unwrap_or(0u32);
                (h, at)
            })
        }
    }

    /// Returns the admin address or [`BallotError::NotInitialized`].
    fn ballot_admin(env: &Env) -> Result<Address, BallotError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BallotError::NotInitialized)
    }

    /// Rejects tally/certification calls made before `voting_end`.
    fn ballot_require_closed(env: &Env) -> Result<(), BallotError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(BallotError::NotInitialized);
        }
        let voting_end: u32 = env
            .storage()
            .instance()
            .get(&DataKey::VotingEnd)
            .unwrap_or(0u32);
        if env.ledger().sequence() <= voting_end {
            return Err(BallotError::VotingNotClosed);
        }
        Ok(())
    }

    /// Reads the per-choice tallies in declaration order.
    fn ballot_tally_counts(env: &Env) -> Result<Vec<i128>, BallotError> {
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

    /// Total number of votes cast, tolerating the legacy `u32` encoding.
    fn ballot_total_votes(env: &Env) -> i128 {
        if let Some(v) = env
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::TotalVotes)
        {
            return v;
        }
        env.storage()
            .instance()
            .get::<DataKey, u32>(&DataKey::TotalVotes)
            .map(|v| v as i128)
            .unwrap_or(0i128)
    }

    /// Returns the indices of the choices sharing the highest tally.
    ///
    /// An empty vector means no votes were cast; a single element means a
    /// unique leader.  This makes tie detection deterministic and independent
    /// of iteration order (#1332).
    fn ballot_tied_choices(env: &Env) -> Result<Vec<u32>, BallotError> {
        let counts = ballot_tally_counts(env)?;
        let mut max = 0i128;
        for count in counts.iter() {
            if count > max {
                max = count;
            }
        }
        let mut tied: Vec<u32> = Vec::new(env);
        if max > 0 {
            for i in 0..counts.len() {
                if counts.get(i).unwrap_or(0i128) == max {
                    tied.push_back(i);
                }
            }
            Ok(results)
        }

        fn record_vote(env: &Env, choice: u32) {
            let choice_key = DataKey::ChoiceVotes(choice);
            let current: i128 = env.storage().instance().get(&choice_key).unwrap_or(0i128);
            env.storage().instance().set(&choice_key, &(current + 1));

            if choice == 0 {
                let no: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::NoVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::NoVotes, &(no + 1));
            } else if choice == 1 {
                let yes: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::YesVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::YesVotes, &(yes + 1));
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + 1));
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
                .get(&DataKey::RankedVoteCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::RankedVoteCount, &(count + 1));

            let total: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0);
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
            let choice_key = DataKey::ChoiceVotes(choice);
            let current: i128 = env.storage().instance().get(&choice_key).unwrap_or(0i128);
            env.storage().instance().set(&choice_key, &(current + 1));

            if choice == 0 {
                let no: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::NoVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::NoVotes, &(no + 1));
            } else if choice == 1 {
                let yes: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::YesVotes)
                    .unwrap_or(0i128);
                env.storage().instance().set(&DataKey::YesVotes, &(yes + 1));
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage()
                .instance()
                .set(&DataKey::TotalVotes, &(total + 1));
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
        }
        Ok(tied)
    }

    /// Builds the immutable `(winner, tallies)` snapshot hash (#1333).
    fn ballot_result_hash(env: &Env, winner: u32, counts: &Vec<i128>) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.extend_from_array(&winner.to_be_bytes());
        for count in counts.iter() {
            preimage.extend_from_array(&i128_to_be_bytes(count));
        }
        commit_hash(env, &preimage)
    }

    /// Encodes a non-negative tally as 16 big-endian bytes.
    fn i128_to_be_bytes(value: i128) -> [u8; 16] {
        value.to_be_bytes()
    }
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

    // ── #1332 / #1333: tie-breaking and result certification ───────────────

    use soroban_sdk::testutils::Ledger as _;

    /// Registers `votes.len()` fresh voters, casts each vote inside the open
    /// window, then advances the ledger past `voting_end`.
    fn setup_closed_ballot(
        env: &Env,
        quorum: u32,
        votes: &[u32],
    ) -> (Address, BallotContractClient) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(env, &contract_id);
        let mut choices: Vec<String> = Vec::new(env);
        choices.push_back(String::from_str(env, "no"));
        choices.push_back(String::from_str(env, "yes"));
        client.initialize(&admin, &1u32, &100u32, &choices, &quorum);

        env.ledger().set_sequence_number(50);
        for choice in votes {
            let voter = Address::generate(env);
            client.register_voter(&voter);
            client.vote(&voter, choice);
        }
        env.ledger().set_sequence_number(101);
        (admin, client)
    }

    #[test]
    fn get_winner_detects_tie() {
        let env = Env::default();
        let (_admin, client) = setup_closed_ballot(&env, 2, &[0, 1]);

        match client.get_winner() {
            BallotWinner::Tied(indices) => assert_eq!(indices.len(), 2),
            other => panic!("expected a tie, got {:?}", other),
        }
    }

    #[test]
    fn certify_results_rejects_tied_ballot() {
        let env = Env::default();
        let (_admin, client) = setup_closed_ballot(&env, 2, &[0, 1]);

        let result = client.try_certify_results();
        assert_eq!(result, Err(Ok(BallotError::Tied)));
    }

    #[test]
    fn admin_tie_break_allows_certification() {
        let env = Env::default();
        let (_admin, client) = setup_closed_ballot(&env, 2, &[0, 1]);

        client.set_tie_breaker(&TieBreaker::AdminDecidingVote);
        client.resolve_tie(&1u32);

        let hash = client.certify_results();
        let (stored, at) = client.certified_results().unwrap();
        assert_eq!(stored, hash);
        assert_eq!(at, 101);

        // Certification is one-shot.
        assert_eq!(
            client.try_certify_results(),
            Err(Ok(BallotError::AlreadyCertified))
        );
    }

    #[test]
    fn resolve_tie_rejects_non_tied_choice() {
        let env = Env::default();
        let (_admin, client) = setup_closed_ballot(&env, 2, &[0, 1]);

        client.set_tie_breaker(&TieBreaker::AdminDecidingVote);
        // Two choices are tied; index 5 is neither of them.
        let result = client.try_resolve_tie(&5u32);
        assert_eq!(result, Err(Ok(BallotError::InvalidTieResolution)));
    }

    #[test]
    fn certify_results_snapshots_unique_winner() {
        let env = Env::default();
        // 2 votes for "yes" (1), 1 vote for "no" (0).
        let (_admin, client) = setup_closed_ballot(&env, 2, &[1, 1, 0]);

        match client.get_winner() {
            BallotWinner::Choice(1) => {}
            other => panic!("expected choice 1 to win, got {:?}", other),
        }

        let hash = client.certify_results();
        let (stored, at) = client.certified_results().unwrap();
        assert_eq!(stored, hash);
        assert_eq!(at, 101);
    }

    #[test]
    fn certify_results_requires_voting_closed() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(&env, &contract_id);
        let mut choices: Vec<String> = Vec::new(&env);
        choices.push_back(String::from_str(&env, "no"));
        choices.push_back(String::from_str(&env, "yes"));
        client.initialize(&admin, &1u32, &100u32, &choices, &0u32);

        let result = client.try_certify_results();
        assert_eq!(result, Err(Ok(BallotError::VotingNotClosed)));
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

    /// Any relayer can reveal a valid blinded commitment.
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

        // The reveal path never receives the voter address or requires auth.
        client.reveal_vote_blinded(&commitment, &secret, &choice, &proof);

        assert_eq!(client.get_yes_votes(), 1);
        assert_eq!(client.total_votes(), 1);
        let counts = client.get_tally();
        assert_eq!(counts.get(0).unwrap(), 0);
        assert_eq!(counts.get(1).unwrap(), 1);
    }

    /// A mismatched preimage is rejected.
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

    /// A blinded commitment cannot be revealed twice.
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

    /// Blinded reveals are only accepted during the reveal phase.
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

    /// An unknown commitment hash is rejected.
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

    /// Only registered voters may submit blinded commitments.
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
    // Issue #1330 — voter revocation before the ballot opens
    // -----------------------------------------------------------------------

    /// The admin can unregister a voter while the ballot is pending.
    #[test]
    fn unregister_voter_before_opening_removes_registration() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        client.register_voter(&voter);
        assert!(client.is_registered(&voter));

        client.unregister_voter(&voter);
        assert!(!client.is_registered(&voter));
    }

    /// An unregistered voter can no longer vote once the window opens.
    #[test]
    fn unregistered_voter_cannot_vote() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        client.register_voter(&voter);
        client.unregister_voter(&voter);

        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        let result = client.try_vote(&voter, &1u32);
        assert!(matches!(result, Err(Ok(BallotError::NotRegistered))));
    }

    /// Revocation is rejected exactly at the opening ledger.
    #[test]
    fn unregister_voter_rejected_at_opening() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        client.register_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);

        let result = client.try_unregister_voter(&voter);
        assert!(matches!(result, Err(Ok(BallotError::VotingAlreadyStarted))));
        assert!(client.is_registered(&voter));
    }

    /// Revocation is rejected after the ballot has opened.
    #[test]
    fn unregister_voter_rejected_after_opening() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let voter = Address::generate(&env);

        client.register_voter(&voter);
        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START + 1);

        let result = client.try_unregister_voter(&voter);
        assert!(matches!(result, Err(Ok(BallotError::VotingAlreadyStarted))));
        assert!(client.is_registered(&voter));
    }

    /// Unknown voters cannot be revoked.
    #[test]
    fn unregister_unknown_voter_rejected() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let unknown = Address::generate(&env);

        let result = client.try_unregister_voter(&unknown);
        assert!(matches!(result, Err(Ok(BallotError::NotRegistered))));
    // Issue #1329 — weighted voting
    // -----------------------------------------------------------------------

    /// Votes are counted using each voter's registered weight.
    #[test]
    fn weighted_votes_use_assigned_weights() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        let v3 = Address::generate(&env);

        client.register_voter_with_weight(&v1, &3u32);
        client.register_voter_with_weight(&v2, &2u32);
        client.register_voter(&v3);

        assert_eq!(client.get_voter_weight(&v1), 3);
        assert_eq!(client.get_voter_weight(&v2), 2);
        assert_eq!(client.get_voter_weight(&v3), 1);

        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&v1, &1u32);
        client.vote(&v2, &0u32);
        client.vote(&v3, &1u32);

        assert_eq!(client.get_yes_votes(), 4);
        assert_eq!(client.get_no_votes(), 2);
        assert_eq!(client.total_votes(), 6);

        let counts = client.get_tally();
        assert_eq!(counts.get(0).unwrap(), 2);
        assert_eq!(counts.get(1).unwrap(), 4);
    }

    /// Quorum is evaluated against the weighted turnout.
    #[test]
    fn weighted_quorum_uses_weighted_total() {
        let env = make_env();
        let (_admin, client) = setup(&env, 5);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);

        client.register_voter_with_weight(&v1, &3u32);
        client.register_voter_with_weight(&v2, &2u32);

        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&v1, &1u32);
        client.vote(&v2, &0u32);

        match client.tally_all() {
            BallotResult::Certified(counts) => {
                assert_eq!(counts.get(0).unwrap(), 2);
                assert_eq!(counts.get(1).unwrap(), 3);
            }
            BallotResult::QuorumNotMet => panic!("expected weighted quorum to be met"),
        }
    // Issue #1328 — enforce a future voting window at initialization
    // -----------------------------------------------------------------------

    /// A start ledger in the past is rejected.
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

    /// A start ledger inside the preparation buffer is rejected.
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

    /// A start ledger exactly at the buffer boundary is accepted.
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

    /// A window shorter than the minimum voting duration is rejected.
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

    /// A window exactly at the minimum voting duration is accepted.
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

    /// A degenerate window (end <= start) is rejected.
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

    /// Weighted quorum is not met when the weighted total is too low.
    #[test]
    fn weighted_quorum_can_fail() {
        let env = make_env();
        let (_admin, client) = setup(&env, 6);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);

        client.register_voter_with_weight(&v1, &3u32);
        client.register_voter_with_weight(&v2, &2u32);

        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&v1, &1u32);
        client.vote(&v2, &0u32);

        assert_eq!(client.tally_all(), BallotResult::QuorumNotMet);
    }

    /// A zero-weight voter is recorded but contributes no tally.
    #[test]
    fn zero_weight_vote_contributes_nothing() {
        let env = make_env();
        let (_admin, client) = setup(&env, 0);
        let zero = Address::generate(&env);
        let normal = Address::generate(&env);

        client.register_voter_with_weight(&zero, &0u32);
        client.register_voter(&normal);

        env.ledger()
            .with_mut(|le| le.sequence_number = VOTING_START);
        client.vote(&zero, &1u32);
        client.vote(&normal, &1u32);

        assert_eq!(client.get_yes_votes(), 1);
        assert_eq!(client.total_votes(), 1);
    }

    // -----------------------------------------------------------------------
    // Issue #1328 — enforce a future voting window at initialization
    // -----------------------------------------------------------------------

    /// A start ledger in the past is rejected.
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

    /// A start ledger inside the preparation buffer is rejected.
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

    /// A start ledger exactly at the buffer boundary is accepted.
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

    /// A window shorter than the minimum voting duration is rejected.
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

    /// A window exactly at the minimum voting duration is accepted.
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

    /// A degenerate window (end <= start) is rejected.
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
}
