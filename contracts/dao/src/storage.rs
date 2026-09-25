// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, String, Symbol, Val, Vec, contracttype};
use soroban_sdk::{Address, String, Vec, contracttype};

/// Instance-storage keys (contract-level state).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    VotingPeriod,
    Quorum,
    /// Minimum participation expressed in basis points of total token supply
    /// (0–10_000). Zero means no percentage-based quorum is enforced.
    QuorumBps,
    ProposalCount,
    Initialized,
    /// Configurable bond amount escrowed per proposal (issue #1106).
    ProposalBond,
    /// Adaptive quorum parameters (issue #1107).
    MinQuorumBps,
    MaxQuorumBps,
    /// Number of past proposals tracked for EMA smoothing (issue #1107).
    QuorumEmaWindow,
    /// Current smoothed quorum BPS (0–10_000). Stored as u32.
    QuorumEmaBps,
    /// Number of proposals included in the EMA so far.
    QuorumEmaCount,
    /// Number of ledgers a passed proposal must wait in `Queued` state before
    /// it can be executed. Zero means immediate execution is allowed.
    ExecutionDelay,
}

/// Persistent-storage keys (per-proposal and per-vote data).
#[contracttype]
#[derive(Clone)]
pub enum ProposalKey {
    Proposal(u32),
}

/// Composite key for vote deduplication.
#[contracttype]
#[derive(Clone)]
pub struct VoteKey {
    pub proposal_id: u32,
    pub voter: Address,
}

/// Key for the amount of governance tokens a voter locked into the DAO for a
/// specific proposal. Stored in persistent storage.
#[contracttype]
#[derive(Clone)]
pub struct LockedTokensKey {
    pub proposal_id: u32,
    pub voter: Address,
}

/// Key for a delegator → delegatee mapping (liquid-democracy delegation).
/// Stored in persistent storage.
#[contracttype]
#[derive(Clone)]
pub struct DelegateKey {
    pub delegator: Address,
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProposalState {
    /// Voting is open.
    Active = 0,
    /// Voting closed and the proposal passed; waiting out the execution timelock.
    Queued = 1,
    /// Timelock expired — the proposal has been executed on-chain.
    Executed = 2,
    /// Cancelled by the admin, a security-council veto, or the original proposer.
    Cancelled = 3,
}

impl core::fmt::Display for ProposalState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ProposalState::Active => "active",
            ProposalState::Queued => "queued",
            ProposalState::Executed => "executed",
            ProposalState::Cancelled => "cancelled",
        })
    }
}

/// A governance proposal.
///
/// `action_target`, `action_function`, and `action_args` are optional; when
/// `action_target` is `Some`, `execute_proposal` will dispatch the call via
/// `env.invoke_contract` (issue #1108).
#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u32,
    pub proposer: Address,
    pub title: String,
    pub description: String,
    /// Last ledger at which votes may be cast.
    pub deadline: u32,
    pub yes_votes: i128,
    pub no_votes: i128,
    pub state: ProposalState,
    // ── Executable action payload (issue #1108) ──────────────────────────────
    /// Optional target contract to call on execution.
    pub action_target: Option<Address>,
    /// Function name to invoke on the target contract.
    pub action_function: Option<Symbol>,
    /// Arguments forwarded to the target function.
    pub action_args: Option<Vec<Val>>,
    // ── Bond tracking (issue #1106) ──────────────────────────────────────────
    /// Token amount escrowed by the proposer at submission time.
    pub bond_amount: i128,
    /// Total token supply at the time the proposal was created.
    /// Used for quorum calculation and to cap voter voting power,
    /// preventing flash-loan-style manipulation.
    pub total_supply_at_creation: i128,
    /// Earliest ledger at which this proposal may be executed once it is
    /// queued (= `deadline` + `execution_delay`). Zero when not yet queued.
    pub execution_eta: u32,
}

/// One page of results from `get_proposals` (#1110).
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProposalPage {
    /// Proposals found in this page, in ascending ID order.
    pub proposals: Vec<Proposal>,
    /// The cursor to pass to the next call to continue scanning, or `None`
    /// once the end of the proposal range has been reached.
    pub next_cursor: Option<u32>,
}
