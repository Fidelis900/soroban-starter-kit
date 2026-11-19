#![allow(clippy::unwrap_used, clippy::expect_used, clippy::arithmetic_side_effects)]
#![cfg(test)]

use soroban_sdk::{
    Address, Env, Vec,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec,
};

use crate::{ScheduleParams, VestingContract, VestingContractClient, VestingError};

struct Ctx<'a> {
    env: Env,
    client: VestingContractClient<'a>,
    admin: Address,
    token: Address,
}

fn setup<'a>() -> Ctx<'a> {
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
    Ctx { env, client, admin, token }
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
    let c = setup();
    let b = Address::generate(&c.env);
    // start == now, start == cliff allowed
    c.client.create_schedule(&b, &100, &100, &200, &1000, &true);
    assert_eq!(c.client.get_info(&b).unwrap().amount, 1000);
}

#[test]
fn bounds_invalid_rejected() {
    let c = setup();
    let b = Address::generate(&c.env);
    let bad = [
        (99, 150, 200),  // start in the past
        (120, 110, 200), // cliff < start
        (100, 150, 150), // end == cliff
        (100, 150, 140), // end < cliff
    ];
    for (s, cl, e) in bad {
        let r = c.client.try_create_schedule(&b, &s, &cl, &e, &1000, &true);
        assert_eq!(r, Err(Ok(VestingError::InvalidScheduleBounds)), "{s} {cl} {e}");
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
    let c = setup();
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
    let c = setup();
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
    let c = setup();
    let p = vec![&c.env, params(&c, 10)];
    let other = Address::generate(&c.env);
    assert_eq!(
        c.client.try_create_schedules_batch(&other, &p),
        Err(Ok(VestingError::NotAuthorized))
    );
    assert_eq!(
        c.client.try_create_schedules_batch(&c.admin, &Vec::new(&c.env)),
        Err(Ok(VestingError::InvalidSchedule))
    );
}

// ── #1341 milestones ─────────────────────────────────────────────────────────

#[test]
fn milestone_unlocks_on_approval() {
    let c = setup();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 3000u32), (2, 3000), (3, 4000)];
    c.client.create_milestone_schedule(&b, &v, &ms, &1000, &true);

    assert_eq!(c.client.try_claim(&b), Err(Ok(VestingError::NothingToClaim)));
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
    let c = setup();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 5000u32), (2, 5000)];
    c.client.create_milestone_schedule(&b, &v, &ms, &1000, &true);
    assert_eq!(c.client.try_approve_milestone(&b, &9), Err(Ok(VestingError::MilestoneNotFound)));
    c.client.approve_milestone(&b, &1);
    assert_eq!(
        c.client.try_approve_milestone(&b, &1),
        Err(Ok(VestingError::MilestoneAlreadyApproved))
    );
    // weights must sum to 10_000
    let b2 = Address::generate(&c.env);
    let bad = vec![&c.env, (1u32, 5000u32)];
    assert_eq!(
        c.client.try_create_milestone_schedule(&b2, &v, &bad, &1000, &true),
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
    let c = setup();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 10_000u32)];
    c.client.create_milestone_schedule(&b, &v, &ms, &500, &false);
    c.client.approve_milestone(&b, &1);
    let auths = c.env.auths();
    assert_eq!(auths.last().unwrap().0, v);
}

// ── #1343 revocation math ────────────────────────────────────────────────────

/// Revoke at `seq` for a 1000-token schedule (start 100, cliff 150, end 250)
/// and assert exact conservation.
fn check_revoke(seq: u32, claim_first_at: Option<u32>) -> (i128, i128) {
    let c = setup();
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
    let c = setup();
    let b = Address::generate(&c.env);
    c.client.create_schedule(&b, &100, &150, &250, &1000, &false);
    assert_eq!(c.client.try_revoke(&b), Err(Ok(VestingError::ScheduleIrrevocable)));
    let b2 = Address::generate(&c.env);
    c.client.create_schedule(&b2, &100, &150, &250, &1000, &true);
    c.client.revoke(&b2);
    assert_eq!(c.client.try_revoke(&b2), Err(Ok(VestingError::AlreadyRevoked)));
    assert_eq!(c.client.try_claim(&b2), Err(Ok(VestingError::NothingToClaim)));
}

#[test]
fn milestone_revoke_conserves() {
    let c = setup();
    let b = Address::generate(&c.env);
    let v = Address::generate(&c.env);
    let ms = vec![&c.env, (1u32, 3333u32), (2, 6667)];
    c.client.create_milestone_schedule(&b, &v, &ms, &1001, &true);
    c.client.approve_milestone(&b, &1);
    let (pay, claw) = c.client.revoke(&b);
    assert_eq!(pay + claw, 1001);
    assert_eq!(pay, 333); // 1001*3333/10000 rounded down
}
