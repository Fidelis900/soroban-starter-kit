#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![cfg(test)]

extern crate std;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

use crate::{NftContract, NftContractClient};

fn setup_nft<'a>(env: &'a Env) -> (NftContractClient<'a>, Address) {
    let admin = Address::generate(env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &String::from_str(env, "PropTest"),
        &String::from_str(env, "PT"),
        &0,
        &None,
        &None,
    &false,
    );
    (client, admin)
}

proptest! {
    /// Minting `n` distinct tokens always results in `total_supply == n`.
    #[test]
    fn prop_total_supply_equals_mint_count(n in 1u32..=20u32) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup_nft(&env);
        let owner = Address::generate(&env);

        for token_id in 0..n {
            client.mint(&owner, &token_id, &String::from_str(&env, "ipfs://x"), &None, &None);
        }

        prop_assert_eq!(client.total_supply(), n);
    }

    /// Mint then burn returns total_supply to 0 for a single token.
    #[test]
    fn prop_burn_decrements_supply(token_id in 0u32..=1000u32) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup_nft(&env);
        let owner = Address::generate(&env);

        client.mint(&owner, &token_id, &String::from_str(&env, "ipfs://x"), &None, &None);
        prop_assert_eq!(client.total_supply(), 1);

        client.burn(&owner, &token_id);
        prop_assert_eq!(client.total_supply(), 0);
    }

    /// Transfer does not change total_supply.
    #[test]
    fn prop_transfer_does_not_change_supply(token_id in 0u32..=1000u32) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup_nft(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &token_id, &String::from_str(&env, "ipfs://x"), &None, &None);
        let before = client.total_supply();
        client.transfer(&alice, &bob, &token_id);
        prop_assert_eq!(client.total_supply(), before);
    }

    /// owner_of returns the correct owner after mint.
    #[test]
    fn prop_owner_of_after_mint(token_id in 0u32..=1000u32) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup_nft(&env);
        let owner = Address::generate(&env);

        client.mint(&owner, &token_id, &String::from_str(&env, "ipfs://x"), &None, &None);
        prop_assert_eq!(client.owner_of(&token_id), owner);
    }
}
