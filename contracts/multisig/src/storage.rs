// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Map, Symbol, Val, Vec, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Signers,
    /// Per-signer vote weights. Key: signer address, value: weight (u32).
    /// When absent all signers have weight 1 (backward-compatible default).
    Weights,
    /// Accumulated weight threshold required to execute a proposal.
    /// For un-weighted wallets this equals the signer count threshold.
    Threshold,
    NextTransactionId,
    Transaction(u64),
    Paused,
    Version,
    /// Maximum number of ledgers a proposal remains valid after creation.
    MaxProposalValidityLedgers,
    /// Token spending limits map keyed by token address.
    SpendingLimits(Address),
    /// All configured token spending limits.
    SpendingLimitsList,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub id: u64,
    pub proposer: Address,
    pub target: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
    /// Addresses that have signed this proposal.
    pub signatures: Vec<Address>,
    /// Accumulated weight of all signatures collected so far.
    pub accumulated_weight: u32,
    pub executed: bool,
    /// Ledger sequence after which this proposal is considered expired.
    pub expiry_ledger: u32,
    /// Ledger sequence when the proposal was created.
    pub created_ledger: u32,
    /// Nonce for off-chain signature domain separation.
    pub nonce: u64,
}

/// Per-signer weight entry stored in the `Weights` map.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignerWeight {
    pub signer: Address,
    pub weight: u32,
}

/// Spending limit configuration for a specific token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenSpendingLimit {
    pub token: Address,
    pub daily_limit: i128,
    pub spent_amount: i128,
    pub last_refill_ledger: u32,
}

/// Spending limit configuration for a specific token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenSpendingLimit {
    pub token: Address,
    pub daily_limit: i128,
    pub spent_amount: i128,
    pub last_refill_ledger: u32,
}

/// Retrieve the weight assigned to `signer`.  Returns 1 if no weights map is
/// stored (i.e. the wallet was initialized without per-signer weights).
pub fn get_weight(weights: &Map<Address, u32>, signer: &Address) -> u32 {
    weights.get(signer.clone()).unwrap_or(1)
}
