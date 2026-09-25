//! Minimal cross-contract interface for custodial NFT escrow (issue #1069).
//!
//! Only `transfer(from, to, token_id)` is required, matching the NFT template in
//! `contracts/nft`. `from` must authorize the transfer: the seller does so as a
//! sub-invocation of `start`/`start_dutch`, and the auction contract authorizes
//! its own releases implicitly as the direct invoker.
// `#[contractclient]` generates undocumented public items.
#![allow(missing_docs, dead_code)]

use soroban_sdk::{Address, Env, contractclient};

#[contractclient(name = "NftClient")]
pub trait NftInterface {
    fn transfer(env: Env, from: Address, to: Address, token_id: u32);
}
