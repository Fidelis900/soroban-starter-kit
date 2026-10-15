#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::{Address, Env, String, testutils::Address as _};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (NftContractClient, Address) {
    let admin = Address::generate(env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &String::from_str(env, "My Collection"),
        &String::from_str(env, "MYC"),
        &0,
        &None,
        &None,
    );
    (client, admin)
}

fn setup_with_cap(env: &Env, max_supply: u32) -> (NftContractClient, Address) {
    let admin = Address::generate(env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &String::from_str(env, "Capped"),
        &String::from_str(env, "CAP"),
        &max_supply,
        &None,
        &None,
    );
    (client, admin)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    assert_eq!(client.name(), String::from_str(&env, "My Collection"));
    assert_eq!(client.symbol(), String::from_str(&env, "MYC"));
    assert_eq!(client.total_supply(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);
    client.initialize(
        &admin,
        &String::from_str(&env, "A"),
        &String::from_str(&env, "A"),
        &0,
        &None,
        &None,
    );
    client.initialize(
        &admin,
        &String::from_str(&env, "B"),
        &String::from_str(&env, "B"),
        &0,
        &None,
        &None,
    );
}

#[test]
fn test_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://token/1"),
        &None,
        &None,
    );

    assert_eq!(client.owner_of(&1), owner);
    assert_eq!(client.total_supply(), 1);
    assert_eq!(
        client.token_uri(&1),
        String::from_str(&env, "ipfs://token/1")
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_mint_duplicate_token_id_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1b"),
        &None,
        &None,
    );
}

#[test]
fn test_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.transfer(&alice, &bob, &1);

    assert_eq!(client.owner_of(&1), bob);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_not_owner_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    client.mint(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    // bob tries to transfer alice's token
    client.transfer(&bob, &carol, &1);
}

#[test]
fn test_burn() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    assert_eq!(client.total_supply(), 1);

    client.burn(&owner, &1);
    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_approve_and_transfer_from() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.mint(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.approve(&1, &bob);
    assert_eq!(client.get_approved(&1), Some(bob.clone()));

    client.transfer_from(&bob, &alice, &carol, &1);
    assert_eq!(client.owner_of(&1), carol);
    // Approval should be cleared after transfer.
    assert_eq!(client.get_approved(&1), None);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_transfer_from_without_approval_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    client.mint(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    // Bob was never approved.
    client.transfer_from(&bob, &alice, &carol, &1);
}

#[test]
fn test_transfer_clears_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.mint(
        &alice,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.approve(&1, &bob);

    // Direct transfer should clear the approval.
    client.transfer(&alice, &carol, &1);
    assert_eq!(client.get_approved(&1), None);
}

#[test]
fn test_supply_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_with_cap(&env, 2);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.mint(
        &owner,
        &2,
        &String::from_str(&env, "ipfs://2"),
        &None,
        &None,
    );
    assert_eq!(client.total_supply(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_supply_cap_exceeded_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_with_cap(&env, 1);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );
    client.mint(
        &owner,
        &2,
        &String::from_str(&env, "ipfs://2"),
        &None,
        &None,
    );
}

#[test]
fn test_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let meta = client.metadata();
    assert_eq!(meta.name, String::from_str(&env, "My Collection"));
    assert_eq!(meta.symbol, String::from_str(&env, "MYC"));
}

// ---------------------------------------------------------------------------
// Royalty tests (#831)
// ---------------------------------------------------------------------------

/// No royalty configured → royalty_info returns None.
#[test]
fn test_royalty_info_none_when_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );

    let info = client.royalty_info(&1, &10_000i128);
    assert!(info.is_none());
}

/// Collection-level royalty is returned when no per-token override exists.
#[test]
fn test_royalty_info_collection_level() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let royalty_recipient = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);

    // 5 % collection royalty
    client.initialize(
        &admin,
        &String::from_str(&env, "Royal"),
        &String::from_str(&env, "ROY"),
        &0,
        &Some(500u32),
        &Some(royalty_recipient.clone()),
    );

    let owner = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &None,
        &None,
    );

    let info = client.royalty_info(&1, &10_000i128).unwrap();
    assert_eq!(info.recipient, royalty_recipient);
    assert_eq!(info.amount, 500i128); // 5 % of 10 000
}

/// Per-token royalty overrides the collection-level default.
#[test]
fn test_royalty_info_per_token_override() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let collection_recipient = Address::generate(&env);
    let token_recipient = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);

    // 10 % collection royalty
    client.initialize(
        &admin,
        &String::from_str(&env, "Override"),
        &String::from_str(&env, "OVR"),
        &0,
        &Some(1_000u32),
        &Some(collection_recipient.clone()),
    );

    let owner = Address::generate(&env);
    // Mint token 1 with a 2.5 % per-token override
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &Some(250u32),
        &Some(token_recipient.clone()),
    );
    // Mint token 2 with no per-token override → falls back to collection
    client.mint(
        &owner,
        &2,
        &String::from_str(&env, "ipfs://2"),
        &None,
        &None,
    );

    // Token 1: per-token recipient at 2.5 %
    let info1 = client.royalty_info(&1, &10_000i128).unwrap();
    assert_eq!(info1.recipient, token_recipient);
    assert_eq!(info1.amount, 250i128);

    // Token 2: collection recipient at 10 %
    let info2 = client.royalty_info(&2, &10_000i128).unwrap();
    assert_eq!(info2.recipient, collection_recipient);
    assert_eq!(info2.amount, 1_000i128);
}

/// royalty_info returns None for a token with an explicit 0 bps per-token royalty
/// even when collection royalty is set.
#[test]
fn test_royalty_info_per_token_zero_bps_returns_none() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let collection_recipient = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &String::from_str(&env, "Z"),
        &String::from_str(&env, "Z"),
        &0,
        &Some(500u32),
        &Some(collection_recipient.clone()),
    );

    let owner = Address::generate(&env);
    // Mint with an explicit 0-bps per-token override (royalty-free token)
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &Some(0u32),
        &None,
    );

    let info = client.royalty_info(&1, &10_000i128);
    assert!(info.is_none());
}

/// royalty_info on a non-existent token returns TokenNotFound.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_royalty_info_token_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    // Token 99 was never minted.
    client.royalty_info(&99, &1_000i128);
}

/// initialize rejects royalty_bps > 10 000.
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_initialize_invalid_royalty_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "BAD"),
        &0,
        &Some(10_001u32),
        &Some(recipient),
    );
}

/// initialize rejects royalty_bps > 0 without a recipient.
#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_initialize_royalty_missing_recipient_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let addr = env.register_contract(None, NftContract);
    let client = NftContractClient::new(&env, &addr);

    client.initialize(
        &admin,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "BAD"),
        &0,
        &Some(500u32),
        &None, // missing recipient
    );
}

/// mint rejects token_royalty_bps > 10 000.
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_mint_invalid_token_royalty_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.mint(
        &owner,
        &1,
        &String::from_str(&env, "ipfs://1"),
        &Some(20_000u32),
        &Some(recipient),
    );
}


// ---------------------------------------------------------------------------
// Persistent storage (#1185)
// ---------------------------------------------------------------------------

#[test]
fn test_per_token_data_uses_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let owner = Address::generate(&env);
    let contract_id = client.address.clone();

    for id in 1u32..=20 {
        client.mint(
            &owner,
            &id,
            &String::from_str(&env, "ipfs://QmVeryLongHashThatWouldBloatInstanceStorageIfStoredThere"),
            &None,
            &None,
        );
    }

    env.as_contract(&contract_id, || {
        for id in 1u32..=20 {
            assert!(env.storage().persistent().has(&TokenKey::Owner(id)));
            assert!(env.storage().persistent().has(&TokenKey::Uri(id)));
            assert!(!env.storage().instance().has(&TokenKey::Owner(id)));
            assert!(!env.storage().instance().has(&TokenKey::Uri(id)));
        }
        assert!(env.storage().instance().has(&DataKey::TotalSupply));
        assert!(env.storage().instance().has(&DataKey::Name));
    });
    assert_eq!(client.total_supply(), 20);
}


// ---------------------------------------------------------------------------
// Batch mint / transfer (#1186)
// ---------------------------------------------------------------------------

#[test]
fn test_mint_batch_sequential_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let owner = Address::generate(&env);

    let mut uris = soroban_sdk::Vec::new(&env);
    uris.push_back(String::from_str(&env, "ipfs://a"));
    uris.push_back(String::from_str(&env, "ipfs://b"));
    uris.push_back(String::from_str(&env, "ipfs://c"));

    let ids = client.mint_batch(&owner, &uris);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 2);
    assert_eq!(ids.get(2).unwrap(), 3);
    assert_eq!(client.total_supply(), 3);
    assert_eq!(client.owner_of(&2), owner);
    assert_eq!(client.token_uri(&3), String::from_str(&env, "ipfs://c"));
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_mint_batch_respects_supply_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_with_cap(&env, 2);
    let owner = Address::generate(&env);

    let mut uris = soroban_sdk::Vec::new(&env);
    uris.push_back(String::from_str(&env, "ipfs://a"));
    uris.push_back(String::from_str(&env, "ipfs://b"));
    uris.push_back(String::from_str(&env, "ipfs://c"));
    client.mint_batch(&owner, &uris);
}

#[test]
fn test_transfer_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let mut uris = soroban_sdk::Vec::new(&env);
    uris.push_back(String::from_str(&env, "ipfs://a"));
    uris.push_back(String::from_str(&env, "ipfs://b"));
    let ids = client.mint_batch(&alice, &uris);

    let mut token_ids = soroban_sdk::Vec::new(&env);
    token_ids.push_back(ids.get(0).unwrap());
    token_ids.push_back(ids.get(1).unwrap());
    client.transfer_batch(&alice, &bob, &token_ids);

    assert_eq!(client.owner_of(&1), bob);
    assert_eq!(client.owner_of(&2), bob);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_batch_not_owner_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    client.mint(&alice, &1, &String::from_str(&env, "ipfs://1"), &None, &None);
    client.mint(&bob, &2, &String::from_str(&env, "ipfs://2"), &None, &None);

    let mut token_ids = soroban_sdk::Vec::new(&env);
    token_ids.push_back(1u32);
    token_ids.push_back(2u32);
    // alice does not own token 2
    client.transfer_batch(&alice, &carol, &token_ids);
}
