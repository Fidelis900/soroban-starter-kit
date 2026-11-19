#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, vec,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

use crate::{DEFAULT_ROTATION_DELAY, VestingContract, VestingContractClient, VestingError};

// ── helpers ──────────────────────────────────────────────────────────────────

const AMOUNT: i128 = 1_000;

struct T<'a> {
    env: Env,
    client: VestingContractClient<'a>,
    admin: Address,
    beneficiary: Address,
    token: Address,
    cliff: u32,
    end: u32,
}

impl T<'_> {
    fn balance(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.token).balance(who)
    }
    fn goto(&self, ledger: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number = ledger);
    }
}

/// Linear schedule: cliff at 110, fully vested at 210 (1_000 tokens).
fn setup(env: &Env) -> T<'_> {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 100);

    let admin = Address::generate(env);
    let beneficiary = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    StellarAssetClient::new(env, &token).mint(&admin, &(AMOUNT * 10));

    let client = VestingContractClient::new(env, &env.register(VestingContract, ()));
    client.initialize(&admin, &token);
    let cliff = 110;
    let end = 210;
    client.create_schedule(&beneficiary, &cliff, &end, &AMOUNT, &true);

    T { env: env.clone(), client, admin, beneficiary, token, cliff, end }
}

// ── core behaviour ───────────────────────────────────────────────────────────

#[test]
fn initialize_and_create_schedule() {
    let env = Env::default();
    let t = setup(&env);
    let info = t.client.get_info(&t.beneficiary).unwrap();
    assert_eq!((info.amount, info.cliff_ledger, info.end_ledger), (AMOUNT, t.cliff, t.end));
    assert_eq!(info.claimed, 0);
    assert_eq!(
        t.client.try_initialize(&t.admin, &t.token),
        Err(Ok(VestingError::AlreadyInitialized))
    );
    assert_eq!(
        t.client
            .try_create_schedule(&t.beneficiary, &t.cliff, &t.end, &AMOUNT, &true),
        Err(Ok(VestingError::ScheduleAlreadyExists))
    );
}

#[test]
fn invalid_schedules_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let other = Address::generate(&env);
    assert_eq!(
        t.client.try_create_schedule(&other, &200, &150, &AMOUNT, &true),
        Err(Ok(VestingError::InvalidSchedule))
    );
    assert_eq!(
        t.client.try_create_schedule(&other, &110, &200, &0, &true),
        Err(Ok(VestingError::InvalidAmount))
    );
}

#[test]
fn linear_claims() {
    let env = Env::default();
    let t = setup(&env);
    assert_eq!(t.client.try_claim(&t.beneficiary), Err(Ok(VestingError::NothingToClaim)));

    t.goto(t.cliff + 50);
    assert_eq!(t.client.claim(&t.beneficiary), AMOUNT / 2);

    t.goto(t.end + 10);
    assert_eq!(t.client.claim(&t.beneficiary), AMOUNT / 2);
    assert_eq!(t.balance(&t.beneficiary), AMOUNT);
    assert_eq!(t.client.try_claim(&t.beneficiary), Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn revoke_returns_unvested_and_keeps_vested_claimable() {
    let env = Env::default();
    let t = setup(&env);
    let admin_before = t.balance(&t.admin);

    t.goto(t.cliff + 25);
    assert_eq!(t.client.revoke(&t.beneficiary), 750);
    assert_eq!(t.balance(&t.admin), admin_before + 750);

    t.goto(t.end);
    assert_eq!(t.client.claim(&t.beneficiary), 250);
    assert_eq!(t.client.try_revoke(&t.beneficiary), Err(Ok(VestingError::AlreadyRevoked)));
}

#[test]
fn irrevocable_schedule_cannot_be_revoked() {
    let env = Env::default();
    let t = setup(&env);
    let b = Address::generate(&env);
    t.client.create_schedule(&b, &t.cliff, &t.end, &AMOUNT, &false);
    assert_eq!(t.client.try_revoke(&b), Err(Ok(VestingError::ScheduleIrrevocable)));
}

#[test]
fn tranche_schedule_vests_in_steps() {
    let env = Env::default();
    let t = setup(&env);
    let b = Address::generate(&env);
    t.client
        .create_tranche_schedule(&b, &vec![&env, (120, 2_500), (150, 7_500)], &AMOUNT, &true);

    t.goto(130);
    assert_eq!(t.client.claim(&b), 250);
    t.goto(150);
    assert_eq!(t.client.claim(&b), 750);
}

// ── #1345: minimum claim threshold ───────────────────────────────────────────

#[test]
fn dust_claim_below_minimum_rejected() {
    let env = Env::default();
    let t = setup(&env);
    t.client.set_min_claim_amount(&Some(100));
    assert_eq!(t.client.min_claim_amount(), Some(100));

    // 1 ledger after the cliff only 10 tokens are vested: a dust claim.
    t.goto(t.cliff + 1);
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::ClaimBelowMinimum))
    );
    assert_eq!(t.client.get_info(&t.beneficiary).unwrap().claimed, 0);

    // Once enough has accrued the claim goes through.
    t.goto(t.cliff + 10);
    assert_eq!(t.client.claim(&t.beneficiary), 100);
}

#[test]
fn final_remainder_below_minimum_allowed() {
    let env = Env::default();
    let t = setup(&env);
    t.client.set_min_claim_amount(&Some(100));

    t.goto(t.cliff + 95);
    assert_eq!(t.client.claim(&t.beneficiary), 950);

    // Remaining 50 is below the threshold but completes the schedule.
    t.goto(t.end);
    assert_eq!(t.client.claim(&t.beneficiary), 50);
    assert_eq!(t.balance(&t.beneficiary), AMOUNT);
}

#[test]
fn min_claim_amount_validation_and_disable() {
    let env = Env::default();
    let t = setup(&env);
    assert_eq!(
        t.client.try_set_min_claim_amount(&Some(0)),
        Err(Ok(VestingError::InvalidAmount))
    );
    t.client.set_min_claim_amount(&Some(500));
    t.client.set_min_claim_amount(&None);
    assert_eq!(t.client.min_claim_amount(), None);
    t.goto(t.cliff + 1);
    assert_eq!(t.client.claim(&t.beneficiary), 10);
}

// ── #1344: beneficiary key rotation ──────────────────────────────────────────

#[test]
fn beneficiary_rotation_after_timelock() {
    let env = Env::default();
    let t = setup(&env);
    let new_key = Address::generate(&env);

    t.goto(t.cliff + 20);
    t.client.claim(&t.beneficiary); // 200 already claimed under the old key

    let eta = t
        .client
        .propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &new_key);
    assert_eq!(eta, t.cliff + 20 + DEFAULT_ROTATION_DELAY);

    // Timelock protects the rotation.
    assert_eq!(
        t.client.try_accept_beneficiary_rotation(&t.beneficiary),
        Err(Ok(VestingError::RotationTimelockActive))
    );

    t.goto(eta);
    t.client.accept_beneficiary_rotation(&t.beneficiary);

    assert_eq!(t.client.get_info(&t.beneficiary), None);
    assert_eq!(t.client.get_pending_rotation(&t.beneficiary), None);
    let info = t.client.get_info(&new_key).unwrap();
    assert_eq!((info.amount, info.claimed), (AMOUNT, 200));

    // Old key can no longer claim; new key receives the rest.
    assert_eq!(t.client.try_claim(&t.beneficiary), Err(Ok(VestingError::ScheduleNotFound)));
    assert_eq!(t.client.claim(&new_key), 800);
    assert_eq!(t.balance(&new_key), 800);
}

#[test]
fn admin_rotation_freezes_claims_and_cannot_be_overridden_by_beneficiary() {
    let env = Env::default();
    let t = setup(&env);
    let safe = Address::generate(&env);
    let attacker = Address::generate(&env);
    t.client.set_rotation_delay(&50);

    t.client
        .propose_beneficiary_rotation(&t.admin, &t.beneficiary, &safe);

    // A compromised beneficiary key cannot drain, redirect, or cancel.
    t.goto(t.end);
    assert_eq!(t.client.try_claim(&t.beneficiary), Err(Ok(VestingError::RotationPending)));
    assert_eq!(
        t.client
            .try_propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &attacker),
        Err(Ok(VestingError::RotationPending))
    );
    assert_eq!(
        t.client
            .try_cancel_beneficiary_rotation(&t.beneficiary, &t.beneficiary),
        Err(Ok(VestingError::NotAuthorized))
    );

    t.client.accept_beneficiary_rotation(&t.beneficiary);
    assert_eq!(t.client.claim(&safe), AMOUNT);
}

#[test]
fn rotation_rejects_unauthorized_and_invalid_targets() {
    let env = Env::default();
    let t = setup(&env);
    let stranger = Address::generate(&env);
    let other = Address::generate(&env);
    t.client.create_schedule(&other, &t.cliff, &t.end, &AMOUNT, &true);

    assert_eq!(
        t.client
            .try_propose_beneficiary_rotation(&stranger, &t.beneficiary, &stranger),
        Err(Ok(VestingError::NotAuthorized))
    );
    assert_eq!(
        t.client
            .try_propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &t.beneficiary),
        Err(Ok(VestingError::InvalidBeneficiary))
    );
    assert_eq!(
        t.client
            .try_propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &other),
        Err(Ok(VestingError::BeneficiaryAlreadyExists))
    );
    assert_eq!(
        t.client.try_accept_beneficiary_rotation(&t.beneficiary),
        Err(Ok(VestingError::RotationNotFound))
    );
}

#[test]
fn admin_can_cancel_rotation() {
    let env = Env::default();
    let t = setup(&env);
    let new_key = Address::generate(&env);
    t.client
        .propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &new_key);
    t.client.cancel_beneficiary_rotation(&t.admin, &t.beneficiary);

    t.goto(t.end + DEFAULT_ROTATION_DELAY);
    assert_eq!(
        t.client.try_accept_beneficiary_rotation(&t.beneficiary),
        Err(Ok(VestingError::RotationNotFound))
    );
    assert_eq!(t.client.claim(&t.beneficiary), AMOUNT);
}

#[test]
fn rotation_requires_new_beneficiary_auth() {
    let env = Env::default();
    let t = setup(&env);
    let new_key = Address::generate(&env);
    t.client.set_rotation_delay(&0);
    t.client
        .propose_beneficiary_rotation(&t.beneficiary, &t.beneficiary, &new_key);

    t.client.accept_beneficiary_rotation(&t.beneficiary);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, new_key);
}
