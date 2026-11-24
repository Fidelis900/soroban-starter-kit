// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, BytesN, Symbol, Val, Vec, contracttype};

/// Represents a single tranche in a multi-tranche release schedule.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseTranche {
    /// Ledger sequence at which this tranche becomes available for release.
    pub release_ledger: u32,
    /// Amount of tokens in this tranche.
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Token contract address.
    Token,
    /// Beneficiary address.
    Beneficiary,
    /// Deprecated: single release ledger (kept for backwards compatibility).
    ReleaseLedger,
    /// Deprecated: single amount (kept for backwards compatibility).
    Amount,
    /// Timelock state.
    State,
    /// List of release tranches for multi-tranche releases.
    Tranches,
    /// Bitmask tracking which tranches have been released (index -> true/false).
    ReleasedTranches,
    /// Optional authorized staking contract for yield generation.
    StakingContract,
    /// Address that receives accrued yield on release.
    YieldRecipient,
    /// Whether the locked balance has been deposited into staking.
    Deposited,
    /// Principal currently deposited in the staking contract.
    StakedAmount,
    /// Individual release tranche stored in persistent storage (index -> tranche).
    Tranche(u32),
    /// Release flag for an individual tranche stored in persistent storage
    /// (index -> true/false).
    TrancheReleased(u32),
    /// Pending beneficiary address proposed for two-step transfer.
    PendingBeneficiary,
    /// Early release penalty in basis points.
    EarlyReleasePenaltyBps,
    /// Address that receives the early release penalty.
    PenaltyRecipient,
    /// Locked balance for a specific token (token address -> amount).
    LockedBalance(Address),
    /// List of release tranches for a specific token (token address -> tranches).
    TokenTranches(Address),
    /// Individual release tranche for a specific token
    /// (token address, index -> tranche).
    TokenTranche(Address, u32),
    /// Release flag for an individual tranche of a specific token
    /// (token address, index -> true/false).
    TokenTrancheReleased(Address, u32),
    /// Release ledger for a specific token (token address -> ledger).
    TokenReleaseLedger(Address),
    /// Number of distinct tokens tracked by this instance.
    TokenCount,
    /// Token address at a registry index (index -> token address).
    TokenAt(u32),
    /// Grace period (in ledgers) after a queued transaction's ETA during which
    /// it may still be executed.
    GracePeriod,
    /// Queued governance transaction (tx hash -> [`QueuedTransaction`]).
    QueuedTx(BytesN<32>),
}

/// Lifecycle state of a queued governance transaction.
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxState {
    /// Waiting for its ETA; may be executed or cancelled.
    Queued = 0,
    /// Executed successfully; terminal.
    Executed = 1,
    /// Cancelled by the governance admin; terminal.
    Cancelled = 2,
}

/// A governance transaction queued behind the timelock delay.
#[contracttype]
#[derive(Clone, Debug)]
pub struct QueuedTransaction {
    /// Contract to invoke on execution.
    pub target: Address,
    /// Function to invoke on `target`.
    pub function: Symbol,
    /// Arguments passed to `function`.
    pub args: Vec<Val>,
    /// Ledger at which the transaction was queued.
    pub queued_at: u32,
    /// Earliest ledger at which the transaction may execute (`queued_at + delay`).
    pub eta: u32,
    /// Current lifecycle state.
    pub state: TxState,
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimelockState {
    Active = 0,
    Released = 1,
    Cancelled = 2,
}

impl core::fmt::Display for TimelockState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            TimelockState::Active => "active",
            TimelockState::Released => "released",
            TimelockState::Cancelled => "cancelled",
        })
    }
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct TimelockInfo {
    pub admin: Address,
    pub token: Address,
    pub beneficiary: Address,
    /// Deprecated: single release ledger (for backwards compatibility).
    pub release_ledger: u32,
    /// Deprecated: single amount (for backwards compatibility).
    pub amount: i128,
    pub state: TimelockState,
    /// List of tranches (empty for legacy single-tranche timelocks).
    pub tranches: Vec<ReleaseTranche>,
    /// Optional authorized staking contract for yield generation.
    pub staking_contract: Option<Address>,
    /// Address that receives accrued yield on release.
    pub yield_recipient: Option<Address>,
    /// Whether the locked balance has been deposited into staking.
    pub deposited: bool,
    /// Principal currently deposited in the staking contract.
    pub staked_amount: i128,
}
