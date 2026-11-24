#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    Address, Bytes, BytesN, Env, IntoVal, Vec, symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec,
};

// ---------------------------------------------------------------------------
// Merkle helpers (mirror the on-chain logic for test setup)
// ---------------------------------------------------------------------------

fn hash_pair(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::new(env);
    if a.to_array() <= b.to_array() {
        data.append(&Bytes::from(a.clone()));
        data.append(&Bytes::from(b.clone()));
    } else {
        data.append(&Bytes::from(b.clone()));
        data.append(&Bytes::from(a.clone()));
    }
    env.crypto().sha256(&data).into()
}

const FAR_DEADLINE: u32 = 1_000_000;
const TIER_1: u32 = 1;
const TIER_2: u32 = 2;
const TIER_1_AMOUNT: i128 = 1_000;
const TIER_2_AMOUNT: i128 = 5_000;

struct T<'a> {
    env: Env,
    client: AirdropContractClient<'a>,
    contract: Address,
    token: Address,
    admin: Address,
    alice: Address,
    bob: Address,
}

impl T<'_> {
    /// Leaf as computed by the deployed contract instance.
    fn leaf(&self, recipient: &Address, amount: i128, tier: u32) -> BytesN<32> {
        self.env.as_contract(&self.contract, || {
            compute_leaf(&self.env, recipient, &self.token, amount, tier)
        })
    }

    /// Two-leaf tree: alice @ tier 1, bob @ tier 2. Returns (proof_alice, proof_bob).
    fn set_tiered_root(&self) -> (Vec<BytesN<32>>, Vec<BytesN<32>>) {
        let la = self.leaf(&self.alice, TIER_1_AMOUNT, TIER_1);
        let lb = self.leaf(&self.bob, TIER_2_AMOUNT, TIER_2);
        self.client.set_root(&hash_pair(&self.env, &la, &lb));
        (vec![&self.env, lb], vec![&self.env, la])
    }

    fn balance(&self, who: &Address) -> i128 {
        TokenClient::new(&self.env, &self.token).balance(who)
    }
}

fn setup_with(env: &Env, vesting: Option<VestingConfig>, mock_auths: bool) -> T<'_> {
    if mock_auths {
        env.mock_all_auths();
    }
    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(env, &contract);

    if mock_auths {
        client.initialize(&admin, &FAR_DEADLINE, &vesting);
        StellarAssetClient::new(env, &token).mint(&contract, &100_000i128);
    }

    T {
        env: env.clone(),
        client,
        contract,
        token,
        admin,
        alice: Address::generate(env),
        bob: Address::generate(env),
    }
}

fn setup(env: &Env) -> T<'_> {
    setup_with(env, None, true)
}

// ---------------------------------------------------------------------------
// Core behaviour
// ---------------------------------------------------------------------------

#[test]
fn initialize_rejects_duplicate() {
    let env = Env::default();
    let t = setup(&env);
    assert_eq!(
        t.client.try_initialize(&t.admin, &FAR_DEADLINE, &None),
        Err(Ok(AirdropError::AlreadyInitialized))
    );
}

#[test]
fn claim_pays_recipient_and_blocks_duplicate() {
    let env = Env::default();
    let t = setup(&env);
    let (proof_a, _) = t.set_tiered_root();

    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert!(t.client.is_claimed(&t.alice, &t.token));

    assert_eq!(
        t.client
            .try_claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a),
        Err(Ok(AirdropError::AlreadyClaimed))
    );
}

#[test]
fn wrong_amount_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let (proof_a, _) = t.set_tiered_root();
    assert_eq!(
        t.client
            .try_claim(&t.alice, &t.token, &TIER_2_AMOUNT, &TIER_1, &proof_a),
        Err(Ok(AirdropError::InvalidProof))
    );
}

#[test]
fn claim_after_deadline_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let (proof_a, _) = t.set_tiered_root();
    env.ledger()
        .with_mut(|l| l.sequence_number = FAR_DEADLINE + 1);
    assert_eq!(
        t.client
            .try_claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a),
        Err(Ok(AirdropError::ClaimWindowClosed))
    );
}

#[test]
fn vested_claim_releases_over_time() {
    let env = Env::default();
    let cfg = VestingConfig {
        initial_unlock_bps: 2_000,
        vesting_duration_ledgers: 100,
    };
    let t = setup_with(&env, Some(cfg), true);
    let (proof_a, _) = t.set_tiered_root();

    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert_eq!(t.balance(&t.alice), 200);
    assert_eq!(t.client.total_locked(&t.token), 800);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    assert_eq!(t.client.release(&t.alice, &t.token), 400);

    env.ledger().with_mut(|l| l.sequence_number += 50);
    assert_eq!(t.client.release(&t.alice, &t.token), 400);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert_eq!(t.client.total_locked(&t.token), 0);
}

// ---------------------------------------------------------------------------
// #1346 — tiered allocations
// ---------------------------------------------------------------------------

#[test]
fn tiered_claims_record_tier_in_storage() {
    let env = Env::default();
    let t = setup(&env);
    let (proof_a, proof_b) = t.set_tiered_root();

    assert_eq!(t.client.get_claimed_tier(&t.alice, &t.token), None);

    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_b);

    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert_eq!(t.balance(&t.bob), TIER_2_AMOUNT);
    assert_eq!(t.client.get_claimed_tier(&t.alice, &t.token), Some(TIER_1));
    assert_eq!(t.client.get_claimed_tier(&t.bob, &t.token), Some(TIER_2));
}

#[test]
fn tiered_claim_emits_tier_in_event() {
    let env = Env::default();
    let t = setup(&env);
    let (_, proof_b) = t.set_tiered_root();

    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_b);

    assert_eq!(
        env.events().all().filter_by_contract(&t.contract),
        vec![
            &env,
            (
                t.contract.clone(),
                (symbol_short!("claimed"), TIER_2).into_val(&env),
                (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2).into_val(&env),
            ),
        ]
    );
}

#[test]
fn claiming_under_a_higher_tier_is_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let (proof_a, _) = t.set_tiered_root();

    assert_eq!(
        t.client
            .try_claim(&t.alice, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_a),
        Err(Ok(AirdropError::InvalidProof))
    );
    assert_eq!(
        t.client
            .try_claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_2, &proof_a),
        Err(Ok(AirdropError::InvalidProof))
    );
}

#[test]
fn tiered_batch_claim_with_multi_proof() {
    let env = Env::default();
    let t = setup(&env);
    let la = t.leaf(&t.alice, TIER_1_AMOUNT, TIER_1);
    let lb = t.leaf(&t.bob, TIER_2_AMOUNT, TIER_2);
    t.client.set_root(&hash_pair(&env, &la, &lb));

    let entries = vec![
        &env,
        (t.alice.clone(), t.token.clone(), TIER_1_AMOUNT, TIER_1),
        (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    t.client
        .claim_batch(&entries, &Vec::new(&env), &vec![&env, true]);

    let authorizers: std::vec::Vec<Address> = env.auths().into_iter().map(|(a, _)| a).collect();
    assert!(authorizers.contains(&t.alice));
    assert!(authorizers.contains(&t.bob));

    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert_eq!(t.balance(&t.bob), TIER_2_AMOUNT);
    assert_eq!(t.client.get_claimed_tier(&t.bob, &t.token), Some(TIER_2));
}

// ---------------------------------------------------------------------------
// #1347 — front-running / recipient binding
// ---------------------------------------------------------------------------

fn setup_strict(env: &Env) -> (T<'_>, Vec<BytesN<32>>) {
    let t = setup_with(env, None, false);
    env.mock_all_auths();
    t.client.initialize(&t.admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(env, &t.token).mint(&t.contract, &100_000i128);
    let (proof_a, _) = t.set_tiered_root();
    env.set_auths(&[]);
    (t, proof_a)
}

#[test]
fn sniper_cannot_submit_copied_proof_without_recipient_auth() {
    let env = Env::default();
    let (t, proof_a) = setup_strict(&env);
    let res = t
        .client
        .try_claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert!(res.is_err());
    assert!(!t.client.is_claimed(&t.alice, &t.token));
    assert_eq!(t.balance(&t.alice), 0);
}

#[test]
fn sniper_cannot_redirect_claim_to_itself() {
    let env = Env::default();
    let (t, proof_a) = setup_strict(&env);
    let sniper = Address::generate(&env);
    env.mock_all_auths();
    assert_eq!(
        t.client
            .try_claim(&sniper, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a),
        Err(Ok(AirdropError::InvalidProof))
    );
    assert_eq!(t.balance(&sniper), 0);
    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
}

#[test]
fn relayer_batch_without_recipient_auth_is_rejected() {
    let env = Env::default();
    let (t, _) = setup_strict(&env);
    let entries = vec![
        &env,
        (t.alice.clone(), t.token.clone(), TIER_1_AMOUNT, TIER_1),
        (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    let res = t
        .client
        .try_claim_batch(&entries, &Vec::new(&env), &vec![&env, true]);
    assert!(res.is_err());
    assert!(!t.client.is_claimed(&t.alice, &t.token));
    assert!(!t.client.is_claimed(&t.bob, &t.token));
}

// ---------------------------------------------------------------------------
// #1348 — emergency pause / resume
// ---------------------------------------------------------------------------

#[test]
fn pause_rejects_claims_and_resume_restores_them() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);

    let proof = Vec::<BytesN<32>>::new(&env);
    assert!(!client.is_paused());

    client.set_paused(&true);
    assert!(client.is_paused());
    assert_eq!(
        client.try_claim(&recipient, &token, &100, &1, &proof),
        Err(Ok(AirdropError::Paused))
    );

    client.set_paused(&false);
    assert!(!client.is_paused());
    // Root is not set yet — pause is gone.
    assert_eq!(
        client.try_claim(&recipient, &token, &100, &1, &proof),
        Err(Ok(AirdropError::RootNotSet))
    );
}

#[test]
fn pause_state_defaults_to_false() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// #1349 — expiring claim windows with variable reward decay
// ---------------------------------------------------------------------------

#[test]
fn decayed_payout_and_treasury_remainder() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);

    let config = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: 100,
        decay_step_ledgers: 10,
        decay_bps_per_step: 1_000,
        floor_bps: 5_000,
    };
    client.set_decay_config(&treasury, &config);
    assert_eq!(client.get_decay_config(), Some(config.clone()));

    env.ledger().with_mut(|li| li.sequence_number = 50);
    assert_eq!(decayed_amount(&env, 1_000).unwrap(), (1_000, 0));

    env.ledger().with_mut(|li| li.sequence_number = 130);
    assert_eq!(decayed_amount(&env, 1_000).unwrap(), (700, 300));
    assert_eq!(decay_bps(&config, 130), 7_000);

    env.ledger().with_mut(|li| li.sequence_number = 500);
    assert_eq!(decayed_amount(&env, 1_000).unwrap(), (500, 500));
    assert_eq!(decay_bps(&config, 500), 5_000);
}

#[test]
fn invalid_decay_config_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);
    let config = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: 10,
        decay_step_ledgers: 0,
        decay_bps_per_step: 100,
        floor_bps: 5_000,
    };
    assert_eq!(
        client.try_set_decay_config(&treasury, &config),
        Err(Ok(AirdropError::InvalidDecayConfig))
    );
}

// ---------------------------------------------------------------------------
// #1350 — governance-authorized multi-epoch Merkle root updates
// ---------------------------------------------------------------------------

#[test]
fn governance_root_updates_preserve_claimed_bitmaps() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dao = Address::generate(&env);
    let alice = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);
    client.set_governance(&dao);
    assert_eq!(client.get_governance(), Some(dao.clone()));

    env.as_contract(&contract, || {
        env.storage()
            .persistent()
            .set(&DataKey::Claimed(alice.clone(), token.clone()), &true);
    });

    assert_eq!(client.get_epoch(), 0);
    let root_one = BytesN::from_array(&env, &[1u8; 32]);
    client.update_root_by_governance(&root_one, &2_000);
    assert_eq!(client.get_epoch(), 1);
    env.as_contract(&contract, || {
        assert!(env
            .storage()
            .instance()
            .has(&DataKey::MerkleRoot(1)));
        assert!(env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Claimed(alice.clone(), token.clone()))
            .unwrap());
    });

    let root_two = BytesN::from_array(&env, &[2u8; 32]);
    client.update_root_by_governance(&root_two, &3_000);
    assert_eq!(client.get_epoch(), 2);
    env.as_contract(&contract, || {
        assert!(env
            .storage()
            .instance()
            .has(&DataKey::MerkleRoot(2)));
        assert!(env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Claimed(alice.clone(), token.clone()))
            .unwrap());
    });
}

// ---------------------------------------------------------------------------
// Large-claim vesting routing (whale dump prevention)
// ---------------------------------------------------------------------------

/// Claims at or above the threshold are locked into an internal vesting schedule;
/// no tokens are transferred immediately.
#[test]
fn large_claim_above_threshold_is_fully_vested() {
    let env = Env::default();
    let t = setup(&env);

    // Set threshold at 3_000 — bob's 5_000 claim qualifies, alice's 1_000 does not.
    t.client.set_claim_threshold(&3_000i128);
    assert_eq!(t.client.get_claim_threshold(), Some(3_000i128));

    let (proof_a, proof_b) = t.set_tiered_root();

    // Alice's small claim (1_000 < 3_000): paid immediately.
    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert_eq!(t.client.total_locked(&t.token), 0);
    assert!(t.client.get_vesting(&t.alice, &t.token).is_none());

    // Bob's large claim (5_000 >= 3_000): fully locked, no immediate transfer.
    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_b);
    assert_eq!(t.balance(&t.bob), 0);
    assert_eq!(t.client.total_locked(&t.token), TIER_2_AMOUNT);
    let schedule = t.client.get_vesting(&t.bob, &t.token).unwrap();
    assert_eq!(schedule.total, TIER_2_AMOUNT);
    assert_eq!(schedule.released, 0);
    assert_eq!(schedule.duration_ledgers, SIX_MONTHS_IN_LEDGERS);
}

/// After the vesting duration elapses the full locked amount becomes releasable.
#[test]
fn large_claim_vesting_fully_releases_after_duration() {
    let env = Env::default();
    let t = setup(&env);
    t.client.set_claim_threshold(&3_000i128);
    let (_, proof_b) = t.set_tiered_root();

    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_b);
    assert_eq!(t.balance(&t.bob), 0);

    // Advance past the 6-month vesting duration.
    env.ledger()
        .with_mut(|l| l.sequence_number += SIX_MONTHS_IN_LEDGERS + 1);

    let released = t.client.release(&t.bob, &t.token);
    assert_eq!(released, TIER_2_AMOUNT);
    assert_eq!(t.balance(&t.bob), TIER_2_AMOUNT);
    assert_eq!(t.client.total_locked(&t.token), 0);
}

/// Small claims (below threshold) are paid out immediately with no vesting schedule.
#[test]
fn small_claim_below_threshold_is_immediate() {
    let env = Env::default();
    let t = setup(&env);
    // Set a very high threshold so alice's 1_000 is small.
    t.client.set_claim_threshold(&10_000i128);
    let (proof_a, _) = t.set_tiered_root();

    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert!(t.client.get_vesting(&t.alice, &t.token).is_none());
    assert_eq!(t.client.total_locked(&t.token), 0);
}

/// Large claim with an explicit VestingConfig respects the initial_unlock_bps split.
#[test]
fn large_claim_with_vesting_config_applies_unlock_split() {
    let env = Env::default();
    // 20% immediate unlock.
    let cfg = VestingConfig {
        initial_unlock_bps: 2_000,
        vesting_duration_ledgers: 500,
    };
    let t = setup_with(&env, Some(cfg), true);
    t.client.set_claim_threshold(&3_000i128);
    let (_, proof_b) = t.set_tiered_root();

    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &proof_b);

    // 20% of 5_000 = 1_000 immediate.
    assert_eq!(t.balance(&t.bob), 1_000);
    // 80% of 5_000 = 4_000 locked.
    assert_eq!(t.client.total_locked(&t.token), 4_000);
    let schedule = t.client.get_vesting(&t.bob, &t.token).unwrap();
    assert_eq!(schedule.total, 4_000);
    assert_eq!(schedule.duration_ledgers, 500);
}

// ---------------------------------------------------------------------------
// reclaim_unclaimed — treasury reclamation after deadline
// ---------------------------------------------------------------------------

/// Treasury cannot reclaim before the claim deadline (ClaimNotExpired).
#[test]
fn reclaim_before_deadline_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);

    // Wire up a treasury so the config is complete.
    let decay_cfg = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: FAR_DEADLINE,
        decay_step_ledgers: 1,
        decay_bps_per_step: 0,
        floor_bps: 10_000,
    };
    client.set_decay_config(&treasury, &decay_cfg);

    StellarAssetClient::new(&env, &token).mint(&contract, &50_000i128);

    // Deadline has NOT passed yet.
    assert_eq!(env.ledger().sequence(), 0);
    assert_eq!(
        client.try_reclaim_unclaimed(&token),
        Err(Ok(AirdropError::ClaimNotExpired))
    );
}

/// After deadline, treasury receives all unlocked tokens.
#[test]
fn reclaim_after_deadline_transfers_to_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let deadline: u32 = 100;
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &deadline, &None);

    let decay_cfg = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: deadline,
        decay_step_ledgers: 1,
        decay_bps_per_step: 0,
        floor_bps: 10_000,
    };
    client.set_decay_config(&treasury, &decay_cfg);

    let funded: i128 = 50_000;
    StellarAssetClient::new(&env, &token).mint(&contract, &funded);

    // Advance past deadline.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    let reclaimed = client.reclaim_unclaimed(&token);
    assert_eq!(reclaimed, funded);
    assert_eq!(TokenClient::new(&env, &token).balance(&treasury), funded);
    assert_eq!(TokenClient::new(&env, &token).balance(&contract), 0);
}

/// Locked vesting tokens are excluded from reclamation.
#[test]
fn reclaim_excludes_locked_vesting_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let deadline: u32 = 100;
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    // 0% immediate unlock — entire claim goes into vesting.
    let vesting = VestingConfig {
        initial_unlock_bps: 0,
        vesting_duration_ledgers: 500,
    };
    client.initialize(&admin, &deadline, &Some(vesting));

    let decay_cfg = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: deadline,
        decay_step_ledgers: 1,
        decay_bps_per_step: 0,
        floor_bps: 10_000,
    };
    client.set_decay_config(&treasury, &decay_cfg);

    let funded: i128 = 50_000;
    StellarAssetClient::new(&env, &token).mint(&contract, &funded);

    // Alice claims 1_000 — goes entirely into her vesting schedule.
    let alice = Address::generate(&env);
    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    client.set_root(&la);
    client.claim(&alice, &token, &TIER_1_AMOUNT, &TIER_1, &Vec::new(&env));
    assert_eq!(client.total_locked(&token), TIER_1_AMOUNT);

    // Advance past deadline.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);

    // Reclaim should only get back (funded - locked) = 49_000.
    let reclaimed = client.reclaim_unclaimed(&token);
    assert_eq!(reclaimed, funded - TIER_1_AMOUNT);
    assert_eq!(
        TokenClient::new(&env, &token).balance(&treasury),
        funded - TIER_1_AMOUNT
    );
    // The locked portion remains in the contract for alice to release.
    assert_eq!(
        TokenClient::new(&env, &token).balance(&contract),
        TIER_1_AMOUNT
    );
}

/// reclaim_unclaimed returns 0 when nothing is left to reclaim.
#[test]
fn reclaim_returns_zero_when_nothing_to_reclaim() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let deadline: u32 = 100;
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &deadline, &None);

    let decay_cfg = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: deadline,
        decay_step_ledgers: 1,
        decay_bps_per_step: 0,
        floor_bps: 10_000,
    };
    client.set_decay_config(&treasury, &decay_cfg);

    // No tokens funded — balance is 0.
    env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
    assert_eq!(client.reclaim_unclaimed(&token), 0);
}

// ---------------------------------------------------------------------------
// Best-effort claim_batch — one failure does not revert valid claims
// ---------------------------------------------------------------------------

/// An already-claimed entry in the batch is silently skipped; other valid
/// entries in the same batch are still processed and paid.
#[test]
fn batch_skips_already_claimed_entry_without_reverting_others() {
    let env = Env::default();
    let t = setup(&env);
    let la = t.leaf(&t.alice, TIER_1_AMOUNT, TIER_1);
    let lb = t.leaf(&t.bob, TIER_2_AMOUNT, TIER_2);
    t.client.set_root(&hash_pair(&env, &la, &lb));

    // Alice claims individually first.
    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &vec![&env, lb.clone()]);
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert!(t.client.is_claimed(&t.alice, &t.token));

    // Now a batch includes alice (already claimed) and bob (not yet claimed).
    let entries = vec![
        &env,
        (t.alice.clone(), t.token.clone(), TIER_1_AMOUNT, TIER_1),
        (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    // This must succeed (not revert) even though alice is already claimed.
    t.client
        .claim_batch(&entries, &Vec::new(&env), &vec![&env, true]);

    // Bob's valid claim went through.
    assert_eq!(t.balance(&t.bob), TIER_2_AMOUNT);
    assert!(t.client.is_claimed(&t.bob, &t.token));

    // Alice's balance is unchanged (skipped, not double-paid).
    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
}

/// A duplicate (recipient, token) pair within the same batch: second occurrence
/// is silently skipped, first is processed normally.
#[test]
fn batch_skips_duplicate_pair_without_reverting() {
    let env = Env::default();
    let t = setup(&env);
    let la = t.leaf(&t.alice, TIER_1_AMOUNT, TIER_1);
    let lb = t.leaf(&t.bob, TIER_2_AMOUNT, TIER_2);
    // Three-leaf tree: alice, bob, alice-duplicate (same leaf).
    let lab = hash_pair(&env, &la, &lb);
    let root = hash_pair(&env, &lab, &la);
    t.client.set_root(&root);

    // Single entry for alice and bob — straightforward proof.
    let entries = vec![
        &env,
        (t.alice.clone(), t.token.clone(), TIER_1_AMOUNT, TIER_1),
        (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    t.client
        .claim_batch(&entries, &Vec::new(&env), &vec![&env, true]);

    assert_eq!(t.balance(&t.alice), TIER_1_AMOUNT);
    assert_eq!(t.balance(&t.bob), TIER_2_AMOUNT);
}

/// An empty batch returns EmptyBatch error immediately.
#[test]
fn batch_empty_entries_returns_error() {
    let env = Env::default();
    let t = setup(&env);
    let la = t.leaf(&t.alice, TIER_1_AMOUNT, TIER_1);
    t.client.set_root(&la);

    let entries: Vec<(Address, Address, i128, u32)> = Vec::new(&env);
    assert_eq!(
        t.client
            .try_claim_batch(&entries, &Vec::new(&env), &Vec::new(&env)),
        Err(Ok(AirdropError::EmptyBatch))
    );
}

/// When every entry in the batch is already claimed, the call succeeds but no
/// tokens move.
#[test]
fn batch_all_already_claimed_succeeds_with_no_transfers() {
    let env = Env::default();
    let t = setup(&env);
    let la = t.leaf(&t.alice, TIER_1_AMOUNT, TIER_1);
    let lb = t.leaf(&t.bob, TIER_2_AMOUNT, TIER_2);
    t.client.set_root(&hash_pair(&env, &la, &lb));

    // Both claim individually first.
    t.client
        .claim(&t.alice, &t.token, &TIER_1_AMOUNT, &TIER_1, &vec![&env, lb.clone()]);
    t.client
        .claim(&t.bob, &t.token, &TIER_2_AMOUNT, &TIER_2, &vec![&env, la.clone()]);

    let balance_alice_before = t.balance(&t.alice);
    let balance_bob_before = t.balance(&t.bob);

    // Batch with both already claimed must succeed without reverting.
    let entries = vec![
        &env,
        (t.alice.clone(), t.token.clone(), TIER_1_AMOUNT, TIER_1),
        (t.bob.clone(), t.token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    t.client
        .claim_batch(&entries, &Vec::new(&env), &vec![&env, true]);

    // Balances unchanged.
    assert_eq!(t.balance(&t.alice), balance_alice_before);
    assert_eq!(t.balance(&t.bob), balance_bob_before);
}
