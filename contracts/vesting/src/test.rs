#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, Vec,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec,
};

use crate::{
    DEFAULT_ROTATION_DELAY, ScheduleParams, VestingContract, VestingContractClient, VestingError,
};

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
    client.create_schedule(&beneficiary, &cliff, &cliff, &end, &AMOUNT, &true);

    T {
        env: env.clone(),
        client,
        admin,
        beneficiary,
        token,
        cliff,
        end,
    }
}

// ── core behaviour ───────────────────────────────────────────────────────────

#[test]
fn initialize_and_create_schedule() {
    let env = Env::default();
    let t = setup(&env);
    let info = t.client.get_info(&t.beneficiary).unwrap();
    assert_eq!(
        (info.amount, info.cliff_ledger, info.end_ledger),
        (AMOUNT, t.cliff, t.end)
    );
    assert_eq!(info.claimed, 0);
    assert_eq!(
        t.client.try_initialize(&t.admin, &t.token),
        Err(Ok(VestingError::AlreadyInitialized))
    );
    assert_eq!(
        t.client
            .try_create_schedule(&t.beneficiary, &t.cliff, &t.cliff, &t.end, &AMOUNT, &true),
        Err(Ok(VestingError::ScheduleAlreadyExists))
    );
}

#[test]
fn invalid_schedules_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let other = Address::generate(&env);
    assert_eq!(
        t.client
            .try_create_schedule(&other, &200, &200, &150, &AMOUNT, &true),
        Err(Ok(VestingError::InvalidScheduleBounds))
    );
    assert_eq!(
        t.client
            .try_create_schedule(&other, &110, &110, &200, &0, &true),
        Err(Ok(VestingError::InvalidAmount))
    );
}

#[test]
fn linear_claims() {
    let env = Env::default();
    let t = setup(&env);
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::NothingToClaim))
    );

    t.goto(t.cliff + 50);
    assert_eq!(t.client.claim(&t.beneficiary), AMOUNT / 2);

    t.goto(t.end + 10);
    assert_eq!(t.client.claim(&t.beneficiary), AMOUNT / 2);
    assert_eq!(t.balance(&t.beneficiary), AMOUNT);
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::NothingToClaim))
    );
}

#[test]
fn revoke_pays_vested_and_returns_unvested() {
    let env = Env::default();
    let t = setup(&env);
    let admin_before = t.balance(&t.admin);

    t.goto(t.cliff + 25);
    assert_eq!(t.client.revoke(&t.beneficiary), (250, 750));
    assert_eq!(t.balance(&t.admin), admin_before + 750);
    assert_eq!(t.balance(&t.beneficiary), 250);

    t.goto(t.end);
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::NothingToClaim))
    );
    assert_eq!(
        t.client.try_revoke(&t.beneficiary),
        Err(Ok(VestingError::AlreadyRevoked))
    );
}

#[test]
fn revoke_with_grace_period_caps_vesting_at_finalize() {
    let env = Env::default();
    let t = setup(&env);
    t.client.set_revocation_delay(&25);

    t.goto(t.cliff + 25);
    assert_eq!(t.client.revoke(&t.beneficiary), (0, 0));
    assert_eq!(
        t.client.try_finalize_revocation(&t.beneficiary),
        Err(Ok(VestingError::RevocationNotDue))
    );

    // Vesting keeps accruing during the grace period.
    t.goto(t.cliff + 40);
    assert_eq!(t.client.claim(&t.beneficiary), 400);

    // Finalized late: vesting is capped at the end of the grace period.
    t.goto(t.end);
    assert_eq!(t.client.finalize_revocation(&t.beneficiary), (100, 500));
    assert_eq!(t.balance(&t.beneficiary), 500);
    assert_eq!(t.balance(&t.client.address), 0);
}

#[test]
fn irrevocable_schedule_cannot_be_revoked() {
    let env = Env::default();
    let t = setup(&env);
    let b = Address::generate(&env);
    t.client
        .create_schedule(&b, &t.cliff, &t.cliff, &t.end, &AMOUNT, &false);
    assert_eq!(
        t.client.try_revoke(&b),
        Err(Ok(VestingError::ScheduleIrrevocable))
    );
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
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::ScheduleNotFound))
    );
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
    assert_eq!(
        t.client.try_claim(&t.beneficiary),
        Err(Ok(VestingError::RotationPending))
    );
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
    t.client
        .create_schedule(&other, &t.cliff, &t.cliff, &t.end, &AMOUNT, &true);

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
    t.client
        .cancel_beneficiary_rotation(&t.admin, &t.beneficiary);

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

// ── #1340–#1343: bounds, milestones, batch creation, revocation math ─────────

struct Ctx<'a> {
    env: Env,
    client: VestingContractClient<'a>,
    admin: Address,
    token: Address,
}

fn setup_ctx<'a>() -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 100);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = sac.address();
    StellarAssetClient::new(&env, &token).mint(&admin, &1_000_000);
    let id = env.register(VestingContract, ());
    let client = VestingContractClient::new(&env, &id);
    client.initialize(&admin, &token);
    Ctx {
        env,
        client,
        admin,
        token,
    }
}

fn bal(c: &Ctx, a: &Address) -> i128 {
    TokenClient::new(&c.env, &c.token).balance(a)
}

fn at(c: &Ctx, seq: u32) {
    c.env.ledger().with_mut(|l| l.sequence_number = seq);
}

// ── #1340 bounds validation ──────────────────────────────────────────────────

#[test]
fn bounds_valid_and_edges() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    // start == now, start == cliff allowed
    c.client.create_schedule(&b, &100, &100, &200, &1000, &true);
    assert_eq!(c.client.get_info(&b).unwrap().amount, 1000);
}

#[test]
fn bounds_invalid_rejected() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let bad = [
        (99, 150, 200),  // start in the past
        (120, 110, 200), // cliff < start
        (100, 150, 150), // end == cliff
        (100, 150, 140), // end < cliff
    ];
    for (s, cl, e) in bad {
        let r = c.client.try_create_schedule(&b, &s, &cl, &e, &1000, &true);
        assert_eq!(
            r,
            Err(Ok(VestingError::InvalidScheduleBounds)),
            "{s} {cl} {e}"
        );
    }
}

// ── #1342 batch ──────────────────────────────────────────────────────────────

fn params(c: &Ctx, amount: i128) -> ScheduleParams {
    ScheduleParams {
        beneficiary: Address::generate(&c.env),
        start_ledger: 100,
        cliff_ledger: 110,
        end_ledger: 200,
        amount,
        is_revocable: true,
    }
}

#[test]
fn batch_creates_all_and_funds_once() {
    let c = setup_ctx();
    let p: Vec<ScheduleParams> = vec![&c.env, params(&c, 100), params(&c, 250), params(&c, 50)];
    let before = bal(&c, &c.admin);
    c.client.create_schedules_batch(&c.admin, &p);
    assert_eq!(before - bal(&c, &c.admin), 400);
    assert_eq!(bal(&c, &c.client.address), 400);
    for s in p.iter() {
        assert_eq!(c.client.get_info(&s.beneficiary).unwrap().amount, s.amount);
    }
}

#[test]
fn batch_is_atomic_on_invalid_entry() {
    let c = setup_ctx();
    let mut bad = params(&c, 100);
    bad.end_ledger = 100;
    let good = params(&c, 100);
    let p = vec![&c.env, good.clone(), bad];
    assert!(c.client.try_create_schedules_batch(&c.admin, &p).is_err());
    assert!(c.client.get_info(&good.beneficiary).is_none());
    assert_eq!(bal(&c, &c.client.address), 0);
}

#[test]
fn batch_requires_admin_and_nonempty() {
    let c = setup_ctx();
    let p = vec![&c.env, params(&c, 10)];
    let other = Address::generate(&c.env);
    assert_eq!(
        c.client.try_create_schedules_batch(&other, &p),
        Err(Ok(VestingError::NotAuthorized))
    );
    assert_eq!(
        c.client
            .try_create_schedules_batch(&c.admin, &Vec::new(&c.env)),
        Err(Ok(VestingError::InvalidSchedule))
    );
}

// ── #1341 milestones ─────────────────────────────────────────────────────────

#[test]
fn milestone_unlocks_on_approval() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 3000u32), (2, 3000), (3, 4000)];
    c.client
        .create_milestone_schedule(&b, &v, &ms, &1000, &true);

    assert_eq!(
        c.client.try_claim(&b),
        Err(Ok(VestingError::NothingToClaim))
    );
    c.client.approve_milestone(&b, &1);
    assert_eq!(c.client.claim(&b), 300);
    c.client.approve_milestone(&b, &3);
    assert_eq!(c.client.claim(&b), 400);
    c.client.approve_milestone(&b, &2);
    assert_eq!(c.client.claim(&b), 300);
    assert_eq!(bal(&c, &b), 1000);
}

#[test]
fn milestone_errors() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 5000u32), (2, 5000)];
    c.client
        .create_milestone_schedule(&b, &v, &ms, &1000, &true);
    assert_eq!(
        c.client.try_approve_milestone(&b, &9),
        Err(Ok(VestingError::MilestoneNotFound))
    );
    c.client.approve_milestone(&b, &1);
    assert_eq!(
        c.client.try_approve_milestone(&b, &1),
        Err(Ok(VestingError::MilestoneAlreadyApproved))
    );
    // weights must sum to 10_000
    let b2 = Address::generate(&c.env);
    let bad = vec![&c.env, (1u32, 5000u32)];
    assert_eq!(
        c.client
            .try_create_milestone_schedule(&b2, &v, &bad, &1000, &true),
        Err(Ok(VestingError::InvalidSchedule))
    );
    // linear schedule has no verifier
    let b3 = Address::generate(&c.env);
    c.client.create_schedule(&b3, &100, &110, &200, &100, &true);
    assert_eq!(
        c.client.try_approve_milestone(&b3, &1),
        Err(Ok(VestingError::NotMilestoneSchedule))
    );
}

#[test]
fn milestone_approval_needs_verifier_auth() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 10_000u32)];
    c.client
        .create_milestone_schedule(&b, &v, &ms, &500, &false);
    c.client.approve_milestone(&b, &1);
    let auths = c.env.auths();
    assert_eq!(auths.last().unwrap().0, v);
}

// ── #1343 revocation math ────────────────────────────────────────────────────

/// Revoke at `seq` for a 1000-token schedule (start 100, cliff 150, end 250)
/// and assert exact conservation.
fn check_revoke(seq: u32, claim_first_at: Option<u32>) -> (i128, i128) {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &true);
    let admin_start = bal(&c, &c.admin);
    let mut claimed = 0;
    if let Some(t) = claim_first_at {
        at(&c, t);
        claimed = c.client.claim(&b);
    }
    at(&c, seq);
    let (pay, claw) = c.client.revoke(&b);
    assert_eq!(claimed + pay + claw, 1000, "conservation at {seq}");
    assert_eq!(bal(&c, &b), claimed + pay);
    assert_eq!(bal(&c, &c.admin) - admin_start, claw);
    assert_eq!(bal(&c, &c.client.address), 0);
    (pay, claw)
}

#[test]
fn revoke_before_cliff_claws_back_everything() {
    assert_eq!(check_revoke(120, None), (0, 1000));
    assert_eq!(check_revoke(149, None), (0, 1000));
}

#[test]
fn revoke_at_and_after_cliff_pays_exact_vested() {
    // at cliff: 1000 * 50 / 150 = 333 (rounded down), clawback gets the dust
    assert_eq!(check_revoke(150, None), (333, 667));
    assert_eq!(check_revoke(151, None), (340, 660));
    assert_eq!(check_revoke(200, None), (666, 334));
}

#[test]
fn revoke_after_end_and_after_partial_claim() {
    assert_eq!(check_revoke(300, None), (1000, 0));
    let (pay, claw) = check_revoke(200, Some(160));
    assert_eq!(pay + claw, 1000 - 400); // 1000*60/150 = 400 claimed earlier
}

#[test]
fn revoke_guards() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    c.client
        .create_schedule(&b, &100, &150, &250, &1000, &false);
    assert_eq!(
        c.client.try_revoke(&b),
        Err(Ok(VestingError::ScheduleIrrevocable))
    );
    let b2 = Address::generate(&c.env);
    c.client
        .create_schedule(&b2, &100, &150, &250, &1000, &true);
    c.client.revoke(&b2);
    assert_eq!(
        c.client.try_revoke(&b2),
        Err(Ok(VestingError::AlreadyRevoked))
    );
    assert_eq!(
        c.client.try_claim(&b2),
        Err(Ok(VestingError::NothingToClaim))
    );
}

#[test]
fn milestone_revoke_conserves() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 3333u32), (2, 6667)];
    c.client
        .create_milestone_schedule(&b, &v, &ms, &1001, &true);
    c.client.approve_milestone(&b, &1);
    let (pay, claw) = c.client.revoke(&b);
    assert_eq!(pay + claw, 1001);
    assert_eq!(pay, 333); // 1001*3333/10000 rounded down
}

// ── #1455 revoke_grant ───────────────────────────────────────────────────────

#[test]
fn revoke_grant_returns_unvested_to_treasury() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &true);
    
    // At ledger 200, halfway through vesting period (150 to 250)
    at(&c, 200);
    let admin_bal_before = bal(&c, &c.admin);
    let (paid, returned) = c.client.revoke_grant(&b);
    
    // Should have vested 500 tokens (50% of 150->250 range)
    assert_eq!(paid, 500);
    assert_eq!(returned, 500);
    assert_eq!(bal(&c, &b), 500);
    assert_eq!(bal(&c, &c.admin), admin_bal_before + 500);
}

#[test]
fn revoke_grant_calculates_vested_correctly() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &true);
    
    // Beneficiary claims 200 tokens first
    at(&c, 170);
    let claimed = c.client.claim(&b);
    assert_eq!(claimed, 200); // 20% vested
    
    // Admin revokes at ledger 200
    at(&c, 200);
    let admin_bal_before = bal(&c, &c.admin);
    let (paid, returned) = c.client.revoke_grant(&b);
    
    // Total vested = 500, already claimed = 200, so paid = 300
    assert_eq!(paid, 300);
    assert_eq!(returned, 500); // 1000 - 500 vested
    assert_eq!(bal(&c, &b), 200 + 300);
    assert_eq!(bal(&c, &c.admin), admin_bal_before + 500);
}

#[test]
fn revoke_grant_irrevocable_fails() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &false); // irrevocable
    
    at(&c, 200);
    assert_eq!(
        c.client.try_revoke_grant(&b),
        Err(Ok(VestingError::ScheduleIrrevocable))
    );
}

// ── #1456 validate_bounds ────────────────────────────────────────────────────

#[test]
fn cliff_after_end_rejected() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    at(&c, 100);
    
    // cliff_ledger > end_ledger should fail
    assert_eq!(
        c.client.try_create_schedule(&b, &100, &300, &250, &1000, &true),
        Err(Ok(VestingError::InvalidScheduleBounds))
    );
}

#[test]
fn start_after_cliff_rejected() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    at(&c, 100);
    
    // start_ledger > cliff_ledger should fail
    assert_eq!(
        c.client.try_create_schedule(&b, &200, &150, &250, &1000, &true),
        Err(Ok(VestingError::InvalidScheduleTimeline))
    );
}

// ── #1457 batch vesting ──────────────────────────────────────────────────────

#[test]
fn create_schedules_batch_single_transfer() {
    let c = setup_ctx();
    let b1 = Address::generate(&c.env);
    let b2 = Address::generate(&c.env);
    let b3 = Address::generate(&c.env);
    
    let schedules = vec![
        &c.env,
        ScheduleParams {
            beneficiary: b1.clone(),
            start_ledger: 110,
            cliff_ledger: 110,
            end_ledger: 210,
            amount: 1000,
            is_revocable: true,
        },
        ScheduleParams {
            beneficiary: b2.clone(),
            start_ledger: 110,
            cliff_ledger: 110,
            end_ledger: 210,
            amount: 2000,
            is_revocable: true,
        },
        ScheduleParams {
            beneficiary: b3.clone(),
            start_ledger: 110,
            cliff_ledger: 110,
            end_ledger: 210,
            amount: 3000,
            is_revocable: false,
        },
    ];
    
    let admin_bal_before = bal(&c, &c.admin);
    c.client.create_schedules_batch(&c.admin, &schedules);
    
    // Total of 6000 tokens should have been transferred once
    assert_eq!(bal(&c, &c.admin), admin_bal_before - 6000);
    
    // Verify all schedules created
    assert_eq!(c.client.get_info(&b1).unwrap().amount, 1000);
    assert_eq!(c.client.get_info(&b2).unwrap().amount, 2000);
    assert_eq!(c.client.get_info(&b3).unwrap().amount, 3000);
    assert_eq!(c.client.get_info(&b3).unwrap().is_revocable, false);
}

#[test]
fn batch_creation_matches_individual() {
    let c = setup_ctx();
    let b1 = Address::generate(&c.env);
    let b2 = Address::generate(&c.env);
    
    // Create first schedule individually
    c.client.create_schedule(&b1, &110, &110, &210, &1000, &true);
    
    // Create second via batch
    let schedules = vec![
        &c.env,
        ScheduleParams {
            beneficiary: b2.clone(),
            start_ledger: 110,
            cliff_ledger: 110,
            end_ledger: 210,
            amount: 1000,
            is_revocable: true,
        },
    ];
    c.client.create_schedules_batch(&c.admin, &schedules);
    
    // Both should have identical schedules
    at(&c, 160);
    assert_eq!(c.client.claimable(&b1), c.client.claimable(&b2));
}

// ── #1458 change_beneficiary ─────────────────────────────────────────────────

#[test]
fn change_beneficiary_moves_schedule_and_claims_continue() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    let new_b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &true);

    // Partial claim under the old key.
    at(&c, 170);
    assert_eq!(c.client.claim(&b), 200);

    // The current beneficiary must authorize the change.
    c.client.change_beneficiary(&b, &new_b);
    let auths = c.env.auths();
    assert_eq!(auths.last().unwrap().0, b);

    // Schedule moved to the new address.
    assert_eq!(c.client.get_info(&b), None);
    let info = c.client.get_info(&new_b).unwrap();
    assert_eq!((info.amount, info.claimed), (1000, 200));

    // Old key can no longer claim; new key receives the remainder.
    assert_eq!(
        c.client.try_claim(&b),
        Err(Ok(VestingError::ScheduleNotFound))
    );
    at(&c, 250);
    assert_eq!(c.client.claim(&new_b), 800);
    assert_eq!(bal(&c, &new_b), 800);
}

// ── #1459 zero-duration schedules ────────────────────────────────────────────

#[test]
fn zero_duration_schedule_rejected() {
    let c = setup_ctx();
    let b = Address::generate(&c.env);
    at(&c, 100);
    assert_eq!(
        c.client.try_create_schedule(&b, &200, &200, &200, &1000, &true),
        Err(Ok(VestingError::InvalidScheduleDuration))
    );
    assert!(c.client.get_info(&b).is_none());
}

