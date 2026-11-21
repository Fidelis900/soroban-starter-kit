// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, String, contracttype};

/// Maximum royalty that may be configured, in basis points (15%).
///
/// Enforced for both collection-level and per-token royalty overrides so that
/// no creator can extract more than 15% of a secondary sale price.
pub const MAX_ROYALTY_BPS: u32 = 1_500;

/// Instance-storage keys (shared TTL for contract-level data).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Name,
    Symbol,
    TotalSupply,
    MaxSupply,
    Initialized,
    /// Collection-level default royalty in basis points (0–10 000).
    RoyaltyBps,
    /// Collection-level default royalty recipient.
    RoyaltyRecipient,
    /// Number of tokens burned so far. Active supply is `TotalSupply - Burned`.
    Burned,
    /// Persistent: collection-wide operator approval `(owner, operator) -> bool`.
    OperatorApproval(Address, Address),
    /// Persistent: number of tokens owned by an address.
    Balance(Address),
    /// Next token id for sequential batch minting.
    NextTokenId,
    /// Collection-level soulbound (non-transferable) flag.
    Soulbound,
    /// Token id at `index` in an owner's enumerable list.
    OwnerTokens(Address, u32),
    /// Index of a token id within its owner's enumerable list.
    OwnedIndex(u32),
}

/// Persistent-storage keys (per-key TTL for per-token data).
#[contracttype]
#[derive(Clone)]
pub enum TokenKey {
    Owner(u32),
    Approved(u32),
    Uri(u32),
    /// Optional per-token royalty override in basis points (0–10 000).
    TokenRoyaltyBps(u32),
    /// Optional per-token royalty recipient override.
    TokenRoyaltyRecipient(u32),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub token_uri: String,
}

/// Return value of [`NftContract::royalty_info`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoyaltyInfo {
    /// Recipient of the royalty payment.
    pub recipient: Address,
    /// Royalty amount for the given `sale_price` (already computed).
    pub amount: i128,
}

/// Payload emitted once per successful [`NftContract::mint_batch`] call.
///
/// A single event is emitted for the whole batch (rather than one per token)
/// to minimise the transaction overhead of large collection drops.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchMinted {
    /// Recipient of every token in the batch.
    pub to: Address,
    /// First token id assigned in the batch (inclusive).
    pub start_token_id: u32,
    /// Number of tokens minted in the batch.
    pub count: u32,
}
