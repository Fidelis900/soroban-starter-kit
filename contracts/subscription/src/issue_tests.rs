#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]
#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Symbol,
};

use crate::{
    ChargeOutcome, PRICE_NOTICE_LEDGERS, SubscriptionContract, SubscriptionContractClient,
    SubscriptionError,
};

fn setup(env: &Env) -> (SubscriptionContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(SubscriptionContract, ());
    let client = SubscriptionContractClient::new(env, &contract_id);
    let provider = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    client.initialize(&provider, &token_id);
    (client, contract_id, provider, token_id)
}

fn mint_and_approve(
    env: &Env,
    contract_id: &Address,
    token_id: &Address,
    subscriber: &Address,
    amount: i128,
) {
    let admin = token::StellarAssetClient::new(env, token_id);
    admin.mint(subscriber, &amount);
    let tc = token::Client::new(env, token_id);
    tc.approve(subscriber, contract_id, &amount, &200_000);
}

#[test]
fn test_grandfathered_rate_and_approval_required_for_increase() {
    let env = Env::default();
    let (client, contract_id, _provider, token_id) = setup(&env);
    let subscriber = Address::generate(&env);
    let plan = Symbol::new(&env, "basic");

    client.register_plan(&plan, &100, &10, &0, &0);
    mint_and_approve(&env, &contract_id, &token_id, &subscriber, 1_000);
    client.subscribe(&subscriber, &plan, &None, &None, &None);

    client.update_plan_amount(&plan, &150);
    let info = client.get_subscription(&subscriber, &plan).unwrap();
    assert_eq!(info.amount, 100, "existing subscriber must remain grandfathered");

    env.ledger().with_mut(|l| l.sequence_number += 10);
    let outcome = client.charge(&subscriber, &plan);
    assert_eq!(outcome, ChargeOutcome::Charged);
    let tc = token::Client::new(&env, &token_id);
    assert_eq!(tc.balance(&contract_id), 100);
    assert_eq!(client.get_provider_revenue(&_provider), 100);

    client.approve_price_increase(&subscriber, &plan);
    let info = client.get_subscription(&subscriber, &plan).unwrap();
    assert_eq!(info.amount, 150, "approved increase should apply");
}

#[test]
fn test_can_charge_validates_timing_balance_and_allowance() {
    let env = Env::default();
    let (client, contract_id, _provider, token_id) = setup(&env);
    let subscriber = Address::generate(&env);
    let plan = Symbol::new(&env, "basic");

    client.register_plan(&plan, &100, &10, &0, &0);
    // subscribe but do not fund/approve
    client.subscribe(&subscriber, &plan, &None, &None, &None);

    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert!(!client.can_charge(&subscriber, &plan), "no balance/allowance");

    mint_and_approve(&env, &contract_id, &token_id, &subscriber, 1_000);
    assert!(client.can_charge(&subscriber, &plan), "due and funded");

    // not due yet after charge timing reset? before any charge, interval elapsed at +10 means due; simulate already charged state
    // create a fresh case for not-due check
    let subscriber2 = Address::generate(&env);
    mint_and_approve(&env, &contract_id, &token_id, &subscriber2, 1_000);
    client.subscribe(&subscriber2, &plan, &None, &None, &None);
    assert!(!client.can_charge(&subscriber2, &plan), "interval not elapsed yet");
}

// ---------------------------------------------------------------------------
// Pause / resume: freeze billing without cancelling
// ---------------------------------------------------------------------------

#[test]
fn pause_subscription_halts_billing_and_resume_freezes_interval() {
    let env = Env::default();
    let (client, contract_id, _provider, token_id) = setup(&env);
    let subscriber = Address::generate(&env);
    let plan = Symbol::new(&env, "basic");

    client.register_plan(&plan, &100, &10, &0, &0);
    mint_and_approve(&env, &contract_id, &token_id, &subscriber, 1_000);
    client.subscribe(&subscriber, &plan, &None, &None, &None);

    // First charge at ledger +10 covers the next interval.
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert_eq!(client.charge(&subscriber, &plan), ChargeOutcome::Charged);
    let tc = token::Client::new(&env, &token_id);
    assert_eq!(tc.balance(&subscriber), 900);
    assert_eq!(client.get_provider_revenue(&_provider), 100);

    // Subscriber pauses: billing halts and access is withdrawn.
    client.pause_subscription(&subscriber, &plan);
    assert!(!client.has_access(&subscriber, &plan), "no access while paused");

    // While paused, a charge is rejected and no tokens move, even once the
    // interval would have come due.
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert_eq!(
        client.try_charge(&subscriber, &plan),
        Err(Ok(SubscriptionError::SubscriptionPaused))
    );
    assert_eq!(tc.balance(&subscriber), 900, "no deduction while paused");

    // Pausing again is rejected.
    assert_eq!(
        client.try_pause_subscription(&subscriber, &plan),
        Err(Ok(SubscriptionError::AlreadyPaused))
    );

    // Resume shifts the billing interval forward by the 10 paused ledgers, so
    // the paid period is not consumed: the next charge is due at +30, not +20.
    client.resume_subscription(&subscriber, &plan);
    assert!(client.has_access(&subscriber, &plan), "access restored on resume");
    env.ledger().with_mut(|l| l.sequence_number += 9);
    assert_eq!(
        client.try_charge(&subscriber, &plan),
        Err(Ok(SubscriptionError::IntervalNotElapsed)),
        "interval was frozen during the pause"
    );
    env.ledger().with_mut(|l| l.sequence_number += 1);
    assert_eq!(client.charge(&subscriber, &plan), ChargeOutcome::Charged);
    assert_eq!(tc.balance(&subscriber), 800, "only the resumed charge is taken");
    assert_eq!(client.get_provider_revenue(&_provider), 200);

    // Resuming a subscription that is not paused is rejected.
    assert_eq!(
        client.try_resume_subscription(&subscriber, &plan),
        Err(Ok(SubscriptionError::NotPaused))
    );
}

// ---------------------------------------------------------------------------
// Pro-rated credit when switching tiers mid-cycle
// ---------------------------------------------------------------------------

#[test]
fn change_plan_charges_prorated_upgrade_credit() {
    let env = Env::default();
    let (client, contract_id, provider, token_id) = setup(&env);
    let subscriber = Address::generate(&env);
    let basic = Symbol::new(&env, "basic");
    let pro = Symbol::new(&env, "pro");

    client.register_plan(&basic, &100, &10, &0, &0);
    client.register_plan(&pro, &300, &10, &0, &0);
    mint_and_approve(&env, &contract_id, &token_id, &subscriber, 1_000);
    client.subscribe(&subscriber, &basic, &None, &None, &None);

    // Pay one full interval on the basic plan.
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert_eq!(client.charge(&subscriber, &basic), ChargeOutcome::Charged);

    // Three ledgers into the paid interval (7 remaining), switch to the pro
    // tier. Unused credit = 100 * 7 / 10 = 70; upgrade shortfall = 300 - 70 = 230.
    env.ledger().with_mut(|l| l.sequence_number += 3);
    client.change_plan(&subscriber, &basic, &pro);

    let tc = token::Client::new(&env, &token_id);
    // 100 (first interval) + 230 (prorated upgrade) spent.
    assert_eq!(tc.balance(&subscriber), 1_000 - 100 - 230);
    // The upgrade charge is paid directly to the provider, not into escrow.
    assert_eq!(tc.balance(&provider), 230);
    assert_eq!(client.get_provider_revenue(&provider), 100);

    let info = client.get_subscription(&subscriber, &pro).unwrap();
    assert_eq!(info.amount, 300, "new tier amount applied");
    assert_eq!(
        info.last_charged_ledger,
        env.ledger().sequence(),
        "fresh interval starts now"
    );
    assert!(info.active);
    assert!(!info.paused);
    // The old plan subscription is deactivated but its record is retained.
    assert!(!client.get_subscription(&subscriber, &basic).unwrap().active);
}

// ---------------------------------------------------------------------------
// 30-day advance notice before a price increase takes effect
// ---------------------------------------------------------------------------

#[test]
fn scheduled_price_increase_takes_effect_after_notice_window() {
    let env = Env::default();
    let (client, contract_id, _provider, token_id) = setup(&env);
    let subscriber = Address::generate(&env);
    let plan = Symbol::new(&env, "basic");

    client.register_plan(&plan, &100, &10, &0, &0);
    mint_and_approve(&env, &contract_id, &token_id, &subscriber, 10_000);
    // The 30-day notice window outlives the helper's default allowance expiry,
    // so extend the subscriber's allowance to cover the delayed charge.
    token::Client::new(&env, &token_id)
        .approve(&subscriber, &contract_id, &10_000, &1_000_000);
    client.subscribe(&subscriber, &plan, &None, &None, &None);

    // Provider raises the price 100 -> 200. The increase is only scheduled: the
    // plan amount is unchanged and a 30-day notice window is recorded.
    let scheduled_at = env.ledger().sequence();
    client.update_plan_amount(&plan, &200);
    let (new_amount, effective) = client.get_pending_price_increase(&plan).unwrap();
    assert_eq!(new_amount, 200);
    assert_eq!(effective, scheduled_at + PRICE_NOTICE_LEDGERS);
    assert_eq!(
        client.get_plan(&plan).unwrap().amount,
        100,
        "old price still in effect during the notice window"
    );

    // During the notice window the subscriber is still charged the old rate.
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert_eq!(client.charge(&subscriber, &plan), ChargeOutcome::Charged);
    let tc = token::Client::new(&env, &token_id);
    assert_eq!(tc.balance(&subscriber), 10_000 - 100);

    // A subscriber may cancel during the notice window without ever paying the
    // new rate and without penalty.
    let sub2 = Address::generate(&env);
    mint_and_approve(&env, &contract_id, &token_id, &sub2, 10_000);
    client.subscribe(&sub2, &plan, &None, &None, &None);
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert_eq!(client.charge(&sub2, &plan), ChargeOutcome::Charged);
    client.cancel(&sub2, &plan);
    assert!(!client.get_subscription(&sub2, &plan).unwrap().active);
    assert_eq!(tc.balance(&sub2), 10_000 - 100, "only the old rate was charged");

    // Once the notice window elapses, the next charge applies the new price.
    env.ledger().with_mut(|l| l.sequence_number += PRICE_NOTICE_LEDGERS);
    assert_eq!(client.charge(&subscriber, &plan), ChargeOutcome::Charged);
    assert_eq!(tc.balance(&subscriber), 10_000 - 100 - 200, "new rate applied");
    assert_eq!(
        client.get_subscription(&subscriber, &plan).unwrap().amount,
        200,
        "delayed increase took effect"
    );
    assert_eq!(
        client.get_plan(&plan).unwrap().amount,
        200,
        "plan price is now public for new subscribers"
    );
}
