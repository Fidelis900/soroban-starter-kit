// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Merkle root as a 32-byte value stored as Bytes, keyed by campaign epoch
    /// (instance). Epoch 0 is the root set by `set_root`; governance updates
    /// install later epochs while keeping prior ones for history.
    /// Current Merkle root as 32-byte Bytes (instance).
    MerkleRoot,
    /// Epoch-stamped Merkle root written by governance (instance). Key is the epoch number.
    MerkleRootEpoch(u32),
    /// Current Merkle root as a 32-byte value stored as Bytes (instance).
    MerkleRoot,
    /// Per-epoch Merkle root keyed by epoch number (instance).
    MerkleRoot(u32),
    /// Whether `(recipient, token)` has already been claimed (persistent).
    Claimed(Address, Address),
    /// Tier id under which `(recipient, token)` was claimed (persistent, `u32`).
    ClaimedTier(Address, Address),
    /// The ledger sequence number after which claims are rejected (instance).
    ClaimDeadline,
    /// Optional [`VestingConfig`] applied to every claim (instance).
    VestingConfig,
    /// [`VestingSchedule`] for `(recipient, token)` (persistent).
    Vesting(Address, Address),
    /// Total still-locked vesting balance of a token owed to recipients (persistent).
    TotalLocked(Address),
    /// Emergency pause flag: when true all claims are rejected (instance).
    Paused,
    /// Time-decay parameters for individual claim windows (instance).
    DecayConfig,
    /// Address receiving the unclaimed decay remainder (instance).
    Treasury,
    /// Governance DAO allowed to advance the campaign to a new Merkle epoch.
    Governance,
    /// Current Merkle epoch (instance). Claimed bitmaps are shared across epochs.
    Epoch,
    /// Rate-limit configuration applied to `claim` and `claim_batch` (instance).
    RateLimitConfig,
    /// Whether KYC attestation is required to claim (instance, bool).
    KycRequired,
    /// KYC attestation record for an address: set to `true` by the admin (persistent).
    KycAttestation(Address),
    /// Token amount at or above which claims are routed into an internal vesting
    /// schedule rather than paid out immediately. When absent, all claims are
    /// liquid regardless of amount (instance, `i128`).
    ClaimThreshold,
}

/// Time-decaying claim schedule.
///
/// A claim made in `full_window_ledgers` after `start_ledger` receives 100% of
/// its leaf amount. Every `decay_step_ledgers` thereafter the claimable share
/// drops by `decay_bps_per_step` basis points, never below `floor_bps`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DecayConfig {
    /// First ledger of the campaign.
    pub start_ledger: u32,
    /// Number of full-reward ledgers after the start.
    pub full_window_ledgers: u32,
    /// Length of one decay step in ledgers.
    pub decay_step_ledgers: u32,
    /// Basis points removed per elapsed step.
    pub decay_bps_per_step: u32,
    /// Minimum claimable share in basis points.
    pub floor_bps: u32,
}

/// Vesting parameters applied to every claim.
///
/// On claim, `amount * initial_unlock_bps / 10_000` is transferred
/// immediately; the remainder vests linearly over `vesting_duration_ledgers`
/// ledgers starting at the claim ledger.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VestingConfig {
    /// Portion unlocked at claim time, in basis points (0..=10_000).
    pub initial_unlock_bps: u32,
    /// Number of ledgers over which the locked portion vests linearly.
    pub vesting_duration_ledgers: u32,
}

/// Locked portion of a single claim.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VestingSchedule {
    /// Amount locked at claim time.
    pub total: i128,
    /// Amount already released to the recipient.
    pub released: i128,
    /// Ledger at which vesting started (the claim ledger).
    pub start_ledger: u32,
    /// Vesting duration in ledgers.
    pub duration_ledgers: u32,
}

/// Per-ledger rate-limit configuration for `claim` and `claim_batch`.
///
/// Each address is allowed at most `max_claims` claims within any sliding
/// window of `window_ledgers` ledgers. The namespace used for
/// `soroban_common::check_and_record` is `1u32` (airdrop-claim namespace).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitConfig {
    /// Size of the sliding window in ledgers (must be ≥ 1).
    pub window_ledgers: u32,
    /// Maximum number of claims allowed per address within one window.
    pub max_claims: u32,
}
