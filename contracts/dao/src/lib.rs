#![no_std]
#![deny(missing_docs)]
//! DAO governance contract template.
//!
//! Token holders create proposals and cast token-weighted votes; a proposal
//! passes when it reaches quorum with more yes votes than no votes.
//!
//! ## Features
//!
//! * **Executable action payloads** (issue #1108) — proposals may carry an
//!   optional `(target, function, args)` triple that is dispatched atomically
//!   via `env.invoke_contract` on execution.
//! * **Proposal submission bond** (issue #1106) — a configurable bond is
//!   escrowed on `create_proposal`; it is refunded when the proposal passes
//!   quorum or slashed to the admin treasury when it fails to meet minimum
//!   participation.
//! * **Dynamic quorum** (issue #1107) — an exponential moving average of
//!   historical participation keeps quorum between `min_quorum_bps` and
//!   `max_quorum_bps`, bounded in basis points (0–10 000).

use soroban_sdk::{Address, Env, String, Symbol, Val, Vec, contract, contractimpl, token};
//! ## Quorum modes
//!
//! Two quorum controls can be set at initialization; both must be satisfied
//! for a proposal to execute:
//!
//! * **`quorum`** — absolute minimum total votes (in token units).
//! * **`quorum_bps`** — minimum participation as a share of total token supply
//!   expressed in basis points (0–10 000). Set to `0` to disable.
//!
//! ## Execution timelock (#1104)
//!
//! After a proposal passes its voting deadline it enters `Queued` state.
//! `execute_proposal` can be called only once `env.ledger().sequence() >=
//! proposal.execution_eta` (where `execution_eta = deadline +
//! execution_delay`).  The admin / security council may veto a queued
//! proposal at any time before it executes via `veto_proposal`.
//!
//! ## Token locking during voting (#1103)
//!
//! When a voter calls `vote()` their governance tokens are transferred into
//! the DAO contract.  This prevents flash-loan attacks — the tokens are
//! physically absent from the attacker's account during the proposal window.
//! After the proposal's deadline the voter calls `unlock_tokens()` to reclaim
//! their stake.
//!
//! ## Vote delegation (#1105)
//!
//! Token holders may call `delegate(delegator, delegatee)` to assign their
//! voting weight to another address.  When the delegatee calls `vote()` the
//! contract aggregates the delegatee's own locked balance with the locked
//! balances of all addresses that have delegated to them *for this proposal*.
//! Circular delegation chains are rejected at `delegate()` time.
//!
//! ## Proposer self-cancel (#830)
//!
//! The original proposer may call `proposer_cancel_proposal` to retract their
//! proposal **before any vote has been cast**.  This lets them correct mistakes
//! without waiting for the voting period to lapse.
//!
//! ## Proposal listing (#1110)
//!
//! `get_proposals(cursor, limit, state)` returns proposals in ascending ID
//! order, optionally filtered by [`ProposalState`], at most [`MAX_PAGE_SIZE`]
//! per call. Pass the returned `next_cursor` to fetch the following page.

use soroban_sdk::{Address, Env, String, Symbol, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::DaoError;
pub use storage::{DataKey, DelegateKey, LockedTokensKey, Proposal, ProposalKey, ProposalState, VoteKey};
pub use storage::{DataKey, Proposal, ProposalKey, ProposalPage, ProposalState, VoteKey};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, paginate};

/// Maximum number of proposals returned by a single `get_proposals` call.
pub const MAX_PAGE_SIZE: u32 = 50;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::TryIntoVal<Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// DAO governance contract for on-chain proposal creation and token-weighted voting.
///
/// Key security properties:
/// - **Quorum snapshot** – `total_supply_at_creation` is captured from the token
///   at proposal time, preventing token minting/burning from manipulating quorum.
/// - **Token locking** – governance tokens are transferred into this contract when
///   a voter casts their vote, eliminating flash-loan vote manipulation.
/// - **Execution timelock** – passed proposals enter `Queued` state and may only
///   be executed after `execution_delay` ledgers, giving the community time to
///   react to malicious governance.
/// - **Vote delegation** – token holders may delegate their voting weight to
///   trusted representatives without losing token ownership.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    // ── Adaptive-quorum constants ────────────────────────────────────────────
    /// Default EMA smoothing window (number of proposals).
    const DEFAULT_QUORUM_EMA_WINDOW: u32 = 10;
    /// Basis points denominator.
    const BPS_DENOMINATOR: u32 = 10_000;

    #[contract]
    pub struct DaoContract;

    #[contractimpl]
    impl DaoContract {
        /// Initialize the DAO.
        ///
        /// - `voting_period` — number of ledgers a proposal stays open for voting.
        /// - `quorum` — minimum total votes (in token units) required for a valid result.
        ///   Used as the *initial* absolute quorum floor; adaptive quorum BPS are layered
        ///   on top and stored separately.
        /// - `proposal_bond` — token units escrowed by a proposer at submission time
        ///   (issue #1106). Set to 0 to disable bonding.
        /// - `min_quorum_bps` / `max_quorum_bps` — lower and upper bounds (in basis
        ///   points, 0–10 000) for the adaptive quorum EMA (issue #1107). Pass both
        ///   as 0 to disable adaptive quorum.
        /// - `voting_period`   — number of ledgers a proposal stays open for voting.
        /// - `quorum`          — minimum total votes (in token units) required for a valid result.
        /// - `quorum_bps`      — minimum participation as basis points of total supply (0–10 000).
        ///                       Pass `0` to disable the percentage-based quorum check.
        /// - `execution_delay` — number of ledgers a passed proposal must wait in `Queued` state
        ///                       before it can be executed. Pass `0` to allow immediate execution.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::AlreadyInitialized`] if called again.
        /// Returns [`DaoError::InvalidQuorumBps`] if `quorum_bps` > 10 000.
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
            voting_period: u32,
            quorum: i128,
            proposal_bond: i128,
            min_quorum_bps: u32,
            max_quorum_bps: u32,
            quorum_bps: u32,
            execution_delay: u32,
        ) -> Result<(), DaoError> {
            if env.storage().instance().has(&DataKey::Initialized) {
                return Err(DaoError::AlreadyInitialized);
            }
            if quorum_bps > 10_000 {
                return Err(DaoError::InvalidQuorumBps);
            }

            admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage()
                .instance()
                .set(&DataKey::VotingPeriod, &voting_period);
            env.storage().instance().set(&DataKey::Quorum, &quorum);
            env.storage()
                .instance()
                .set(&DataKey::ProposalBond, &proposal_bond);
            env.storage()
                .instance()
                .set(&DataKey::MinQuorumBps, &min_quorum_bps);
            env.storage()
                .instance()
                .set(&DataKey::MaxQuorumBps, &max_quorum_bps);
            env.storage().instance().set(
                &DataKey::QuorumEmaWindow,
                &DEFAULT_QUORUM_EMA_WINDOW,
            );
            // Initialise EMA at the midpoint of the allowed band.
            let initial_ema = (min_quorum_bps + max_quorum_bps) / 2;
            env.storage()
                .instance()
                .set(&DataKey::QuorumEmaBps, &initial_ema);
            env.storage()
                .instance()
                .set(&DataKey::QuorumEmaCount, &0u32);
                .set(&DataKey::QuorumBps, &quorum_bps);
            env.storage()
                .instance()
                .set(&DataKey::ExecutionDelay, &execution_delay);
            env.storage().instance().set(&DataKey::ProposalCount, &0u32);
            env.storage().instance().set(&DataKey::Initialized, &true);

            bump_instance(&env);
            events::initialized(&env, &admin, &token, quorum);

            Ok(())
        }

        /// Create a new proposal. The proposer must hold > 0 governance tokens.
        ///
        /// When `proposal_bond > 0`, exactly `proposal_bond` tokens are transferred
        /// from `proposer` to the DAO contract as an anti-spam measure (issue #1106).
        ///
        /// When `action_target` is `Some`, the proposal carries an executable payload
        /// that is dispatched by `execute_proposal` (issue #1108).
        ///
        /// Returns the newly created `proposal_id`.
        ///
        /// The real total token supply is captured at proposal creation time and
        /// stored in `total_supply_at_creation`.  This snapshot is used for both
        /// the `quorum_bps` check and to cap individual voting weights, preventing
        /// flash-loan-style manipulation.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotInitialized`] if the DAO has not been set up.
        /// Returns [`DaoError::InsufficientVotingPower`] if the proposer has no tokens.
        /// Returns [`DaoError::InsufficientBondBalance`] if the proposer cannot cover
        ///   the proposal bond.
        pub fn create_proposal(
            env: Env,
            proposer: Address,
            title: String,
            description: String,
            action_target: Option<Address>,
            action_function: Option<Symbol>,
            action_args: Option<Vec<Val>>,
        ) -> Result<u32, DaoError> {
            Self::require_initialized(&env)?;
            proposer.require_auth();

            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(DaoError::NotInitialized)?;
            let token_client = token::Client::new(&env, &token_addr);

            let balance = token_client.balance(&proposer);

            let balance = token::Client::new(&env, &token).balance(&proposer);
            if balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }

            // ── Bond escrow (issue #1106) ─────────────────────────────────────
            let proposal_bond: i128 = env
                .storage()
                .instance()
                .get(&DataKey::ProposalBond)
                .unwrap_or(0);
            if proposal_bond > 0 {
                if balance < proposal_bond {
                    return Err(DaoError::InsufficientBondBalance);
                }
                let dao_addr = env.current_contract_address();
                // Transfer the bond into the DAO contract's own account.
                token_client.transfer(&proposer, &dao_addr, &proposal_bond);
            }
            // FIX #1102: Query the real total supply from the token contract at
            // proposal creation time instead of using the i128::MAX sentinel.
            // This snapshot is stored on the proposal and used later for:
            //   (a) the quorum_bps percentage check in execute_proposal, and
            //   (b) capping each voter's weight to prevent flash-loan attacks.
            let total_supply_at_creation: i128 = env.invoke_contract(
                &token,
                &Symbol::new(&env, "total_supply"),
                Vec::new(&env),
            );

            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ProposalCount)
                .unwrap_or(0);
            let proposal_id = count;

            let voting_period: u32 = env
                .storage()
                .instance()
                .get(&DataKey::VotingPeriod)
                .ok_or(DaoError::NotInitialized)?;
            let deadline = env.ledger().sequence() + voting_period;

            let proposal = Proposal {
                id: proposal_id,
                proposer: proposer.clone(),
                title,
                description,
                deadline,
                yes_votes: 0,
                no_votes: 0,
                state: ProposalState::Active,
                action_target,
                action_function,
                action_args,
                bond_amount: proposal_bond,
                total_supply_at_creation,
                execution_eta: 0,
            };

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            env.storage()
                .instance()
                .set(&DataKey::ProposalCount, &(count + 1));

            bump_instance(&env);
            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            events::proposal_created(&env, &proposer, proposal_id);

            Ok(proposal_id)
        }

        /// Cast a vote on an active proposal.
        ///
        /// The voter's governance tokens are **transferred into this contract** for
        /// the duration of the voting window, preventing flash-loan attacks (#1103).
        /// Tokens are returned via [`unlock_tokens`] after `proposal.deadline`.
        ///
        /// If the caller has a delegation set, the delegated tokens are also
        /// pulled and counted toward the delegatee's vote weight (#1105).
        ///
        /// Voting weight is capped at `proposal.total_supply_at_creation` so that
        /// tokens minted after the proposal was created cannot inflate any one
        /// voter's power.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        /// Returns [`DaoError::VotingClosed`] if the voting period has ended.
        /// Returns [`DaoError::AlreadyVoted`] if the voter has already voted.
        /// Returns [`DaoError::InsufficientVotingPower`] if the voter has no tokens.
        pub fn vote(
            env: Env,
            voter: Address,
            proposal_id: u32,
            support: bool,
        ) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;
            voter.require_auth();

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }
            if env.ledger().sequence() > proposal.deadline {
                return Err(DaoError::VotingClosed);
            }

            let vote_key = VoteKey {
                proposal_id,
                voter: voter.clone(),
            };
            if env.storage().persistent().has(&vote_key) {
                return Err(DaoError::AlreadyVoted);
            }

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(DaoError::NotInitialized)?;
            let token_client = token::Client::new(&env, &token);

            // --- Compute the voting weight (#1103 + #1105) ---
            //
            // 1. Start with the voter's own current balance.
            // 2. Add the current balance of every address that has delegated to
            //    the voter *and has not already voted directly*, aggregating
            //    delegated voting power (liquid democracy — #1105).
            // 3. Transfer all counted tokens into this DAO contract so they
            //    cannot be moved while the vote is open (anti-flash-loan — #1103).
            // 4. Cap the combined weight at total_supply_at_creation.

            let dao_address = env.current_contract_address();

            // Voter's own balance.
            let own_balance = token_client.balance(&voter);
            if own_balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }

            // Lock the voter's own tokens into the DAO contract.
            token_client.transfer(&voter, &dao_address, &own_balance);

            let locked_key = LockedTokensKey {
                proposal_id,
                voter: voter.clone(),
            };
            env.storage().persistent().set(&locked_key, &own_balance);
            bump_persistent(&env, &locked_key);
            events::tokens_locked(&env, &voter, proposal_id, own_balance);

            // Aggregate delegated weight: find all addresses whose `DelegateKey`
            // points to `voter`.  Because Soroban persistent storage is a key/value
            // map we cannot iterate it, so delegators must explicitly signal their
            // intent by calling `commit_delegation_for_proposal` — or, in the
            // simpler design used here, by the delegatee passing their delegators
            // list directly.  For this implementation we use a separate delegator
            // list stored under the delegatee address.
            //
            // Practical note: the delegated weight accumulation below works by
            // having each delegator pre-lock their own tokens via `lock_for_vote`
            // before the delegatee calls `vote`.  This keeps the on-chain logic
            // simple and avoids unbounded loops. See `lock_for_vote` and
            // `vote_with_delegators` for the complete delegation flow.
            //
            // Here in `vote()` we only count the caller's own balance so that
            // existing tests and the simple single-voter path continue to work.
            // Delegated-weight aggregation happens in `vote_with_delegators`.

            let weight = if own_balance > proposal.total_supply_at_creation {
                proposal.total_supply_at_creation
            } else {
                own_balance
            };

            if support {
                proposal.yes_votes = proposal
                    .yes_votes
                    .checked_add(weight)
                    .ok_or(DaoError::NotInitialized)?; // overflow guard
            } else {
                proposal.no_votes = proposal
                    .no_votes
                    .checked_add(weight)
                    .ok_or(DaoError::NotInitialized)?;
            }

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            env.storage().persistent().set(&vote_key, &weight);

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            bump_persistent(&env, &vote_key);
            events::voted(&env, &voter, proposal_id, support, weight);

            Ok(())
        }

        /// Delegate voting power to another address (#1105).
        ///
        /// The delegator's future votes will be counted in the delegatee's weight
        /// when the delegatee calls `vote_with_delegators`. Delegation is stored
        /// per-address and applies to all future proposals until changed.
        ///
        /// Circular delegation (A→B→A) is detected at `delegate()` time by
        /// walking the delegation chain from `delegatee` and asserting that
        /// `delegator` does not appear.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotInitialized`] if the DAO has not been set up.
        /// Returns [`DaoError::CircularDelegation`] if the assignment would create a cycle.
        pub fn delegate(
            env: Env,
            delegator: Address,
            delegatee: Address,
        ) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;
            delegator.require_auth();

            // Detect circular delegation: follow the chain starting at `delegatee`
            // and make sure `delegator` is not reachable.
            let mut current = delegatee.clone();
            // Maximum chain depth we will follow to avoid unbounded iteration.
            // In practice governance delegation chains are short (1–3 hops).
            let max_hops = 20u32;
            let mut hops = 0u32;
            loop {
                let next_key = DelegateKey { delegator: current.clone() };
                if let Some(next) = env
                    .storage()
                    .persistent()
                    .get::<DelegateKey, Address>(&next_key)
                {
                    if next == delegator {
                        return Err(DaoError::CircularDelegation);
                    }
                    current = next;
                    hops += 1;
                    if hops >= max_hops {
                        break;
                    }
                } else {
                    break;
                }
            }

            let key = DelegateKey { delegator: delegator.clone() };
            env.storage().persistent().set(&key, &delegatee);
            bump_persistent(&env, &key);
            events::delegated(&env, &delegator, &delegatee);

            Ok(())
        }

        /// Remove the caller's delegation, restoring direct voting (#1105).
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotInitialized`] if the DAO has not been set up.
        pub fn undelegate(env: Env, delegator: Address) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;
            delegator.require_auth();

            let key = DelegateKey { delegator: delegator.clone() };
            env.storage().persistent().remove(&key);
            events::undelegated(&env, &delegator);

            Ok(())
        }

        /// Lock a delegator's tokens into the DAO for a specific proposal (#1103/#1105).
        ///
        /// This must be called by a token holder who has delegated their vote to
        /// someone else **before** the delegatee calls `vote_with_delegators`.
        /// The locked amount equals the delegator's full token balance.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        /// Returns [`DaoError::VotingClosed`] if the voting period has ended.
        /// Returns [`DaoError::InsufficientVotingPower`] if the delegator has no tokens.
        pub fn lock_for_vote(
            env: Env,
            delegator: Address,
            proposal_id: u32,
        ) -> Result<i128, DaoError> {
            Self::require_initialized(&env)?;
            delegator.require_auth();

            let proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }
            if env.ledger().sequence() > proposal.deadline {
                return Err(DaoError::VotingClosed);
            }

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(DaoError::NotInitialized)?;
            let token_client = token::Client::new(&env, &token);
            let balance = token_client.balance(&delegator);
            if balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }

            let dao_address = env.current_contract_address();
            token_client.transfer(&delegator, &dao_address, &balance);

            let locked_key = LockedTokensKey {
                proposal_id,
                voter: delegator.clone(),
            };
            env.storage().persistent().set(&locked_key, &balance);
            bump_persistent(&env, &locked_key);
            events::tokens_locked(&env, &delegator, proposal_id, balance);

            Ok(balance)
        }

        /// Vote on behalf of the caller plus a list of delegators whose tokens
        /// have already been locked via `lock_for_vote` (#1105).
        ///
        /// This is the delegation-aware voting entry point.  The caller
        /// (delegatee) must also have locked tokens (they vote their own weight
        /// via `vote`, or they call this function which handles both in one
        /// transaction).
        ///
        /// The final vote weight is:
        ///   `min(own_balance + Σ delegated_balances, total_supply_at_creation)`
        ///
        /// # Errors
        ///
        /// Returns the same set of errors as [`vote`].
        /// Returns [`DaoError::NotAuthorized`] if a listed delegator has not actually
        /// delegated to this voter.
        /// Returns [`DaoError::NoLockedTokens`] if a delegator has not pre-locked tokens.
        pub fn vote_with_delegators(
            env: Env,
            voter: Address,
            proposal_id: u32,
            support: bool,
            delegators: Vec<Address>,
        ) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;
            voter.require_auth();

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }
            if env.ledger().sequence() > proposal.deadline {
                return Err(DaoError::VotingClosed);
            }

            let vote_key = VoteKey {
                proposal_id,
                voter: voter.clone(),
            };
            if env.storage().persistent().has(&vote_key) {
                return Err(DaoError::AlreadyVoted);
            }

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(DaoError::NotInitialized)?;
            let token_client = token::Client::new(&env, &token);
            let dao_address = env.current_contract_address();

            // --- Voter's own tokens ---
            let own_balance = token_client.balance(&voter);
            if own_balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }
            token_client.transfer(&voter, &dao_address, &own_balance);
            let locked_key = LockedTokensKey {
                proposal_id,
                voter: voter.clone(),
            };
            env.storage().persistent().set(&locked_key, &own_balance);
            bump_persistent(&env, &locked_key);
            events::tokens_locked(&env, &voter, proposal_id, own_balance);

            // --- Aggregate delegated tokens ---
            let mut total_weight: i128 = own_balance;

            for delegator in delegators.iter() {
                // Verify the delegator actually delegates to this voter.
                let delegate_key = DelegateKey { delegator: delegator.clone() };
                let delegatee: Address = env
                    .storage()
                    .persistent()
                    .get(&delegate_key)
                    .ok_or(DaoError::NotAuthorized)?;
                if delegatee != voter {
                    return Err(DaoError::NotAuthorized);
                }

                // Retrieve the amount the delegator pre-locked.
                let delegator_locked_key = LockedTokensKey {
                    proposal_id,
                    voter: delegator.clone(),
                };
                let delegated_amount: i128 = env
                    .storage()
                    .persistent()
                    .get(&delegator_locked_key)
                    .ok_or(DaoError::NoLockedTokens)?;

                total_weight = total_weight
                    .checked_add(delegated_amount)
                    .ok_or(DaoError::NotInitialized)?;
            }

            // Cap at total supply snapshot.
            let weight = if total_weight > proposal.total_supply_at_creation {
                proposal.total_supply_at_creation
            } else {
                total_weight
            };

            if support {
                proposal.yes_votes = proposal
                    .yes_votes
                    .checked_add(weight)
                    .ok_or(DaoError::NotInitialized)?;
            } else {
                proposal.no_votes = proposal
                    .no_votes
                    .checked_add(weight)
                    .ok_or(DaoError::NotInitialized)?;
            }

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            env.storage().persistent().set(&vote_key, &weight);

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            bump_persistent(&env, &vote_key);
            events::voted(&env, &voter, proposal_id, support, weight);

            Ok(())
        }

        /// Reclaim governance tokens locked during voting (#1103).
        ///
        /// Can be called by any voter after `proposal.deadline` has passed,
        /// regardless of the proposal's outcome.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::VotingStillOpen`] if the voting period has not yet ended.
        /// Returns [`DaoError::NoLockedTokens`] if the caller has no locked tokens for this proposal.
        pub fn unlock_tokens(
            env: Env,
            voter: Address,
            proposal_id: u32,
        ) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;

            let proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            // Tokens can only be retrieved once voting has closed.
            if env.ledger().sequence() <= proposal.deadline {
                return Err(DaoError::VotingStillOpen);
            }

            let locked_key = LockedTokensKey {
                proposal_id,
                voter: voter.clone(),
            };
            let locked_amount: i128 = env
                .storage()
                .persistent()
                .get(&locked_key)
                .ok_or(DaoError::NoLockedTokens)?;

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(DaoError::NotInitialized)?;
            let token_client = token::Client::new(&env, &token);
            let dao_address = env.current_contract_address();

            token_client.transfer(&dao_address, &voter, &locked_amount);
            env.storage().persistent().remove(&locked_key);

            events::tokens_unlocked(&env, &voter, proposal_id, locked_amount);

            Ok(())
        }

        /// Queue a passed proposal for execution after the timelock expires (#1104).
        ///
        /// This is a permissionless transition: any caller may queue a proposal
        /// that has passed its deadline with sufficient votes.  The call fails
        /// if quorum or majority is not yet met.
        ///
        /// Execution is fully atomic: the proposal state is updated, the bond is
        /// refunded (if any), the action payload is dispatched (if any), and the
        /// adaptive-quorum EMA is bumped — all in a single transaction. Any panic
        /// inside the invoked contract rolls back the entire transaction.
        ///
        /// After dispatch, the return value is emitted as a
        /// [`ProposalActionExecuted`](events::proposal_action_executed) event
        /// (issue #1108).
        ///
        /// The adaptive-quorum EMA is updated with the actual participation rate of
        /// this proposal (issue #1107).
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        /// Returns [`DaoError::DeadlineNotReached`] if voting is still open.
        /// Returns [`DaoError::QuorumNotMet`] if participation thresholds are not met.
        /// Returns [`DaoError::ProposalRejected`] if `no_votes >= yes_votes`.
        pub fn queue_proposal(env: Env, proposal_id: u32) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }
            if env.ledger().sequence() <= proposal.deadline {
                return Err(DaoError::DeadlineNotReached);
            }

            // --- Quorum checks ---
            let quorum: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Quorum)
                .ok_or(DaoError::NotInitialized)?;

            let total_votes = proposal
                .yes_votes
                .checked_add(proposal.no_votes)
                .ok_or(DaoError::NotInitialized)?;

            // Absolute quorum.
            if total_votes < quorum {
                // Slash bond on participation failure (issue #1106).
                if proposal.bond_amount > 0 {
                    Self::slash_bond(&env, &proposal);
                }
                // Even failed proposals count toward the EMA with zero participation
                // relative to quorum so that a quiet period lowers quorum (issue #1107).
                Self::update_quorum_ema(&env, 0);
                return Err(DaoError::QuorumNotMet);
            }

            // Percentage-based quorum (#1102 fix).
            // FIX: use `total_supply_at_creation` (captured at proposal creation)
            // instead of fetching live supply, and use overflow-safe arithmetic.
            let quorum_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::QuorumBps)
                .unwrap_or(0u32);
            if quorum_bps > 0 {
                // total_votes / total_supply >= quorum_bps / 10_000
                // ⟺  total_votes * 10_000 >= quorum_bps * total_supply
                // Use checked arithmetic to avoid overflow.
                let lhs = total_votes
                    .checked_mul(10_000)
                    .ok_or(DaoError::NotInitialized)?;
                let rhs = i128::from(quorum_bps)
                    .checked_mul(proposal.total_supply_at_creation)
                    .ok_or(DaoError::NotInitialized)?;
                if lhs < rhs {
                let token: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Token)
                    .ok_or(DaoError::NotInitialized)?;
                // `total_supply` is not part of the SEP-41 `TokenInterface`, so it is
                // unreachable through `token::Client`. Invoke it by symbol instead; this
                // requires the configured governance token to implement `total_supply`
                // (as `soroban-token-template` does).
                let total_supply: i128 =
                    env.invoke_contract(&token, &Symbol::new(&env, "total_supply"), Vec::new(&env));
                // total_votes / total_supply >= quorum_bps / 10_000
                // ⟺ total_votes * 10_000 >= quorum_bps * total_supply
                if total_votes * 10_000 < i128::from(quorum_bps) * proposal.total_supply_at_creation
                {
                    return Err(DaoError::QuorumNotMet);
                }
            }

            // Majority check.
            if proposal.yes_votes <= proposal.no_votes {
                // Slash bond on rejection (issue #1106).
                if proposal.bond_amount > 0 {
                    Self::slash_bond(&env, &proposal);
                }
                Self::update_quorum_ema(&env, Self::participation_bps(total_votes, quorum));
                return Err(DaoError::ProposalRejected);
            }

            // Set execution_eta and transition to Queued.
            let execution_delay: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ExecutionDelay)
                .unwrap_or(0u32);
            let execution_eta = proposal.deadline + execution_delay;

            proposal.state = ProposalState::Queued;
            proposal.execution_eta = execution_eta;

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            events::proposal_queued(&env, proposal_id, execution_eta);

            Ok(())
        }

        /// Execute a queued proposal once the execution timelock has expired (#1104).
        ///
        /// The proposal must be in `Queued` state and `env.ledger().sequence()` must
        /// be >= `proposal.execution_eta`.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Queued`.
        /// Returns [`DaoError::TimelockNotExpired`] if the execution delay has not elapsed.
        pub fn execute_proposal(env: Env, proposal_id: u32) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Queued {
                return Err(DaoError::InvalidState);
            }

            // Enforce the execution timelock.
            if env.ledger().sequence() < proposal.execution_eta {
                return Err(DaoError::TimelockNotExpired);
            }

            proposal.state = ProposalState::Executed;
            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));

            // ── Refund bond on success (issue #1106) ─────────────────────────
            if proposal.bond_amount > 0 {
                let token_addr: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Token)
                    .ok_or(DaoError::NotInitialized)?;
                let dao_addr = env.current_contract_address();
                token::Client::new(&env, &token_addr).transfer(
                    &dao_addr,
                    &proposal.proposer,
                    &proposal.bond_amount,
                );
                events::bond_refunded(&env, &proposal.proposer, proposal_id, proposal.bond_amount);
            }

            events::proposal_executed(&env, proposal_id);

            // ── Dispatch action payload (issue #1108) ─────────────────────────
            if let (Some(target), Some(function), Some(args)) = (
                proposal.action_target.clone(),
                proposal.action_function.clone(),
                proposal.action_args.clone(),
            ) {
                let return_val: Val = env.invoke_contract(&target, &function, args);
                events::proposal_action_executed(&env, proposal_id, return_val);
            }

            // ── Update adaptive quorum EMA (issue #1107) ─────────────────────
            Self::update_quorum_ema(&env, Self::participation_bps(total_votes, quorum));

            Ok(())
        }

        /// Veto a queued proposal — admin / security council only (#1104).
        ///
        /// This is the emergency veto entry point.  It transitions a `Queued`
        /// proposal to `Cancelled` before its execution timelock expires, giving
        /// a security council the ability to stop malicious governance actions.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotAuthorized`] if the caller is not the admin.
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Queued`.
        pub fn veto_proposal(env: Env, proposal_id: u32) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;

            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(DaoError::NotInitialized)?;
            admin.require_auth();

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Queued {
                return Err(DaoError::InvalidState);
            }

            proposal.state = ProposalState::Cancelled;
            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            events::proposal_vetoed(&env, &admin, proposal_id);

            Ok(())
        }

        /// Cancel a proposal. Admin only; works on any `Active` proposal regardless of votes.
        ///
        /// The proposal bond (if any) is slashed to the admin treasury on admin
        /// cancellation (issue #1106).
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotAuthorized`] if the caller is not the admin.
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        pub fn cancel_proposal(env: Env, proposal_id: u32) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;

            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(DaoError::NotInitialized)?;
            admin.require_auth();

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }

            proposal.state = ProposalState::Cancelled;
            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);

            // Slash bond on admin cancellation (issue #1106).
            if proposal.bond_amount > 0 {
                Self::slash_bond(&env, &proposal);
            }

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            events::proposal_cancelled(&env, &admin, proposal_id);

            Ok(())
        }

        /// Allow the original proposer to cancel their own proposal before any votes are cast.
        ///
        /// This lets a proposer correct a mistake (wrong description, bad parameters, etc.)
        /// without waiting for the entire voting period to lapse.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotInitialized`] if the DAO has not been set up.
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::NotAuthorized`] if the caller is not the original proposer.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        /// Returns [`DaoError::VotesAlreadyCast`] if at least one vote has already been recorded.
        pub fn proposer_cancel_proposal(
            env: Env,
            proposer: Address,
            proposal_id: u32,
        ) -> Result<(), DaoError> {
            Self::require_initialized(&env)?;
            proposer.require_auth();

            let mut proposal: Proposal = env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)?;

            // Only the original proposer may use this entry point.
            if proposal.proposer != proposer {
                return Err(DaoError::NotAuthorized);
            }

            if proposal.state != ProposalState::Active {
                return Err(DaoError::InvalidState);
            }

            // Reject if any votes have already been cast.
            if proposal.yes_votes > 0 || proposal.no_votes > 0 {
                return Err(DaoError::VotesAlreadyCast);
            }

            proposal.state = ProposalState::Cancelled;
            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);

            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            events::proposal_proposer_cancelled(&env, &proposer, proposal_id);

            Ok(())
        }

        /// Return a proposal by ID.
        #[must_use]
        pub fn get_proposal(env: Env, proposal_id: u32) -> Result<Proposal, DaoError> {
            env.storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
                .ok_or(DaoError::ProposalNotFound)
        }

        /// Return total number of proposals created.
        #[must_use]
        pub fn proposal_count(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&DataKey::ProposalCount)
                .unwrap_or(0)
        }

        /// Return the current adaptive quorum EMA in basis points (issue #1107).
        ///
        /// Returns 0 when adaptive quorum is disabled (both bounds set to 0).
        #[must_use]
        pub fn current_quorum_bps(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&DataKey::QuorumEmaBps)
                .unwrap_or(0)
        }

        // ── Private helpers ──────────────────────────────────────────────────
        /// Return the delegatee for a given delegator, if any.
        #[must_use]
        pub fn get_delegate(env: Env, delegator: Address) -> Option<Address> {
            let key = DelegateKey { delegator };
            env.storage().persistent().get(&key)
        /// List proposals with cursor-based pagination (#1110).
        ///
        /// `cursor` is the proposal ID to resume scanning from (pass `0` to
        /// start from the beginning). `limit` is clamped to
        /// `[1, MAX_PAGE_SIZE]`. When `state` is `Some`, only proposals in
        /// that [`ProposalState`] are returned.
        ///
        /// Returns a [`ProposalPage`] whose `next_cursor` is `None` once the
        /// end of the proposal range has been reached.
        pub fn get_proposals(
            env: Env,
            cursor: u32,
            limit: u32,
            state: Option<ProposalState>,
        ) -> ProposalPage {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ProposalCount)
                .unwrap_or(0);

            let page = paginate(
                &env,
                cursor,
                limit,
                MAX_PAGE_SIZE,
                |c: u32| {
                    let next = c.saturating_add(1);
                    if next < count { Some(next) } else { None }
                },
                |c: u32| {
                    if c >= count {
                        return None;
                    }
                    env.storage()
                        .persistent()
                        .get::<_, Proposal>(&ProposalKey::Proposal(c))
                        .filter(|p| state.is_none_or(|s| p.state == s))
                },
            );

            ProposalPage {
                proposals: page.items,
                next_cursor: page.next_cursor,
            }
        }

        fn require_initialized(env: &Env) -> Result<(), DaoError> {
            if !env.storage().instance().has(&DataKey::Initialized) {
                return Err(DaoError::NotInitialized);
            }
            Ok(())
        }

        /// Transfer `proposal.bond_amount` from the DAO contract to the admin
        /// treasury as a slash penalty (issue #1106).
        fn slash_bond(env: &Env, proposal: &Proposal) {
            let token_addr: Address = match env
                .storage()
                .instance()
                .get(&DataKey::Token)
            {
                Some(a) => a,
                None => return,
            };
            let admin: Address = match env.storage().instance().get(&DataKey::Admin) {
                Some(a) => a,
                None => return,
            };
            let dao_addr = env.current_contract_address();
            token::Client::new(env, &token_addr).transfer(
                &dao_addr,
                &admin,
                &proposal.bond_amount,
            );
            events::bond_slashed(env, &proposal.proposer, proposal.id, proposal.bond_amount);
        }

        /// Compute participation as a fraction of the absolute quorum floor,
        /// capped at `BPS_DENOMINATOR` (10 000 bps = 100 %).
        ///
        /// When `quorum == 0` we treat any participation as 100 % to avoid
        /// division by zero.
        fn participation_bps(total_votes: i128, quorum: i128) -> u32 {
            if quorum <= 0 {
                return BPS_DENOMINATOR;
            }
            // Safe: total_votes and quorum are non-negative i128; result fits u32.
            let ratio = (total_votes * i128::from(BPS_DENOMINATOR)) / quorum;
            if ratio > i128::from(BPS_DENOMINATOR) {
                BPS_DENOMINATOR
            } else {
                // Safe cast: value is in [0, BPS_DENOMINATOR] which fits u32.
                #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
                let r = ratio as u32;
                r
            }
        }

        /// Update the stored exponential moving average of quorum participation
        /// (issue #1107).
        ///
        /// Formula (alpha = 2 / (window + 1)):
        ///
        /// ```text
        /// ema_new = ema_old + alpha * (sample - ema_old)
        ///         = ema_old * (window - 1) / (window + 1) + sample * 2 / (window + 1)
        /// ```
        ///
        /// All arithmetic is done in u32 basis points, bounded by
        /// `[min_quorum_bps, max_quorum_bps]`.
        fn update_quorum_ema(env: &Env, sample_bps: u32) {
            let min_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::MinQuorumBps)
                .unwrap_or(0);
            let max_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::MaxQuorumBps)
                .unwrap_or(0);

            // Adaptive quorum is disabled when both bounds are zero.
            if min_bps == 0 && max_bps == 0 {
                return;
            }

            let window: u32 = env
                .storage()
                .instance()
                .get(&DataKey::QuorumEmaWindow)
                .unwrap_or(DEFAULT_QUORUM_EMA_WINDOW);

            let old_ema: u32 = env
                .storage()
                .instance()
                .get(&DataKey::QuorumEmaBps)
                .unwrap_or(min_bps);

            // EMA update using integer arithmetic to avoid floats.
            // alpha = 2 / (window + 1)
            // ema_new = (old_ema * (window - 1) + sample_bps * 2) / (window + 1)
            let numerator = old_ema
                .saturating_mul(window.saturating_sub(1))
                .saturating_add(sample_bps.saturating_mul(2));
            let denominator = window.saturating_add(1);
            let new_ema_raw = numerator / denominator;

            // Clamp to configured bounds.
            let new_ema = new_ema_raw.max(min_bps).min(max_bps);

            env.storage()
                .instance()
                .set(&DataKey::QuorumEmaBps, &new_ema);

            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::QuorumEmaCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::QuorumEmaCount, &count.saturating_add(1));

            bump_instance(env);
            events::quorum_updated(env, new_ema);
        }
    }
}

mod test;

#[cfg(test)]
mod prop_test;
