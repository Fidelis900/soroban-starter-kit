Looking at the current file, I can see it has significant corruption (duplicate imports, syntax errors, orphaned code blocks). I need to produce a complete, working file that implements all four features while preserving existing functionality.

Let me carefully reconstruct the file with all four features implemented:

```rust
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
//! * **Quadratic voting** (issue #1316) — proposals may opt into quadratic
//!   vote weighting, where a voter's weight is `fixed_sqrt(balance)` rather
//!   than the raw token balance, reducing whale dominance.
//! * **Multi-choice voting** (issue #1317) — proposals may define a list of
//!   `choices`; voters allocate weight across choices and the winner is the
//!   choice with the highest tally.
//! * **Tiered execution timelock** (issue #1318) — proposals are classified
//!   into `Standard`, `Financial`, or `Critical` tiers, each with its own
//!   execution delay so high-impact actions wait longer.
//! * **Duplicate payload rejection** (issue #1319) — proposals are hashed
//!   over their payload; creating a proposal identical to an active one is
//!   rejected with `DaoError::DuplicateProposal`.
//!
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
//! tier_delay(tier)`).  The admin / security council may veto a queued
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

use soroban_sdk::{Address, Bytes, Env, String, Symbol, Val, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::DaoError;
pub use storage::{
    ChoiceTally, DataKey, DelegateKey, LockedTokensKey, Proposal, ProposalKey, ProposalPage,
    ProposalState, ProposalTier, VoteKey, VotingType,
};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, paginate};

/// Maximum number of proposals returned by a single `get_proposals` call.
pub const MAX_PAGE_SIZE: u32 = 50;

/// Default execution delay (in ledgers) for `ProposalTier::Standard` when no
/// tier-specific delay has been configured. 24 hours at 5s/ledger ≈ 17 280.
pub const DEFAULT_STANDARD_DELAY: u32 = 17_280;
/// Default execution delay (in ledgers) for `ProposalTier::Financial`. 72 hours.
pub const DEFAULT_FINANCIAL_DELAY: u32 = 51_840;
/// Default execution delay (in ledgers) for `ProposalTier::Critical`. 7 days.
pub const DEFAULT_CRITICAL_DELAY: u32 = 120_960;

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
///   be executed after the tier-specific delay, giving the community time to
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
        /// - `quorum_bps`      — minimum participation as basis points of total supply (0–10 000).
        ///                       Pass `0` to disable the percentage-based quorum check.
        /// - `execution_delay` — number of ledgers a passed proposal must wait in `Queued` state
        ///                       before it can be executed. Used as the default `Standard` tier delay.
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
            env.storage()
                .instance()
                .set(&DataKey::QuorumBps, &quorum_bps);
            env.storage()
                .instance()
                .set(&DataKey::ExecutionDelay, &execution_delay);
            // Tier-specific delays default to the standard delay for Standard,
            // and to the longer defaults for Financial / Critical (issue #1318).
            env.storage()
                .instance()
                .set(&DataKey::StandardDelay, &execution_delay);
            env.storage()
                .instance()
                .set(&DataKey::FinancialDelay, &DEFAULT_FINANCIAL_DELAY);
            env.storage()
                .instance()
                .set(&DataKey::CriticalDelay, &DEFAULT_CRITICAL_DELAY);
            env.storage().instance().set(&DataKey::ProposalCount, &0u32);
            env.storage().instance().set(&DataKey::Initialized, &true);

            bump_instance(&env);
            events::initialized(&env, &admin, &token, quorum);

            Ok(())
        }

        /// Create a new proposal with the legacy (binary, standard-tier) config.
        ///
        /// This is a thin wrapper around [`create_proposal_with_config`] that
        /// preserves the original signature for backwards compatibility.
        ///
        /// # Errors
        ///
        /// See [`create_proposal_with_config`].
        pub fn create_proposal(
            env: Env,
            proposer: Address,
            title: String,
            description: String,
            action_target: Option<Address>,
            action_function: Option<Symbol>,
            action_args: Option<Vec<Val>>,
        ) -> Result<u32, DaoError> {
            Self::create_proposal_with_config(
                env,
                proposer,
                title,
                description,
                action_target,
                action_function,
                action_args,
                VotingType::Linear,
                ProposalTier::Standard,
                Vec::new(&env),
            )
        }

        /// Create a new proposal with full configuration.
        ///
        /// The proposer must hold > 0 governance tokens. When `proposal_bond > 0`,
        /// exactly `proposal_bond` tokens are transferred from `proposer` to the
        /// DAO contract as an anti-spam measure (issue #1106).
        ///
        /// - `voting_type` — `Linear` (1 token = 1 vote) or `Quadratic`
        ///   (weight = `fixed_sqrt(balance)`) — issue #1316.
        /// - `tier` — `Standard`, `Financial`, or `Critical`; determines the
        ///   execution timelock — issue #1318.
        /// - `choices` — optional list of choice symbols for multi-choice
        ///   voting — issue #1317. When empty, the proposal is binary.
        ///
        /// A SHA-256 hash of the payload is computed and checked against active
        /// proposals; duplicates are rejected with [`DaoError::DuplicateProposal`]
        /// (issue #1319).
        ///
        /// Returns the newly created `proposal_id`.
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::NotInitialized`] if the DAO has not been set up.
        /// Returns [`DaoError::InsufficientVotingPower`] if the proposer has no tokens.
        /// Returns [`DaoError::InsufficientBondBalance`] if the proposer cannot cover
        ///   the proposal bond.
        /// Returns [`DaoError::DuplicateProposal`] if an active proposal with the
        ///   same payload hash already exists.
        pub fn create_proposal_with_config(
            env: Env,
            proposer: Address,
            title: String,
            description: String,
            action_target: Option<Address>,
            action_function: Option<Symbol>,
            action_args: Option<Vec<Val>>,
            voting_type: VotingType,
            tier: ProposalTier,
            choices: Vec<Symbol>,
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
            if balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }

            // ── Duplicate payload rejection (issue #1319) ────────────────────
            let payload_hash = Self::compute_payload_hash(
                &env,
                &title,
                &description,
                &action_target,
                &action_function,
                &action_args,
            );
            let hash_key = ProposalKey::ActivePayloadHash(payload_hash.clone());
            if env.storage().persistent().has(&hash_key) {
                return Err(DaoError::DuplicateProposal);
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
                token_client.transfer(&proposer, &dao_addr, &proposal_bond);
            }

            // FIX #1102: Query the real total supply from the token contract at
            // proposal creation time instead of using the i128::MAX sentinel.
            let total_supply_at_creation: i128 = env.invoke_contract(
                &token_addr,
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
                voting_type,
                tier,
                choices,
                payload_hash: payload_hash.clone(),
            };

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            env.storage()
                .instance()
                .set(&DataKey::ProposalCount, &(count + 1));

            // Track the active payload hash so future duplicates are rejected.
            env.storage().persistent().set(&hash_key, &proposal_id);

            bump_instance(&env);
            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));
            bump_persistent(&env, &hash_key);
            events::proposal_created(&env, &proposer, proposal_id);

            Ok(proposal_id)
        }

        /// Cast a vote on an active proposal.
        ///
        /// The voter's governance tokens are **transferred into this contract** for
        /// the duration of the voting window, preventing flash-loan attacks (#1103).
        /// Tokens are returned via [`unlock_tokens`] after `proposal.deadline`.
        ///
        /// If the proposal uses [`VotingType::Quadratic`], the vote weight is
        /// `fixed_sqrt(balance)` rather than the raw balance (issue #1316).
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

            // Cap at total supply snapshot first, then apply quadratic weighting.
            let capped = if own_balance > proposal.total_supply_at_creation {
                proposal.total_supply_at_creation
            } else {
                own_balance
            };
            let weight = Self::apply_voting_type(&env, proposal.voting_type, capped);

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

        /// Vote for a specific choice on a multi-choice proposal (issue #1317).
        ///
        /// The voter's tokens are locked into the DAO contract for the duration
        /// of the voting window (same anti-flash-loan mechanism as [`vote`]).
        /// The vote weight is computed according to the proposal's
        /// [`VotingType`] (linear or quadratic) and added to the matching
        /// [`ChoiceTally`].
        ///
        /// # Errors
        ///
        /// Returns [`DaoError::ProposalNotFound`] if the proposal does not exist.
        /// Returns [`DaoError::InvalidState`] if the proposal is not `Active`.
        /// Returns [`DaoError::VotingClosed`] if the voting period has ended.
        /// Returns [`DaoError::AlreadyVoted`] if the voter has already voted.
        /// Returns [`DaoError::InsufficientVotingPower`] if the voter has no tokens.
        /// Returns [`DaoError::InvalidChoice`] if `choice` is not in the proposal's choices.
        pub fn vote_choice(
            env: Env,
            voter: Address,
            proposal_id: u32,
            choice: Symbol,
            amount: i128,
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

            // Validate the choice exists on this proposal.
            let mut found = false;
            for c in proposal.choices.iter() {
                if c == choice {
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(DaoError::InvalidChoice);
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

            let own_balance = token_client.balance(&voter);
            if own_balance <= 0 {
                return Err(DaoError::InsufficientVotingPower);
            }

            // Lock the voter's tokens.
            token_client.transfer(&voter, &dao_address, &own_balance);
            let locked_key = LockedTokensKey {
                proposal_id,
                voter: voter.clone(),
            };
            env.storage().persistent().set(&locked_key, &own_balance);
            bump_persistent(&env, &locked_key);
            events::tokens_locked(&env, &voter, proposal_id, own_balance);

            // Determine the base amount to weight: `amount` if positive and
            // within the voter's balance, otherwise the full balance.
            let base = if amount > 0 && amount <= own_balance {
                amount
            } else {
                own_balance
            };
            let capped = if base > proposal.total_supply_at_creation {
                proposal.total_supply_at_creation
            } else {
                base
            };
            let weight = Self::apply_voting_type(&env, proposal.voting_type, capped);

            // Update the matching choice tally.
            let tally_key = ProposalKey::ChoiceTally(proposal_id, choice.clone());
            let mut tally: ChoiceTally = env
                .storage()
                .persistent()
                .get(&tally_key)
                .unwrap_or(ChoiceTally {
                    choice: choice.clone(),
                    votes: 0,
                });
            tally.votes = tally
                .votes
                .checked_add(weight)
                .ok_or(DaoError::NotInitialized)?;
            env.storage().persistent().set(&tally_key, &tally);
            bump_persistent(&env, &tally_key);

            // Also record the vote to prevent double-voting.
            env.storage().persistent().set(&vote_key, &weight);
            bump_persistent(&env, &vote_key);

            // Keep the proposal's aggregate tally in sync so quorum checks work.
            proposal.yes_votes = proposal
                .yes_votes
                .checked_add(weight)
                .ok_or(DaoError::NotInitialized)?;

            env.storage()
                .persistent()
                .set(&ProposalKey::Proposal(proposal_id), &proposal);
            bump_persistent(&env, &ProposalKey::Proposal(proposal_id));

            events::voted(&env, &voter, proposal_id, true, weight);

            Ok(())
        }

        /// Return the per-choice tallies for a multi-choice proposal (issue #1317).
        ///
        /// Returns a `Vec<ChoiceTally>` in the same order as `proposal.choices`.
        /// For binary proposals (empty `choices`) this returns an empty vector.
        #[must_use]
        pub fn get_proposal_choice_tallies(
            env: Env,
            proposal_id: u32,
        ) -> Vec<ChoiceTally> {
            let mut result = Vec::new(&env);
            let proposal: Proposal = match env
                .storage()
                .persistent()
                .get(&ProposalKey::Proposal(proposal_id))
            {
                Some(p) => p,
                None => return result,
            };
            for choice in proposal.choices.iter() {
                let tally_key = ProposalKey::ChoiceTally(proposal_id, choice.clone());
                let tally: ChoiceTally = env
                    .storage()
                    .persistent()
                    .get(&tally_key)
                    .unwrap_or(ChoiceTally {
                        choice: choice.clone(),
                        votes: 0,
                    });
                result.push_back(tally);
            }
            result
        }

        /// Return the winning choice for a multi-choice proposal (issue #1317).
        ///
        /// Returns the choice with the highest tally, or `None` if the proposal
        /// has no choices or no votes have been cast. Ties resolve to the first
        /// choice in declaration order.
        #[must_use]
        pub fn get_proposal_winner(env: Env, proposal_id: u32) -> Option<Symbol> {
            let tallies = Self::get_proposal_choice_tallies(env, proposal_id);
            let mut best: Option<ChoiceTally> = None;
            for tally in tallies.iter() {
                if tally.votes <= 0 {
                    continue;
                }
                match &best {
                    Some(b) if b.votes >= tally.votes => {}
                    _ => best = Some(tally),
                }
            }
            best.map(|t| t.choice)
        }

        /// Delegate voting power to another address (#1105).
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

            let mut current = delegatee.clone();
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
            events::tokens_locked(&env, &voter, proposal_id,