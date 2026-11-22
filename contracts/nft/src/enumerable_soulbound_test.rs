//! Tests for the enumerable owner index (issue #1474) and per-token soulbound
//! flag (issue #1475).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

extern crate std;

use std::vec::Vec as StdVec;

use soroban_sdk::{Address, Env, String, Vec, testutils::Address as _};

use crate::{NftContract, NftContractClient, NftError};

fn setup(env: &Env) -> NftContractClient<'_> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let addr = env.register(NftContract, ());
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &String::from_str(env, "Indexed"),
        &String::from_str(env, "IDX"),
        &0,
        &None,
        &None,
        &false,
    );
    client
}

fn mint(env: &Env, client: &NftContractClient, to: &Address, token_id: u32) {
    client.mint(to, &token_id, &String::from_str(env, "ipfs://t"), &None, &None);
}

/// Collect `owner`'s tokens through the index, sorted, after checking that
/// every index slot points at a token the owner actually holds.
fn owned_tokens(client: &NftContractClient, owner: &Address) -> StdVec<u32> {
    let balance = client.balance_of(owner);
    let mut ids = StdVec::new();
    for index in 0..balance {
        let id = client.tokens_by_owner(owner, &index);
        assert_eq!(&client.owner_of(&id), owner, "index slot {index} is stale");
        assert_eq!(client.token_of_owner_by_index(owner, &index), id);
        ids.push(id);
    }
    // One past the end is always rejected.
    assert_eq!(
        client.try_tokens_by_owner(owner, &balance),
        Err(Ok(NftError::InvalidTokenId))
    );
    ids.sort_unstable();
    ids
}

// ---------------------------------------------------------------------------
// Issue #1474: enumerable owner index
// ---------------------------------------------------------------------------

#[test]
fn test_index_tracks_mints_in_order() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);

    assert_eq!(client.balance_of(&alice), 0);
    assert_eq!(
        client.try_tokens_by_owner(&alice, &0),
        Err(Ok(NftError::InvalidTokenId))
    );

    mint(&env, &client, &alice, 7);
    mint(&env, &client, &alice, 3);
    mint(&env, &client, &alice, 11);

    assert_eq!(client.balance_of(&alice), 3);
    assert_eq!(client.tokens_by_owner(&alice, &0), 7);
    assert_eq!(client.tokens_by_owner(&alice, &1), 3);
    assert_eq!(client.tokens_by_owner(&alice, &2), 11);
}

#[test]
fn test_index_integrity_across_transfers() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    for id in 1..=5u32 {
        mint(&env, &client, &alice, id);
    }

    // Move a middle token: swap-and-pop fills the hole with alice's last token.
    client.transfer(&alice, &bob, &2);
    assert_eq!(client.balance_of(&alice), 4);
    assert_eq!(client.tokens_by_owner(&alice, &1), 5);
    assert_eq!(owned_tokens(&client, &alice), [1, 3, 4, 5]);
    assert_eq!(owned_tokens(&client, &bob), [2]);

    // Move the last token (no swap needed).
    client.transfer(&alice, &bob, &4);
    assert_eq!(owned_tokens(&client, &alice), [1, 3, 5]);
    assert_eq!(owned_tokens(&client, &bob), [2, 4]);

    // transfer_from by an approved spender keeps the index in sync too.
    client.approve(&1, &carol);
    client.transfer_from(&carol, &alice, &carol, &1);
    assert_eq!(owned_tokens(&client, &alice), [3, 5]);
    assert_eq!(owned_tokens(&client, &carol), [1]);

    // Batch transfer.
    let mut batch = Vec::new(&env);
    batch.push_back(3u32);
    batch.push_back(5u32);
    client.transfer_batch(&alice, &bob, &batch);
    assert_eq!(client.balance_of(&alice), 0);
    assert_eq!(owned_tokens(&client, &alice), StdVec::<u32>::new());
    assert_eq!(owned_tokens(&client, &bob), [2, 3, 4, 5]);

    // Transferring to yourself never double-counts.
    client.transfer(&bob, &bob, &3);
    assert_eq!(client.balance_of(&bob), 4);
    assert_eq!(owned_tokens(&client, &bob), [2, 3, 4, 5]);

    // Balances always sum to the supply.
    let total = client.balance_of(&alice) + client.balance_of(&bob) + client.balance_of(&carol);
    assert_eq!(total, client.total_supply());
}

#[test]
fn test_index_integrity_across_burns() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);

    for id in 1..=4u32 {
        mint(&env, &client, &alice, id);
    }

    client.burn(&alice, &1); // first slot: last token (4) swaps in
    assert_eq!(client.tokens_by_owner(&alice, &0), 4);
    assert_eq!(owned_tokens(&client, &alice), [2, 3, 4]);

    client.burn(&alice, &3);
    assert_eq!(owned_tokens(&client, &alice), [2, 4]);
    assert_eq!(client.total_supply(), 2);

    // A burned id can be minted again and is indexed exactly once.
    mint(&env, &client, &alice, 1);
    assert_eq!(owned_tokens(&client, &alice), [1, 2, 4]);

    client.burn(&alice, &2);
    client.burn(&alice, &4);
    client.burn(&alice, &1);
    assert_eq!(client.balance_of(&alice), 0);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_mint_batch_is_indexed() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);

    let mut uris = Vec::new(&env);
    uris.push_back(String::from_str(&env, "ipfs://a"));
    uris.push_back(String::from_str(&env, "ipfs://b"));
    uris.push_back(String::from_str(&env, "ipfs://c"));
    let ids = client.mint_batch(&alice, &uris);

    assert_eq!(client.balance_of(&alice), 3);
    for index in 0..3u32 {
        assert_eq!(client.tokens_by_owner(&alice, &index), ids.get(index).unwrap());
    }
}

#[test]
fn test_batch_with_duplicate_id_reverts_without_drift() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    mint(&env, &client, &alice, 1);
    mint(&env, &client, &alice, 2);

    let mut batch = Vec::new(&env);
    batch.push_back(1u32);
    batch.push_back(1u32);
    assert_eq!(
        client.try_transfer_batch(&alice, &bob, &batch),
        Err(Ok(NftError::NotOwner))
    );

    // The failed call left the index untouched.
    assert_eq!(owned_tokens(&client, &alice), [1, 2]);
    assert_eq!(client.balance_of(&bob), 0);
}

// ---------------------------------------------------------------------------
// Issue #1475: per-token soulbound flag
// ---------------------------------------------------------------------------

#[test]
fn test_soulbound_token_blocks_transfer_and_transfer_from() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_with_soulbound(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://badge"),
        &None,
        &None,
        &true,
    );
    assert!(client.is_token_soulbound(&1));
    assert!(!client.is_soulbound(), "only the token is soulbound");

    assert_eq!(
        client.try_transfer(&alice, &bob, &1),
        Err(Ok(NftError::NonTransferable))
    );

    // Even an approved spender or operator cannot move it.
    client.approve(&1, &bob);
    assert_eq!(
        client.try_transfer_from(&bob, &alice, &bob, &1),
        Err(Ok(NftError::NonTransferable))
    );
    client.set_approval_for_all(&alice, &bob, &true);
    assert_eq!(
        client.try_transfer_from(&bob, &alice, &bob, &1),
        Err(Ok(NftError::NonTransferable))
    );

    let mut batch = Vec::new(&env);
    batch.push_back(1u32);
    assert_eq!(
        client.try_transfer_batch(&alice, &bob, &batch),
        Err(Ok(NftError::NonTransferable))
    );

    // Ownership and index are unchanged.
    assert_eq!(client.owner_of(&1), alice);
    assert_eq!(owned_tokens(&client, &alice), [1]);
    assert_eq!(client.balance_of(&bob), 0);
}

#[test]
fn test_non_soulbound_tokens_remain_transferable() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.mint_with_soulbound(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://badge"),
        &None,
        &None,
        &true,
    );
    client.mint_with_soulbound(
        &alice,
        &2,
        &String::from_str(&env, "ipfs://art"),
        &None,
        &None,
        &false,
    );
    mint(&env, &client, &alice, 3);
    assert!(!client.is_token_soulbound(&2));
    assert!(!client.is_token_soulbound(&3));

    client.transfer(&alice, &bob, &2);
    client.approve(&3, &carol);
    client.transfer_from(&carol, &alice, &carol, &3);

    assert_eq!(client.owner_of(&2), bob);
    assert_eq!(client.owner_of(&3), carol);
    assert_eq!(owned_tokens(&client, &alice), [1]);
}

#[test]
fn test_soulbound_token_can_be_burned_and_flag_cleared() {
    let env = Env::default();
    let client = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_with_soulbound(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://badge"),
        &None,
        &None,
        &true,
    );
    client.burn(&alice, &1);
    assert_eq!(client.balance_of(&alice), 0);
    assert!(!client.is_token_soulbound(&1));

    // Re-minting the id as a regular token makes it transferable again.
    mint(&env, &client, &alice, 1);
    client.transfer(&alice, &bob, &1);
    assert_eq!(client.owner_of(&1), bob);
}
