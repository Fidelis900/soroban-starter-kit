#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
};

fn setup(env: &Env) -> (OracleContractClient, Address) {
    let admin = Address::generate(env);
    let addr = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(env, &addr);
    client.initialize(&admin, &100);
    (client, admin)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    // Push a price so get_price_data works.
    client.update_price(&1_000_000);
    let data = client.get_price_data();
    assert_eq!(data.admin, admin);
    assert_eq!(data.staleness_threshold, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    client.initialize(&admin, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_zero_threshold_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let addr = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &addr);
    client.initialize(&admin, &0);
}

#[test]
fn test_update_and_get_price() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&5_000_000);
    assert_eq!(client.get_price(), 5_000_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_get_price_before_any_update_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.get_price();
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_stale_price_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&1_000_000);
    // Advance ledger past threshold (100).
    env.ledger().with_mut(|l| l.sequence_number += 101);
    client.get_price();
}

#[test]
fn test_price_at_threshold_boundary_is_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&2_000_000);
    // Advance exactly to threshold — still valid.
    env.ledger().with_mut(|l| l.sequence_number += 100);
    assert_eq!(client.get_price(), 2_000_000);
}

#[test]
fn test_price_update_overwrites_previous() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&1_000);
    client.update_price(&9_999);
    assert_eq!(client.get_price(), 9_999);
}

// ---------------------------------------------------------------------------
// #943 — get_median_price with staleness filter
// ---------------------------------------------------------------------------

/// get_median_price requires at least one fresh publisher submission.
#[test]
fn test_get_median_price_with_fresh_submissions() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    let pub1 = Address::generate(&env);
    let pub2 = Address::generate(&env);
    let pub3 = Address::generate(&env);
    client.set_publishers(&admin, &Vec::from_array(&env, [pub1.clone(), pub2.clone(), pub3.clone()]));

    // All publishers submit fresh prices at current time
    client.submit_price(&pub1, &100);
    client.submit_price(&pub2, &200);
    client.submit_price(&pub3, &300);

    // Median of [100, 200, 300] = 200
    assert_eq!(client.get_median_price(&3600), 200);
}

/// Stale submissions are excluded from the median calculation.
///
/// Setup:
/// - pub1 submits price 100 at timestamp 0
/// - pub2 submits price 200 at timestamp 3600 (fresh)
/// - pub3 submits price 300 at timestamp 0 (stale after 3600 seconds)
/// - max_staleness = 3600 (1 hour)
///
/// At current time 3600:
/// - pub1's submission age = 3600 (at boundary, included)
/// - pub2's submission age = 0 (fresh, included)
/// - pub3's submission age = 3600 (at boundary, included)
/// - Median should be calculated from all three: [100, 200, 300] → 200
#[test]
fn test_get_median_price_excludes_stale_submissions() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    let pub1 = Address::generate(&env);
    let pub2 = Address::generate(&env);
    let pub3 = Address::generate(&env);
    client.set_publishers(&admin, &Vec::from_array(&env, [pub1.clone(), pub2.clone(), pub3.clone()]));

    // pub1 and pub3 submit at time 0
    env.ledger().with_mut(|l| l.timestamp = 0);
    client.submit_price(&pub1, &100);
    client.submit_price(&pub3, &300);

    // Advance time to 3600 seconds, pub2 submits fresh price
    env.ledger().with_mut(|l| l.timestamp = 3600);
    client.submit_price(&pub2, &200);

    // At time 3600 with max_staleness_seconds = 1800 (30 minutes):
    // - pub1 age = 3600 > 1800 (stale, excluded)
    // - pub2 age = 0 ≤ 1800 (fresh, included)
    // - pub3 age = 3600 > 1800 (stale, excluded)
    // Only pub2's price [200] remains → median = 200
    assert_eq!(client.get_median_price(&1800), 200);
}

/// get_median_price fails when all submissions are stale.
#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_get_median_price_all_stale_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    let pub1 = Address::generate(&env);
    let pub2 = Address::generate(&env);
    client.set_publishers(&admin, &Vec::from_array(&env, [pub1.clone(), pub2.clone()]));

    // Publishers submit at time 0
    env.ledger().with_mut(|l| l.timestamp = 0);
    client.submit_price(&pub1, &100);
    client.submit_price(&pub2, &200);

    // Advance to time 5000, making all submissions older than 1000 seconds
    env.ledger().with_mut(|l| l.timestamp = 5000);

    // max_staleness = 1000 seconds, all submissions age > 1000 → NoPublisherData
    client.get_median_price(&1000);
}

// ---------------------------------------------------------------------------
// #1258 — update_price rejects zero and negative prices
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_update_price_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&0);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_update_price_negative_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    client.update_price(&-1);
}

// ---------------------------------------------------------------------------
// #1259 — Multi-asset price feed with decimals
// ---------------------------------------------------------------------------

#[test]
fn test_update_asset_price_and_get_price_for() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let asset = soroban_sdk::symbol_short!("XLM");
    client.update_asset_price(&asset, &7_500_000i128, &7u32);
    assert_eq!(client.get_price_for(&asset), 7_500_000i128);
}

#[test]
fn test_get_decimals_for_asset() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let asset = soroban_sdk::symbol_short!("BTC");
    client.update_asset_price(&asset, &30_000_000_000i128, &8u32);
    assert_eq!(client.get_decimals(&asset), 8u32);
}

#[test]
fn test_get_asset_feed_full_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let asset = soroban_sdk::symbol_short!("USDC");
    client.update_asset_price(&asset, &10_000_000i128, &7u32);
    let feed = client.get_asset_feed(&asset);
    assert_eq!(feed.price, 10_000_000i128);
    assert_eq!(feed.decimals, 7u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_get_price_for_unknown_asset_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let unknown = soroban_sdk::symbol_short!("UNKWN");
    client.get_price_for(&unknown);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_update_asset_price_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let asset = soroban_sdk::symbol_short!("XLM");
    client.update_asset_price(&asset, &0i128, &7u32);
}

#[test]
fn test_multiple_assets_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let xlm = soroban_sdk::symbol_short!("XLM");
    let btc = soroban_sdk::symbol_short!("BTC");

    client.update_asset_price(&xlm, &1_000_000i128, &7u32);
    client.update_asset_price(&btc, &50_000_000_000i128, &8u32);

    assert_eq!(client.get_price_for(&xlm), 1_000_000i128);
    assert_eq!(client.get_price_for(&btc), 50_000_000_000i128);
    assert_eq!(client.get_decimals(&xlm), 7u32);
    assert_eq!(client.get_decimals(&btc), 8u32);
}
