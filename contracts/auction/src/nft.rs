//! Minimal cross-contract interface for custodial NFT escrow (issue #1069).
//!
//! Only `transfer(from, to, token_id)` is required, matching the NFT template in
//! `contracts/nft`. `from` must authorize the transfer: the seller does so as a
//! sub-invocation of `start`/`start_dutch`, and the auction contract authorizes
//! its own releases implicitly as the direct invoker.
//!
//! `royalty_info` is optional for issue #1295: if the NFT contract supports it,
//! the auction will query and pay creator royalties on settlement.
// `#[contractclient]` generates undocumented public items.
#![allow(missing_docs, dead_code)]

use soroban_sdk::{Address, Env, contractclient};

/// Royalty info returned by NFT contracts that support creator royalties.
#[derive(Clone, Debug)]
pub struct RoyaltyInfo {
    pub recipient: Address,
    pub amount: i128,
}

#[contractclient(name = "NftClient")]
pub trait NftInterface {
    fn transfer(env: Env, from: Address, to: Address, token_id: u32);
    
    /// Optional: return royalty info for a given token and sale price.
    /// Returns (recipient_address, royalty_amount).
    fn royalty_info(env: Env, token_id: u32, sale_price: i128) -> Option<(Address, i128)>;
}
