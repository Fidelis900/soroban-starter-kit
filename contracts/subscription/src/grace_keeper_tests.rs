//! Tests for the failed-charge grace period (issue #1485) and keeper-bot
//! recurring billing (issue #1484).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, Symbol,
    testutils::{Address as _, Ledger as _},
    token,
};

use crate::{
    ChargeOutcome, DEFAULT_GRACE_PERIOD_LEDGERS, SubscriptionContract, SubscriptionContractClient,
    SubscriptionError,
};

/// Allowance expiry used by the tests; well beyond any ledger they reach.
const ALLOWANCE_EXPIRY: u32 = 200_000;

struct Setup<'a> {
    env: Env,
    client: SubscriptionContractClient<'a>,
    contract_id: Address,
    provider: Address,
    token_id: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(SubscriptionContract, ());
    let client = SubscriptionContractClient::new(&env, &contract_id);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.initialize(&provider, &token_id);
    Setup {
        env,
        client,
        contract_id,
        provider,
        token_id,
    }
}

fn mint(s: &Setup, to: &Address, amount: i128) {
    token::StellarAssetClient::new(&s.env, &s.token_id).mint(to, &amount);
}

fn approve(s: &Setup, subscriber: &Address, amount: i128) {
    token::Client::new(&s.env, &s.token_id).approve(
        subscriber,
        &s.contract_id,
        &amount,
        &ALLOWANCE_EXPIRY,
    );
}

fn balance(s: &Setup, who: &Address) -> i128 {
    token::Client::new(&s.env, &s.token_id).balance(who)
}

fn advance(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|l| l.sequence_number += ledgers);
}

// ---------------------------------------------------------------------------
// Issue #1485: grace period on failed charge
// ---------------------------------------------------------------------------

#[test]
fn test_default_grace_period_is_three_days() {
    let s = setup();
    // 3 days * 24h * 60m * 60s / ~5s per ledger.
    assert_eq!(DEFAULT_GRACE_PERIOD_LEDGERS, 51_840);
    assert_eq!(s.client.get_grace_period(), DEFAULT_GRACE_PERIOD_LEDGERS);
}

#[test]
fn test_access_preserved_during_grace_period_and_retry_succeeds() {
    let s = setup();
    let subscriber = Address::generate(&s.env);
    let plan = Symbol::new(&s.env, "basic");
    s.client.register_plan(&plan, &100, &10, &0, &0);

    // Funded, but no allowance yet: the first recurring charge will fail.
    mint(&s, &subscriber, 1_000);
    s.client.subscribe(&subscriber, &plan, &None, &None, &None);
    assert!(s.client.has_access(&subscriber, &plan));

    advance(&s.env, 10);
    let failed_at = s.env.ledger().sequence();
    assert_eq!(
        s.client.charge(&subscriber, &plan),
        ChargeOutcome::PaymentFailed
    );

    // The failure opened a 3-day grace period instead of cancelling the plan.
    let grace_end = failed_at + DEFAULT_GRACE_PERIOD_LEDGERS;
    assert_eq!(
        s.client.get_grace_period_end(&subscriber, &plan),
        Some(grace_end)
    );
    let info = s.client.get_subscription(&subscriber, &plan).unwrap();
    assert!(info.active);
    assert!(!info.suspended);
    assert_eq!(info.failed_charges_count, 1);
    assert!(s.client.has_access(&subscriber, &plan));

    // A second failed retry inside the window keeps the original deadline.
    advance(&s.env, 1_000);
    assert_eq!(
        s.client.charge(&subscriber, &plan),
        ChargeOutcome::PaymentFailed
    );
    assert_eq!(
        s.client.get_grace_period_end(&subscriber, &plan),
        Some(grace_end)
    );
    assert!(s.client.has_access(&subscriber, &plan));

    // Last ledger of the grace period: access is still active.
    s.env
        .ledger()
        .with_mut(|l| l.sequence_number = grace_end - 1);
    assert!(s.client.has_access(&subscriber, &plan));

    // The subscriber fixes their allowance and the retry succeeds.
    approve(&s, &subscriber, 1_000);
    assert_eq!(s.client.charge(&subscriber, &plan), ChargeOutcome::Charged);

    let info = s.client.get_subscription(&subscriber, &plan).unwrap();
    assert!(info.active);
    assert!(!info.suspended);
    assert_eq!(info.failed_charges_count, 0);
    assert_eq!(info.delinquent_since_ledger, None);
    assert_eq!(s.client.get_grace_period_end(&subscriber, &plan), None);
    assert!(s.client.has_access(&subscriber, &plan));
    assert_eq!(s.client.get_provider_revenue(&s.provider), 100);
    assert_eq!(balance(&s, &subscriber), 900);
}

#[test]
fn test_access_revoked_once_grace_period_lapses() {
    let s = setup();
    let subscriber = Address::generate(&s.env);
    let plan = Symbol::new(&s.env, "basic");
    s.client.register_plan(&plan, &100, &10, &0, &0);
    s.client.set_grace_period(&50);

    mint(&s, &subscriber, 1_000);
    s.client.subscribe(&subscriber, &plan, &None, &None, &None);

    advance(&s.env, 10);
    assert_eq!(
        s.client.charge(&subscriber, &plan),
        ChargeOutcome::PaymentFailed
    );
    assert!(s.client.has_access(&subscriber, &plan));

    // Once the window has passed access ends, even before the next attempt.
    advance(&s.env, 50);
    assert!(!s.client.has_access(&subscriber, &plan));

    // The next failed attempt suspends the subscription.
    assert_eq!(s.client.charge(&subscriber, &plan), ChargeOutcome::Suspended);
    let info = s.client.get_subscription(&subscriber, &plan).unwrap();
    assert!(info.suspended);
    assert!(!info.active);
    assert!(!s.client.has_access(&subscriber, &plan));
}

// ---------------------------------------------------------------------------
// Issue #1484: keeper-bot recurring billing
// ---------------------------------------------------------------------------

#[test]
fn test_keeper_charges_due_subscription_with_bounty_and_interval_enforcement() {
    let s = setup();
    let subscriber = Address::generate(&s.env);
    let keeper = Address::generate(&s.env);
    let plan = Symbol::new(&s.env, "pro");
    s.client.register_plan(&plan, &1_000, &10, &0, &0);
    s.client.set_keeper_bounty(&100); // 1% of every charge
    assert_eq!(s.client.get_keeper_bounty(), 100);

    mint(&s, &subscriber, 10_000);
    approve(&s, &subscriber, 10_000);
    s.client.subscribe(&subscriber, &plan, &None, &None, &None);

    advance(&s.env, 10);

    // Unregistered bots cannot bill subscribers.
    assert!(!s.client.is_keeper(&keeper));
    assert_eq!(
        s.client.try_charge_subscriber(&keeper, &subscriber, &plan),
        Err(Ok(SubscriptionError::KeeperNotRegistered))
    );

    s.client.register_keeper(&keeper);
    assert!(s.client.is_keeper(&keeper));

    // Due: the keeper collects the charge and earns the bounty from the fee.
    assert_eq!(
        s.client.charge_subscriber(&keeper, &subscriber, &plan),
        ChargeOutcome::Charged
    );
    assert_eq!(balance(&s, &keeper), 10);
    assert_eq!(s.client.get_provider_revenue(&s.provider), 990);
    assert_eq!(balance(&s, &subscriber), 9_000);

    // Duplicate call in the same interval is rejected.
    assert_eq!(
        s.client.try_charge_subscriber(&keeper, &subscriber, &plan),
        Err(Ok(SubscriptionError::IntervalNotElapsed))
    );

    // Premature call part-way through the next interval is rejected too.
    advance(&s.env, 9);
    assert_eq!(
        s.client.try_charge_subscriber(&keeper, &subscriber, &plan),
        Err(Ok(SubscriptionError::IntervalNotElapsed))
    );
    assert_eq!(balance(&s, &subscriber), 9_000);

    // Once the interval elapses the keeper can bill again.
    advance(&s.env, 1);
    assert_eq!(
        s.client.charge_subscriber(&keeper, &subscriber, &plan),
        ChargeOutcome::Charged
    );
    assert_eq!(balance(&s, &keeper), 20);
    assert_eq!(s.client.get_provider_revenue(&s.provider), 1_980);
    assert_eq!(balance(&s, &subscriber), 8_000);

    // A removed keeper loses billing rights.
    s.client.remove_keeper(&keeper);
    assert!(!s.client.is_keeper(&keeper));
    advance(&s.env, 10);
    assert_eq!(
        s.client.try_charge_subscriber(&keeper, &subscriber, &plan),
        Err(Ok(SubscriptionError::KeeperNotRegistered))
    );
}

#[test]
fn test_keeper_failed_charge_enters_grace_period_without_bounty() {
    let s = setup();
    let subscriber = Address::generate(&s.env);
    let keeper = Address::generate(&s.env);
    let plan = Symbol::new(&s.env, "pro");
    s.client.register_plan(&plan, &1_000, &10, &0, &0);
    s.client.set_keeper_bounty(&500);
    s.client.register_keeper(&keeper);

    // No allowance: the keeper's attempt fails but does not cancel the plan.
    mint(&s, &subscriber, 10_000);
    s.client.subscribe(&subscriber, &plan, &None, &None, &None);
    advance(&s.env, 10);

    assert_eq!(
        s.client.charge_subscriber(&keeper, &subscriber, &plan),
        ChargeOutcome::PaymentFailed
    );
    assert_eq!(balance(&s, &keeper), 0);
    assert!(s.client.has_access(&subscriber, &plan));

    // The keeper may retry during the grace period once funds are approved.
    approve(&s, &subscriber, 10_000);
    advance(&s.env, 1);
    assert_eq!(
        s.client.charge_subscriber(&keeper, &subscriber, &plan),
        ChargeOutcome::Charged
    );
    assert_eq!(balance(&s, &keeper), 50);
    assert_eq!(s.client.get_provider_revenue(&s.provider), 950);
}

#[test]
fn test_keeper_bounty_rejects_more_than_100_percent() {
    let s = setup();
    assert_eq!(
        s.client.try_set_keeper_bounty(&10_001),
        Err(Ok(SubscriptionError::InvalidBounty))
    );
    s.client.set_keeper_bounty(&10_000);
    assert_eq!(s.client.get_keeper_bounty(), 10_000);
}

#[test]
fn test_remove_unregistered_keeper_fails() {
    let s = setup();
    let keeper = Address::generate(&s.env);
    assert_eq!(
        s.client.try_remove_keeper(&keeper),
        Err(Ok(SubscriptionError::KeeperNotRegistered))
    );
}
