#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, Symbol,
    testutils::{Address as _, Events as _, Ledger as _},
    token::StellarAssetClient,
};

use crate::{
    SubscriptionContract, SubscriptionContractClient, SubscriptionError, SubscriptionStatus,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_token(env: &Env, mint_to: &Address, amount: i128) -> Address {
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let addr = sac.address();
    StellarAssetClient::new(env, &addr).mint(mint_to, &amount);
    addr
}

/// Returns (client, contract_addr, provider, token).
fn setup(env: &Env) -> (SubscriptionContractClient, Address, Address, Address) {
    let provider = Address::generate(env);
    let token = make_token(env, &Address::generate(env), 0);
    let addr = env.register_contract(None, SubscriptionContract);
    let client = SubscriptionContractClient::new(env, &addr);
    client.initialize(&provider, &token);
    (client, addr, provider, token)
}

fn approve_and_subscribe(
    env: &Env,
    client: &SubscriptionContractClient,
    contract_addr: &Address,
    token_addr: &Address,
    subscriber: &Address,
    plan_id: &Symbol,
    allowance: i128,
    trial_ledgers: Option<u32>,
) {
    StellarAssetClient::new(env, token_addr).mint(subscriber, &allowance);
    let token_client = soroban_sdk::token::Client::new(env, token_addr);
    token_client.approve(subscriber, contract_addr, &allowance, &1_000_000);
    client.subscribe(subscriber, plan_id, &trial_ledgers, &None);
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_sets_provider_and_token() {
    let env = setup_env();
    let (client, _addr, provider, token) = setup(&env);
    assert_eq!(client.get_provider(), Some(provider));
    assert_eq!(client.get_token(), Some(token));
}

#[test]
fn test_initialize_twice_fails() {
    let env = setup_env();
    let (client, _addr, provider, token) = setup(&env);
    let result = client.try_initialize(&provider, &token);
    assert_eq!(result, Err(Ok(SubscriptionError::AlreadyInitialized)));
}

#[test]
fn test_register_plan_stores_plan() {
    let env = setup_env();
    let (client, _addr, provider, _token) = setup(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    
    let plan = client.get_plan(&basic_plan).unwrap();
    assert_eq!(plan.plan_id, basic_plan);
    assert_eq!(plan.amount, 100);
    assert_eq!(plan.interval_ledgers, 50);
    assert!(plan.active);
    let _ = provider;
}

#[test]
fn test_register_duplicate_plan_fails() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    let result = client.try_register_plan(&basic_plan, &200, &100);
    assert_eq!(result, Err(Ok(SubscriptionError::PlanAlreadyExists)));
}

#[test]
fn test_non_admin_cannot_register_plan() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let stranger = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    // Remove mock auths to test authorization
    env.set_auths(&[]);
    let result = client.try_register_plan(&basic_plan, &100, &50);
    assert!(result.is_err());
}

#[test]
fn test_subscribe_stores_subscription() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    // Register the plan first
    client.register_plan(&basic_plan, &100, &50);

    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);

    let info = client.get_subscription(&subscriber).unwrap();
    assert_eq!(info.plan_id, basic_plan);
    assert_eq!(info.amount, 100);
    assert_eq!(info.interval_ledgers, 50);
    assert!(info.active);
}

#[test]
fn test_subscribe_to_nonexistent_plan_fails() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let subscriber = Address::generate(&env);
    let nonexistent_plan = Symbol::new(&env, "noexist");
    
    let result = client.try_subscribe(&subscriber, &nonexistent_plan, &None, &None);
    assert_eq!(result, Err(Ok(SubscriptionError::PlanNotFound)));
}

#[test]
fn test_subscribe_to_inactive_plan_fails() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    client.set_plan_active(&basic_plan, &false);
    
    let result = client.try_subscribe(&subscriber, &basic_plan, &None, &None);
    assert_eq!(result, Err(Ok(SubscriptionError::PlanInactive)));
}

#[test]
fn test_subscribe_twice_fails() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);

    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 1_000, None);
    let result = client.try_subscribe(&subscriber, &basic_plan, &None, &None);
    assert_eq!(result, Err(Ok(SubscriptionError::AlreadySubscribed)));
}

#[test]
fn test_charge_before_interval_fails() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);

    // Advance only 10 ledgers (interval is 50)
    env.ledger().with_mut(|l| l.sequence_number += 10);

    let result = client.try_charge(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::IntervalNotElapsed)));
    let _ = provider;
}

#[test]
fn test_charge_after_interval_transfers_tokens() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 100);
    assert_eq!(token_client.balance(&subscriber), 400);
}

#[test]
fn test_charge_updates_last_charged_ledger() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);
    let start = env.ledger().sequence();

    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber);

    let info = client.get_subscription(&subscriber).unwrap();
    assert_eq!(info.last_charged_ledger, start + 50);
}

#[test]
fn test_charge_multiple_times() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 200);
}

#[test]
fn test_multiple_concurrent_plans_work() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber1 = Address::generate(&env);
    let subscriber2 = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic"); // 100 tokens every 50 ledgers
    let premium_plan = Symbol::new(&env, "premium"); // 200 tokens every 30 ledgers
    
    // Register both plans
    client.register_plan(&basic_plan, &100, &50);
    client.register_plan(&premium_plan, &200, &30);
    
    // Subscribe different subscribers to different plans
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber1, &basic_plan, 1000, None);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber2, &premium_plan, 1000, None);
    
    // Advance 30 ledgers - only premium plan interval has elapsed
    env.ledger().with_mut(|l| l.sequence_number += 30);
    
    // Charge both subscribers - only premium should succeed
    client.charge(&subscriber2);
    let premium_result = client.try_charge(&subscriber1);
    assert_eq!(premium_result, Err(Ok(SubscriptionError::IntervalNotElapsed)));
    
    // Check balances - only subscriber2 (premium) was charged
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 200);
    assert_eq!(token_client.balance(&subscriber1), 1000);
    assert_eq!(token_client.balance(&subscriber2), 800);
    
    // Advance another 20 ledgers (total 50) - basic plan interval now elapsed
    env.ledger().with_mut(|l| l.sequence_number += 20);
    client.charge(&subscriber1);
    
    // Check final balances
    assert_eq!(token_client.balance(&provider), 300);
    assert_eq!(token_client.balance(&subscriber1), 900);
    assert_eq!(token_client.balance(&subscriber2), 800);
    let _ = provider;
}

#[test]
fn test_charge_insufficient_allowance_fails() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    
    // Mint tokens but approve less than the charge amount
    StellarAssetClient::new(&env, &token).mint(&subscriber, &500);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    token_client.approve(&subscriber, &addr, &50, &1_000_000); // only 50 approved, need 100
    client.subscribe(&subscriber, &basic_plan, &None, &None);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    let result = client.try_charge(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::InsufficientAllowance)));
}

#[test]
fn test_cancel_deactivates_subscription() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);
    client.cancel(&subscriber);

    let info = client.get_subscription(&subscriber).unwrap();
    assert!(!info.active);
}

#[test]
fn test_cancel_prevents_further_charges() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);
    client.cancel(&subscriber);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    let result = client.try_charge(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::SubscriptionInactive)));
}

#[test]
fn test_cancel_twice_fails() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, None);
    client.cancel(&subscriber);

    let result = client.try_cancel(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::SubscriptionInactive)));
}

#[test]
fn test_cancel_nonexistent_fails() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let subscriber = Address::generate(&env);
    let result = client.try_cancel(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::NotSubscribed)));
}

#[test]
fn test_resubscribe_after_cancel() {
    let env = setup_env();
    let (client, addr, _provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    let premium_plan = Symbol::new(&env, "premium");
    
    client.register_plan(&basic_plan, &100, &50);
    client.register_plan(&premium_plan, &200, &30);
    
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 1_000, None);
    client.cancel(&subscriber);

    // Re-subscribing after cancel should succeed with a different plan.
    let result = client.try_subscribe(&subscriber, &premium_plan, &None, &None);
    assert!(result.is_ok());

    let info = client.get_subscription(&subscriber).unwrap();
    assert_eq!(info.plan_id, premium_plan);
    assert_eq!(info.amount, 200);
    assert_eq!(info.interval_ledgers, 30);
    assert!(info.active);
}

#[test]
fn test_charge_nonexistent_subscriber_fails() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let stranger = Address::generate(&env);
    let result = client.try_charge(&stranger);
    assert_eq!(result, Err(Ok(SubscriptionError::NotSubscribed)));
}

#[test]
fn test_get_subscription_returns_none_for_unknown() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(client.get_subscription(&stranger), None);
}

#[test]
fn test_get_plan_returns_none_for_unknown() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let nonexistent_plan = Symbol::new(&env, "noexist");
    assert_eq!(client.get_plan(&nonexistent_plan), None);
}

#[test]
fn test_set_plan_active_updates_status() {
    let env = setup_env();
    let (client, _addr, _provider, _token) = setup(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    let plan_before = client.get_plan(&basic_plan).unwrap();
    assert!(plan_before.active);
    
    client.set_plan_active(&basic_plan, &false);
    let plan_after = client.get_plan(&basic_plan).unwrap();
    assert!(!plan_after.active);
}

#[test]
fn test_charge_during_trial_fails() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    // Subscribe with a 100 ledger trial period, 50 ledger billing interval
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, Some(100));

    // Advance only 50 ledgers - still within trial period (need 100 to complete trial)
    env.ledger().with_mut(|l| l.sequence_number += 50);

    let result = client.try_charge(&subscriber);
    assert_eq!(result, Err(Ok(SubscriptionError::IntervalNotElapsed)));
    let _ = provider;
}

#[test]
fn test_charge_after_trial_marks_trial_completed_no_charge() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    // Subscribe with 100 ledger trial period
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, Some(100));

    // Advance 100 ledgers to complete the trial
    env.ledger().with_mut(|l| l.sequence_number += 100);

    // First charge after trial completes - should succeed but not charge anything yet
    client.charge(&subscriber);

    // Verify trial is marked as completed
    let info = client.get_subscription(&subscriber).unwrap();
    assert!(info.trial_completed);
    // Verify no tokens were transferred yet
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&subscriber), 500);
}

#[test]
fn test_charge_after_trial_and_first_interval_charges() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    // Subscribe with 100 ledger trial period, 50 ledger billing interval
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, Some(100));

    // Complete the trial period
    env.ledger().with_mut(|l| l.sequence_number += 100);
    client.charge(&subscriber); // Trial completed, no charge

    // Now advance another 50 ledgers to complete the first billing interval after trial
    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber); // First actual charge

    // Verify tokens were transferred
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 100);
    assert_eq!(token_client.balance(&subscriber), 400);
}

#[test]
fn test_subscribe_with_zero_trial_ledgers_skips_trial() {
    let env = setup_env();
    let (client, addr, provider, token) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic_plan = Symbol::new(&env, "basic");
    
    client.register_plan(&basic_plan, &100, &50);
    // Subscribe with explicit 0 trial ledgers (should behave like None)
    approve_and_subscribe(&env, &client, &addr, &token, &subscriber, &basic_plan, 500, Some(0));

    // Verify trial is already marked as completed
    let info = client.get_subscription(&subscriber).unwrap();
    assert!(info.trial_completed);

    // Advance 50 ledgers and charge normally
    env.ledger().with_mut(|l| l.sequence_number += 50);
    client.charge(&subscriber);

    // Verify tokens were transferred
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 100);
    assert_eq!(token_client.balance(&subscriber), 400);
}
#[test]
fn revenue_accumulates_in_escrow_and_is_withdrawn_at_threshold() {
    let env = setup_env();
    let (client, contract_addr, provider, token) = setup(&env);
    let plan = Symbol::new(&env, "plan");
    client.register_plan(&plan, &100, &10, &0, &0);
    let subscriber = Address::generate(&env);
    approve_and_subscribe(&env, &client, &contract_addr, &token, &subscriber, &plan, 1_000, None);
    client.set_min_payout_threshold(&provider, &150);

    client.charge(&subscriber, &plan);
    assert_eq!(client.get_provider_revenue(&provider), 100);
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&contract_addr), 100);
    assert_eq!(
        client.try_withdraw_provider_revenue(&provider),
        Err(Ok(SubscriptionError::BelowPayoutThreshold))
    );

    env.ledger().with_mut(|l| l.sequence_number = 10);
    client.charge(&subscriber, &plan);
    assert_eq!(client.get_provider_revenue(&provider), 200);
    assert_eq!(client.withdraw_provider_revenue(&provider), 200);
    assert_eq!(client.get_provider_revenue(&provider), 0);
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&provider), 200);
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&contract_addr), 0);
}

#[test]
fn due_subscriptions_are_queryable_with_advance_notice() {
    let env = setup_env();
    let (client, contract_addr, provider, token) = setup(&env);
    let plan = Symbol::new(&env, "plan");
    client.register_plan(&plan, &100, &100, &0, &0);
    client.set_notification_window(&10);
    let subscriber = Address::generate(&env);
    approve_and_subscribe(&env, &client, &contract_addr, &token, &subscriber, &plan, 1_000, None);

    env.ledger().with_mut(|l| l.sequence_number = 89);
    assert_eq!(client.get_due_subscriptions(&10).len(), 0);

    env.ledger().with_mut(|l| l.sequence_number = 95);
    let events_before = env.events().all().events().len();
    let due = client.get_due_subscriptions(&10);
    assert_eq!(due.len(), 1);
    assert_eq!(due.first().unwrap().0, subscriber);
    assert_eq!(due.first().unwrap().2, 100);
    assert!(env.events().all().events().len() > events_before);
    assert_eq!(client.get_due_subscriptions(&0).len(), 0);
}

#[test]
fn fixed_term_subscription_expires_after_max_billing_cycles() {
    let env = setup_env();
    let (client, contract_addr, provider, token) = setup(&env);
    let plan = Symbol::new(&env, "plan");
    client.register_plan(&plan, &100, &10, &0, &0);
    let subscriber = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&subscriber, &1_000);
    soroban_sdk::token::Client::new(&env, &token).approve(&subscriber, &contract_addr, &1_000, &1_000_000);
    client.subscribe(&subscriber, &plan, &None, &None, &Some(2));

    client.charge(&subscriber, &plan);
    env.ledger().with_mut(|l| l.sequence_number = 10);
    client.charge(&subscriber, &plan);
    let info = client.get_subscription(&subscriber, &plan).unwrap();
    assert_eq!(info.billing_cycles, 2);
    assert_eq!(info.status, SubscriptionStatus::Active);
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&contract_addr), 200);

    env.ledger().with_mut(|l| l.sequence_number = 20);
    assert_eq!(client.charge(&subscriber, &plan), crate::ChargeOutcome::Skipped);
    let info = client.get_subscription(&subscriber, &plan).unwrap();
    assert_eq!(info.status, SubscriptionStatus::Expired);
    assert_eq!(info.billing_cycles, 2);
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&contract_addr), 200);
    assert_eq!(client.get_due_subscriptions(&10).len(), 0);
    assert_eq!(
        client.try_charge(&subscriber, &plan),
        Err(Ok(SubscriptionError::SubscriptionInactive))
    );
}
