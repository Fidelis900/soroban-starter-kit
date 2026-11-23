// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Vec, contracttype};

/// A beneficiary's vesting schedule: linear-with-cliff, stepped tranches, or
/// milestone based.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BeneficiarySchedule {
    /// Total tokens allocated (reduced to the vested amount on revocation).
    pub amount: i128,
    /// Ledger at which linear vesting starts accruing.
    pub start_ledger: u32,
    /// Ledger before which nothing is claimable.
    pub cliff_ledger: u32,
    /// Ledger at which everything is vested.
    pub end_ledger: u32,
    /// Tokens already paid to the beneficiary.
    pub claimed: i128,
    /// Whether the schedule has been revoked.
    pub revoked: bool,
    /// Whether the admin may revoke this schedule.
    pub is_revocable: bool,
    /// Optional stepped tranche schedule as `(ledger_sequence, percentage_bps)`
    /// pairs. When non-empty, vesting is discrete instead of linear.
    pub tranches: Vec<(u32, u32)>,
    /// Milestones as `(milestone_id, weight_bps)`; empty unless this is a
    /// milestone schedule.
    pub milestones: Vec<(u32, u32)>,
    /// Approved milestone ids.
    pub approved: Vec<u32>,
    /// Address allowed to approve milestones.
    pub verifier: Option<Address>,
    /// Address authorized to release the whole schedule (oracle / multi-sig).
    pub milestone_oracle: Option<Address>,
    /// Whether the oracle has released the schedule (fully vests it).
    pub milestone_released: bool,
    /// Whether a revocation is pending its grace period.
    pub revocation_pending: bool,
    /// Ledger at which the revocation was requested (pending) or applied.
    pub revocation_ledger: u32,
}

/// Parameters for one schedule in a batch.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleParams {
    pub beneficiary: Address,
    pub start_ledger: u32,
    pub cliff_ledger: u32,
    pub end_ledger: u32,
    pub amount: i128,
    pub is_revocable: bool,
}

/// A pending beneficiary key rotation, executable once `eta_ledger` is reached.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingRotation {
    /// Address that will receive the schedule.
    pub new_beneficiary: Address,
    /// Who proposed the rotation (the current beneficiary or the admin).
    pub proposer: Address,
    /// Ledger at which the rotation becomes executable.
    pub eta_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Token contract address.
    Token,
    /// Vesting schedule for a specific beneficiary.
    Schedule(Address),
    /// Total tokens released early by admin (audit log).
    AdminReleased,
    /// Contract version number (`u32`).
    Version,
    /// Configurable revocation grace period in ledgers (`u32`).
    RevocationDelay,
    /// Minimum amount per claim, unless claiming the final remainder (`i128`).
    MinClaimAmount,
    /// Timelock, in ledgers, before a beneficiary rotation can execute (`u32`).
    RotationDelay,
    /// Pending beneficiary rotation keyed by the current beneficiary.
    PendingRotation(Address),
}

/// Snapshot returned by `get_info`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VestingInfo {
    pub start_ledger: u32,
    pub cliff_ledger: u32,
    pub end_ledger: u32,
    pub amount: i128,
    pub claimed: i128,
    pub revoked: bool,
    pub is_revocable: bool,
    pub revocation_pending: bool,
    pub tranches: Vec<(u32, u32)>,
    pub milestones: Vec<(u32, u32)>,
    pub approved: Vec<u32>,
}
