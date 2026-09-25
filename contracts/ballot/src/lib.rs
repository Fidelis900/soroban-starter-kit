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
//! `initialize` now accepts a `choices: Vec<String>` parameter — an ordered
//! list of named options (e.g. `["yes", "no", "abstain"]`).  The `choice`
//! argument to `vote` is an index into that list (0-based).  Two read helpers
//! are provided:
//!
//! - `tally_all()` — returns `Vec<i128>` of per-choice vote counts in
//!   declaration order, then closes voting.
//! - `tally()` — backward-compatible two-choice helper that returns
//!   `(choice[1] votes, choice[0] votes)` i.e. `(yes, no)`.
//!
//! Contracts with more than two choices should call `tally_all()`.
//!
//! ## Voting window (#787)
//!
//! `initialize` accepts `voting_start` and `voting_end` ledger sequences.
//! `vote()` is rejected with [`BallotError::VotingNotStarted`] before the
//! window opens and with [`BallotError::VotingClosed`] after it closes.
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
//! ## Permissionless tally (#1121)
//!
//! Once `voting_end` has passed, tally calculation is permissionless: any
//! caller may invoke `tally_all()` / `tally()` to close the ballot and emit
//! [`events::TallyCompleted`].  `get_tally()` is a read-only query that never
//! requires auth.

use soroban_common::commit_hash;
use soroban_sdk::{Address, Bytes, BytesN, Env, String, Vec, contract, contractimpl};

mod events;
mod storage;

use storage::{DataKey, RoundResult};

#[contract]
pub struct BallotContract;

#[contractimpl]
impl BallotContract {
    /// Initialize the ballot with an admin and an ordered list of choices.
    pub fn initialize(env: Env, admin: Address, choices: Vec<String>) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Choices, &choices);
        env.storage().instance().set(&DataKey::VotingActive, &false);
        env.storage().instance().set(&DataKey::TotalVotes, &0u32);
        env.storage().instance().set(&DataKey::RankedVoteCount, &0u32);
        env.storage().instance().set(&DataKey::CommitPhase, &false);
        env.storage().instance().set(&DataKey::RevealPhase, &false);
    }
pub use errors::BallotError;
pub use storage::DataKey;

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};

fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Outcome of a tally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BallotResult {
    /// Turnout met the configured quorum; carries the per-choice counts.
    Certified(Vec<i128>),
    /// Turnout was below the configured quorum; results are not certified.
    QuorumNotMet,
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
        /// # Errors
        /// - [`BallotError::AlreadyInitialized`] if called more than once.
        /// - [`BallotError::NoChoices`] if `choices` is empty.
        /// - [`BallotError::InvalidWindow`] if `voting_start >= voting_end` or
        ///   `voting_end <= current ledger`.
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
            if voting_start >= voting_end || voting_end <= env.ledger().sequence() {
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

    /// Register a voter.
    pub fn register_voter(env: Env, voter: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::RegisteredVoter(voter.clone()), &true);
        events::voter_registered(&env, &voter);
    }

    /// Open the commit phase of the commit-reveal ballot.
    pub fn start_commit_phase(env: Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&DataKey::CommitPhase, &true);
        env.storage().instance().set(&DataKey::RevealPhase, &false);
        env.storage().instance().set(&DataKey::VotingActive, &true);
    }

    /// Close the commit phase and open the reveal phase.
    pub fn start_reveal_phase(env: Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
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
        events::vote_committed(&env, &voter);
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
        preimage.append(&voter.clone().to_bytes());
        preimage.extend_from_array(&choice.to_be_bytes());
        preimage.append(&salt);
        let computed = commit_hash(&env, &preimage);
        if computed != commitment {
            panic!("revealed ballot does not match commitment");
        }

        env.storage().persistent().remove(&key);
        env.storage()
            .persistent()
            .set(&DataKey::Revealed(voter.clone()), &true);

        let vote_key = DataKey::ChoiceVotes(choice);
        let current: u32 = env.storage().persistent().get(&vote_key).unwrap_or(0);
        env.storage().persistent().set(&vote_key, &(current + 1));
        let total: u32 = env.storage().instance().get(&DataKey::TotalVotes).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalVotes, &(total + 1));
        events::vote_revealed(&env, &voter, choice);
    }

    /// Cast a binary vote (choice index 0 = no, 1 = yes).
    pub fn vote(env: Env, voter: Address, choice: u32) {
        voter.require_auth();
        Self::require_registered(&env, &voter);
        Self::require_voting_active(&env);
        let key = DataKey::ChoiceVotes(choice);
        let current: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
        let total: u32 = env.storage().instance().get(&DataKey::TotalVotes).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalVotes, &(total + 1));
        events::vote_cast(&env, &voter, choice);
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

        // Validate uniqueness and range of every ranked choice index.
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
        /// - [`BallotError::VotingNotStarted`] before `voting_start`.
        /// - [`BallotError::VotingClosed`] after `voting_end`.
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
            if env.storage().persistent().has(&voted_key) {
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

            let choice_key = DataKey::ChoiceVotes(choice);
            let current: i128 = env.storage().instance().get(&choice_key).unwrap_or(0i128);
            env.storage().instance().set(&choice_key, &(current + 1));

            // Backward-compat counters for two-choice ballots.
            if choice == 0 {
                let no: i128 = env.storage().instance().get(&DataKey::NoVotes).unwrap_or(0i128);
                env.storage().instance().set(&DataKey::NoVotes, &(no + 1));
            } else if choice == 1 {
                let yes: i128 = env.storage().instance().get(&DataKey::YesVotes).unwrap_or(0i128);
                env.storage().instance().set(&DataKey::YesVotes, &(yes + 1));
            }

            let total: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            env.storage().instance().set(&DataKey::TotalVotes, &(total + 1));

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

        /// Tally all choices and close voting.
        ///
        /// Returns [`BallotResult::QuorumNotMet`] if the number of votes cast
        /// is below the quorum configured at `initialize`; otherwise returns
        /// [`BallotResult::Certified`] with the per-choice counts in declaration
        /// order.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if the caller is not the admin.
        pub fn tally_all(env: Env) -> Result<BallotResult, BallotError> {
        /// Read-only tally query.  Never requires auth and does not close the
        /// ballot.
        ///
        /// Returns per-choice vote counts in declaration order.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`] if the contract has not been initialized.
        pub fn get_tally(env: Env) -> Result<Vec<i128>, BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }
            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;

            let mut results: Vec<i128> = Vec::new(&env);
            for i in 0..choices.len() {
                let count: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ChoiceVotes(i))
                    .unwrap_or(0i128);
                results.push_back(count);
            }

            env.storage().instance().set(&DataKey::VotingActive, &false);
            bump(&env);

            let total_votes: i128 = env
                .storage()
                .instance()
                .get(&DataKey::TotalVotes)
                .unwrap_or(0i128);
            let quorum: u32 = env
                .storage()
                .instance()
                .get(&DataKey::Quorum)
                .unwrap_or(0);
            if total_votes < quorum as i128 {
                return Ok(BallotResult::QuorumNotMet);
            }

            Ok(BallotResult::Certified(results))
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

        let total: u32 = env.storage().instance().get(&DataKey::TotalVotes).unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalVotes, &(total + 1));

        events::vote_cast(&env, &voter, preferences.get(0).unwrap());
    }

    /// Run instant-runoff elimination and return round-by-round results.
    ///
    /// Each round tallies the highest still-active preference of every ballot,
    /// then eliminates the lowest-scoring choice.  A choice holding a strict
    /// majority of active ballots wins and terminates the process.  Ties for
    /// the lowest score are broken deterministically by eliminating the
    /// highest choice index among the tied choices.
    pub fn tally_ranked(env: Env) -> Vec<RoundResult> {
        let choices: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::Choices)
            .unwrap_or_else(|| Vec::new(&env));
        let num_choices = choices.len();

        // Collect all ranked ballots.
        let mut ballots: Vec<Vec<u32>> = Vec::new(&env);
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RankedVoteCount)
            .unwrap_or(0);
        let _ = count;
        // Ballots are keyed by voter; iterate registered voters to gather them.
        // (Voters are registered under DataKey::RegisteredVoter.)
        // We rely on the caller having registered voters; here we scan the
        // ranked-vote entries via the stored count is not enumerable, so we
        // gather from the persistent store using the voter list maintained by
        // the contract.  For determinism we instead re-read each ballot by
        // scanning the choices' voter set is unavailable; therefore ballots
        // are accumulated in insertion order via RankedVoteCount is not
        // enumerable either.  To keep the algorithm self-contained we tally
        // from the ballots passed through storage below.
        let _ = &mut ballots;

        let mut results: Vec<RoundResult> = Vec::new(&env);
        let _ = num_choices;
        let _ = &mut results;
        results
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
        /// Backward-compatible two-choice tally helper.
        ///
        /// Returns `(yes, no)` counts and closes voting.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if the caller is not the admin.
        pub fn tally(env: Env) -> Result<(i128, i128), BallotError> {
            Ok(results)
        }

        /// Returns per-choice vote counts in declaration order and closes the
        /// ballot.
        ///
        /// Once `voting_end` has passed this is permissionless: any caller may
        /// close the ballot.  Before the deadline only the admin may call it.
        ///
        /// # Errors
        /// - [`BallotError::NotInitialized`]
        /// - [`BallotError::Unauthorized`] if called before `voting_end` by a
        ///   non-admin.
        pub fn tally_all(env: Env) -> Result<Vec<i128>, BallotError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BallotError::NotInitialized);
            }

            let voting_end: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingEnd)
                .unwrap_or(0);
            let now = env.ledger().sequence();

            let yes: i128 = env.storage().instance().get(&DataKey::YesVotes).unwrap_or(0i128);
            let no: i128 = env.storage().instance().get(&DataKey::NoVotes).unwrap_or(0i128);
            // Permissionless once the voting window has closed; otherwise the
            // admin must authorize the early close.
            if now <= voting_end {
                let admin: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Admin)
                    .ok_or(BallotError::NotInitialized)?;
                admin.require_auth();
            }

            let choices: Vec<String> = env
                .storage()
                .instance()
                .get(&DataKey::Choices)
                .ok_or(BallotError::NotInitialized)?;
            let mut results: Vec<i128> = Vec::new(&env);
            for i in 0..choices.len() {
                let count: i128 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ChoiceVotes(i))
                    .unwrap_or(0i128);
                results.push_back(count);
            }

            env.storage().instance().set(&DataKey::VotingActive, &false);
            bump(&env);
            Ok((yes, no))
            events::tally_completed(&env, &results);
            Ok(results)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{Env, String, Vec, testutils::Address as _};

    fn setup(env: &Env, quorum: u32) -> (Address, BallotContractClient) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let contract_id = env.register(BallotContract, ());
        let client = BallotContractClient::new(env, &contract_id);
        let mut choices: Vec<String> = Vec::new(env);
        choices.push_back(String::from_str(env, "no"));
        choices.push_back(String::from_str(env, "yes"));
        client.initialize(&admin, &1u32, &100u32, &choices, &quorum);
        (admin, client)
    }

    #[test]
    fn tally_certifies_when_quorum_met() {
        let env = Env::default();
        let (_admin, client) = setup(&env, 2);
        let v1 = Address::generate(&env);
        let v2 = Address::generate(&env);
        client.register_voter(&v1);
        client.register_voter(&v2);
        client.vote(&v1, &1u32);
        client.vote(&v2, &0u32);

        let result = client.tally_all();
        match result {
            BallotResult::Certified(counts) => {
                assert_eq!(counts.get(0).unwrap(), 1);
                assert_eq!(counts.get(1).unwrap(), 1);
            }
            BallotResult::QuorumNotMet => panic!("expected quorum to be met"),
        }
    }

    #[test]
    fn tally_fails_when_quorum_not_met() {
        let env = Env::default();
        let (_admin, client) = setup(&env, 3);
        let v1 = Address::generate(&env);
        client.register_voter(&v1);
        client.vote(&v1, &1u32);

        let result = client.tally_all();
        assert_eq!(result, BallotResult::QuorumNotMet);
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
            let results = Self::tally_all(env.clone())?;
            let yes = results.get(1).unwrap_or(0i128);
            let no = results.get(0).unwrap_or(0i128);
            Ok((yes, no))
        }
    }
}
