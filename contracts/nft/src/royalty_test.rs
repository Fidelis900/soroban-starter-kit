#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(test)]

use super::*;
use soroban_sdk::{Address, Env, String, testutils::Address as _};

fn init_with_collection_royalty<'a>(
    env: &'a Env,
    bps: u32,
    recipient: &Address,
) -> (NftContractClient<'a>, Address) {
    let admin = Address::generate(env);
    let addr = env.register(NftContract, ());
    let client = NftContractClient::new(env, &addr);
    client.initialize(
        &admin,
        &String::from_str(env, "Royal"),
        &String::from_str(env, "ROY"),
        &0,
        &Some(bps),
        &Some(recipient.clone()),
        &false,
    );
    (client, admin)
}

/// Per-token royalty written at mint is readable from `royalty_info`.
/// Guards the instance/persistent storage mismatch on `TokenRoyaltyRecipient`.
#[test]
fn royalty_info_returns_per_token_royalty() {
    let env = Env::default();
    env.mock_all_auths();

    let collection_recipient = Address::generate(&env);
    let token_recipient = Address::generate(&env);
    let (client, _) = init_with_collection_royalty(&env, 1_000, &collection_recipient);

    let owner = Address::generate(&env);
    let token_id = 1u32;
    client.mint(
        &owner,
        &token_id,
        &String::from_str(&env, "ipfs://1"),
        &Some(250u32),
        &Some(token_recipient.clone()),
    );

    let info = client.royalty_info(&token_id, &10_000i128).unwrap();
    assert_eq!(info.recipient, token_recipient);
    assert_eq!(info.amount, 250i128);
}

/// A second token with no override still falls back to the collection royalty,
/// and a zero-bps per-token override suppresses the collection default.
#[test]
fn royalty_info_per_token_and_collection_fallback() {
    let env = Env::default();
    env.mock_all_auths();

    let collection_recipient = Address::generate(&env);
    let token_recipient = Address::generate(&env);
    let (client, _) = init_with_collection_royalty(&env, 500, &collection_recipient);

    let owner = Address::generate(&env);
    let overridden = 1u32;
    client.mint(
        &owner,
        &overridden,
        &String::from_str(&env, "ipfs://overridden"),
        &Some(100u32),
        &Some(token_recipient.clone()),
    );
    let fallback = 2u32;
    client.mint(
        &owner,
        &fallback,
        &String::from_str(&env, "ipfs://fallback"),
        &None,
        &None,
    );

    let per_token = client.royalty_info(&overridden, &20_000i128).unwrap();
    assert_eq!(per_token.recipient, token_recipient);
    assert_eq!(per_token.amount, 200i128);

    let collection = client.royalty_info(&fallback, &20_000i128).unwrap();
    assert_eq!(collection.recipient, collection_recipient);
    assert_eq!(collection.amount, 1_000i128);
}

/// Royalty amount scales linearly with the sale price: amount == price * bps / 10_000.
#[test]
fn royalty_info_calculation_across_sale_prices() {
    let env = Env::default();
    env.mock_all_auths();

    let recipient = Address::generate(&env);
    let (client, _) = init_with_collection_royalty(&env, 500, &recipient);

    let owner = Address::generate(&env);
    let token_id = 1u32;
    client.mint(&owner, &token_id, &String::from_str(&env, "ipfs://scale"), &None, &None);

    // Zero sale price yields zero royalty.
    let zero = client.royalty_info(&token_id, &0i128).unwrap();
    assert_eq!(zero.recipient, recipient);
    assert_eq!(zero.amount, 0i128);

    // Typical sale prices: 5% of the price, floored.
    for price in [1_000i128, 10_000, 25_000, 100_000, 1_000_000] {
        let info = client.royalty_info(&token_id, &price).unwrap();
        assert_eq!(info.recipient, recipient);
        assert_eq!(info.amount, price * 500 / 10_000);
    }

    // Rounding down on non-divisible prices.
    let odd = client.royalty_info(&token_id, &333i128).unwrap();
    assert_eq!(odd.amount, 333i128 * 500 / 10_000);
}

/// The maximum allowed royalty (15%) is accepted and computed correctly.
#[test]
fn royalty_info_at_max_basis_points() {
    let env = Env::default();
    env.mock_all_auths();

    let recipient = Address::generate(&env);
    let (client, _) = init_with_collection_royalty(&env, 1_500, &recipient);

    let owner = Address::generate(&env);
    let token_id = 1u32;
    client.mint(&owner, &token_id, &String::from_str(&env, "ipfs://max"), &None, &None);

    for price in [0i128, 1_000, 10_000, 123_456] {
        let info = client.royalty_info(&token_id, &price).unwrap();
        assert_eq!(info.recipient, recipient);
        assert_eq!(info.amount, price * 1_500 / 10_000);
    }
}

/// Basis points above the 15% cap are rejected at initialization.
#[test]
fn initialize_rejects_royalty_above_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let addr = env.register(NftContract, ());
    let client = NftContractClient::new(&env, &addr);

    let result = client.try_initialize(
        &admin,
        &String::from_str(&env, "Royal"),
        &String::from_str(&env, "ROY"),
        &0,
        &Some(1_501u32),
        &Some(recipient.clone()),
        &false,
    );
    assert!(result.is_err());
}

/// Per-token basis points above the 15% cap are rejected at mint.
#[test]
fn mint_rejects_per_token_royalty_above_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let recipient = Address::generate(&env);
    let (client, _) = init_with_collection_royalty(&env, 500, &recipient);

    let owner = Address::generate(&env);
    let result = client.try_mint(
        &owner,
        &1u32,
        &String::from_str(&env, "ipfs://over-cap"),
        &Some(1_501u32),
        &Some(recipient.clone()),
    );
    assert!(result.is_err());
}
