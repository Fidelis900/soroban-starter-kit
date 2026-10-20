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
//! ## Permissionless tally (#1121)
//!
//! Once `voting_end` has passed, tally calculation is permissionless: any
//! caller may invoke `tally_all()` / `tally()` to close the ballot and emit
//! [`events::TallyCompleted`].  `get_tally()` is a read-only query that never
//! requires auth.

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
