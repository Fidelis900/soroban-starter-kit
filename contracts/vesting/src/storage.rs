// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Vec, contracttype};

/// A beneficiary's vesting schedule (linear-with-cliff or milestone based).
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
    /// Whether the admin may revoke.
    pub is_revocable: bool,
    /// Milestones as `(milestone_id, weight_bps)`; empty for linear schedules.
    pub milestones: Vec<(u32, u32)>,
    /// Approved milestone ids.
    pub approved: Vec<u32>,
    /// Address allowed to approve milestones.
    pub verifier: Option<Address>,
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

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Token,
    Schedule(Address),
}
