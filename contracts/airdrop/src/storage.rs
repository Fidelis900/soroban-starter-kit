// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address (instance).
    Admin,
    /// Payment token address (instance).
    Token,
    /// Merkle root as a 32-byte value stored as Bytes, keyed by round (instance).
    MerkleRoot(u32),
    /// Whether a given address has already claimed in a given round (persistent).
    Claimed(u32, Address),
    /// The ledger sequence number after which claims are rejected (instance).
    ClaimDeadline,
}
