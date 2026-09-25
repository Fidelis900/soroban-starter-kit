// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{contracttype, Address, Vec};

/// Stores all vesting schedule details for a single beneficiary and token.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BeneficiarySchedule {
    /// Token contract address for this schedule.
    pub token: Address,
    /// Total tokens to vest for this beneficiary.
    pub amount: i128,
    /// Ledger sequence at which vesting begins (cliff).
    pub cliff_ledger: u32,
    /// Ledger sequence at which all tokens are fully vested.
    pub end_ledger: u32,
    /// Tokens already claimed by the beneficiary.
    pub claimed: i128,
    /// Whether the schedule has been revoked by admin.
    pub revoked: bool,
    /// Optional stepped tranche schedule as `(ledger_sequence, percentage_bps)`
    /// pairs. When non-empty, vesting is discrete: each tranche releases its
    /// percentage (in basis points) of `amount` once `ledger_sequence` is
    /// reached. Percentages are expected to sum to 10_000 BPS.
    pub tranches: Vec<(u32, u32)>,
    /// Optional milestone release schedule as `(milestone_id, percentage_bps)`
    /// pairs. Each milestone releases its percentage of `amount` once the
    /// authorized oracle or multi-sig verifies the deliverable.
    pub milestones: Vec<(u32, u32)>,
    /// Milestone ids that have been verified and released.
    pub released_milestones: Vec<u32>,
    /// Ledger at which a pending revocation was requested (0 if none).
    pub revocation_requested_ledger: u32,
    /// Ledger at which a pending revocation finalizes (0 if none).
    pub revocation_finalize_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Token contract address.
    Token,
    /// Vesting schedule for a specific beneficiary and token.
    Schedule(Address, Address),
    /// Total tokens released early by admin (audit log).
    AdminReleased,
    /// Contract version number (`u32`).
    Version,
    /// Address authorized to verify milestones (oracle / multi-sig).
    MilestoneOracle,
    /// Configurable revocation grace period in ledgers (`u32`).
    RevocationDelay,
}

/// Snapshot returned by `get_info`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VestingInfo {
    pub token: Address,
    pub cliff_ledger: u32,
    pub end_ledger: u32,
    pub amount: i128,
    pub claimed: i128,
    pub revoked: bool,
    pub tranches: Vec<(u32, u32)>,
    pub milestones: Vec<(u32, u32)>,
    pub released_milestones: Vec<u32>,
}
    pub revocation_requested_ledger: u32,
    pub revocation_finalize_ledger: u32,
}
