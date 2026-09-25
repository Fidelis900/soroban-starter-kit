// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Merkle root as a 32-byte value stored as Bytes (instance).
    MerkleRoot,
    /// Whether `(recipient, token)` has already been claimed (persistent).
    Claimed(Address, Address),
    /// The ledger sequence number after which claims are rejected (instance).
    ClaimDeadline,
    /// Optional [`VestingConfig`] applied to every claim (instance).
    VestingConfig,
    /// [`VestingSchedule`] for `(recipient, token)` (persistent).
    Vesting(Address, Address),
    /// Total still-locked vesting balance of a token owed to recipients (persistent).
    TotalLocked(Address),
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
