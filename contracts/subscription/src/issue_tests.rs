#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]
#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Symbol,
};

use crate::{ChargeOutcome, SubscriptionContract, SubscriptionContractClient};

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
