// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, String, contracttype};

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
    /// Next token id for sequential batch minting.
    NextTokenId,
    /// Collection-level soulbound (non-transferable) flag.
    Soulbound,
    /// Number of tokens owned by an address (enumerable).
    Balance(Address),
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
    Approval(u32),
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
