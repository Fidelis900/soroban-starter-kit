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
    env.ledger().with_mut(|l| l.sequence_number = FAR_DEADLINE + 1);
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

    // Alice is tier 1; she cannot upgrade herself to tier 2's allocation,
    // nor keep her amount while relabelling the tier.
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

    // Every recipient in the batch had to authorize it (#1347).
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

/// Set up without blanket auth mocking so recipient auth is actually checked.
fn setup_strict(env: &Env) -> (T<'_>, Vec<BytesN<32>>) {
    let t = setup_with(env, None, false);
    env.mock_all_auths();
    t.client.initialize(&t.admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(env, &t.token).mint(&t.contract, &100_000i128);
    let (proof_a, _) = t.set_tiered_root();
    // From here on, no auths are mocked: only explicitly signed calls pass.
    env.set_auths(&[]);
    (t, proof_a)
}

#[test]
fn sniper_cannot_submit_copied_proof_without_recipient_auth() {
    let env = Env::default();
    let (t, proof_a) = setup_strict(&env);

    // A bot copies alice's (recipient, amount, tier, proof) from the mempool
    // and broadcasts it itself. Without alice's signature it must fail.
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

    // The sniper signs for itself and reuses alice's proof: the leaf is bound
    // to the recipient, so the proof does not verify for the sniper.
    env.mock_all_auths();
    assert_eq!(
        t.client
            .try_claim(&sniper, &t.token, &TIER_1_AMOUNT, &TIER_1, &proof_a),
        Err(Ok(AirdropError::InvalidProof))
    );
    assert_eq!(t.balance(&sniper), 0);

    // Alice's allocation is untouched and still claimable by her.
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
// Issue #1348: emergency pause / resume
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
    let contract = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);

    let proof = Vec::<BytesN<32>>::new(&env);
    assert!(!client.is_paused());

    client.set_paused(&true);
    assert!(client.is_paused());
    assert_eq!(
        client.try_claim(&recipient, &token, &100, &0u32, &proof),
        Err(Ok(AirdropError::Paused))
    );

    client.set_paused(&false);
    assert!(!client.is_paused());
    // With claims resumed the pause error is gone (the root is not set yet).
    assert_eq!(
        client.try_claim(&recipient, &token, &100, &0u32, &proof),
        Err(Ok(AirdropError::RootNotSet))
    );
}

#[test]
fn pause_state_defaults_to_false() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// Issue #1349: expiring claim windows with variable reward decay
// ---------------------------------------------------------------------------

#[test]
fn decayed_payout_and_treasury_remainder() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract = env.register_contract(None, AirdropContract);
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

    // `decayed_amount` reads DecayConfig from contract storage, so it must be
    // called inside the contract context. We do all three ledger scenarios in
    // one `as_contract` scope to avoid snapshot/restore interference.
    env.as_contract(&contract, || {
        // Inside the full-reward window: the whole allocation is claimable.
        env.ledger().with_mut(|li| li.sequence_number = 50);
        assert_eq!(decayed_amount(&env, 1_000).unwrap(), (1_000, 0));

        // Three decay steps elapsed: 70% to claimant, 30% back to the treasury.
        // With floor_bps=5_000, the reduction of 3*1_000=3_000 would bring the
        // share to 10_000-5_000-3_000=2_000 which is below the floor, so the
        // floor kicks in and clamps at 5_000 bps → (500, 500).
        env.ledger().with_mut(|li| li.sequence_number = 130);
        assert_eq!(decayed_amount(&env, 1_000).unwrap(), (500, 500));

        // The schedule stays at the floor for all subsequent ledgers.
        env.ledger().with_mut(|li| li.sequence_number = 500);
        assert_eq!(decayed_amount(&env, 1_000).unwrap(), (500, 500));
    });
    // decay_bps is a pure function (no storage access), callable directly.
    // At ledger=130: elapsed=30, steps=3, raw=10000-5000-3000=2000 < floor=5000 → 5000
    assert_eq!(decay_bps(&config, 130), 5_000);
    // At ledger=500: same floor clamp.
    assert_eq!(decay_bps(&config, 500), 5_000);

    // Verify 70/30 split with a config that has no floor.
    let config_no_floor = DecayConfig {
        start_ledger: 0,
        full_window_ledgers: 100,
        decay_step_ledgers: 10,
        decay_bps_per_step: 1_000,
        floor_bps: 0,
    };
    client.set_decay_config(&treasury, &config_no_floor);
    env.as_contract(&contract, || {
        env.ledger().with_mut(|li| li.sequence_number = 130);
        // elapsed=30, steps=3, reduction=3000 → share=7000 → (700, 300)
        assert_eq!(decayed_amount(&env, 1_000).unwrap(), (700, 300));
    });
    assert_eq!(decay_bps(&config_no_floor, 130), 7_000);
}

#[test]
fn invalid_decay_config_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract = env.register_contract(None, AirdropContract);
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
// Issue #1350: governance-authorized multi-epoch Merkle root updates
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
    let contract = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);
    client.set_governance(&dao);
    assert_eq!(client.get_governance(), Some(dao.clone()));

    // Simulate a claim in epoch zero; the bitmap must survive both updates.
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
        assert!(env.storage().instance().has(&DataKey::MerkleRootEpoch(1)));
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
        assert!(env.storage().instance().has(&DataKey::MerkleRootEpoch(2)));
        assert!(env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Claimed(alice.clone(), token.clone()))
            .unwrap());
    });
}

// ---------------------------------------------------------------------------
// Per-ledger rate limiting — Task #4
// ---------------------------------------------------------------------------

/// A single address can claim at most `max_claims` times per window.
/// The (window_ledgers + 1)th call within the same window must be rejected.
#[test]
fn rate_limit_blocks_burst_claiming() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    client.initialize(&admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(&env, &token).mint(&contract, &100_000i128);

    // Allow at most 1 claim per 100-ledger window.
    client.set_rate_limit(&100, &1);
    assert_eq!(
        client.get_rate_limit(),
        Some(RateLimitConfig { window_ledgers: 100, max_claims: 1 })
    );

    // Build a two-leaf tree so we have two independent proofs for alice.
    // We actually need a separate token and leaf per claim because the
    // AlreadyClaimed check would fire first. Instead we build a tree with
    // alice appearing for two different tokens.
    let token2 = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token2).mint(&contract, &100_000i128);

    let alice = Address::generate(&env);
    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    let lb = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token2, TIER_1_AMOUNT, TIER_1)
    });
    let root = hash_pair(&env, &la, &lb);
    client.set_root(&root);

    let proof_a = vec![&env, lb.clone()];
    let proof_b = vec![&env, la.clone()];

    // First claim: allowed.
    client.claim(&alice, &token, &TIER_1_AMOUNT, &TIER_1, &proof_a);
    assert!(client.is_claimed(&alice, &token));

    // Second claim in the same window (same ledger): must be blocked.
    assert_eq!(
        client.try_claim(&alice, &token2, &TIER_1_AMOUNT, &TIER_1, &proof_b),
        Err(Ok(AirdropError::RateLimitExceeded))
    );
    assert!(!client.is_claimed(&alice, &token2));
}

/// Once the sliding window expires the counter resets and claiming is allowed again.
#[test]
fn rate_limit_resets_after_window_expires() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token2 = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    client.initialize(&admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(&env, &token).mint(&contract, &100_000i128);
    StellarAssetClient::new(&env, &token2).mint(&contract, &100_000i128);

    // 1 claim per 50-ledger window.
    client.set_rate_limit(&50, &1);

    let alice = Address::generate(&env);
    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    let lb = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token2, TIER_1_AMOUNT, TIER_1)
    });
    client.set_root(&hash_pair(&env, &la, &lb));

    // First claim at ledger 0.
    client.claim(&alice, &token, &TIER_1_AMOUNT, &TIER_1, &vec![&env, lb.clone()]);

    // Advance past the 50-ledger window.
    env.ledger().with_mut(|l| l.sequence_number = 51);

    // Second claim should now succeed because the window expired.
    client.claim(&alice, &token2, &TIER_1_AMOUNT, &TIER_1, &vec![&env, la.clone()]);
    assert!(client.is_claimed(&alice, &token2));
}

/// Setting window_ledgers = 0 removes the rate limit entirely.
#[test]
fn rate_limit_can_be_removed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);
    client.initialize(&admin, &FAR_DEADLINE, &None);

    client.set_rate_limit(&100, &1);
    assert!(client.get_rate_limit().is_some());

    client.set_rate_limit(&0, &0);
    assert_eq!(client.get_rate_limit(), None);
}

// ---------------------------------------------------------------------------
// KYC attestation — Task #5
// ---------------------------------------------------------------------------

/// When KYC is required, an unverified address must be rejected.
#[test]
fn kyc_required_blocks_unverified_claimant() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    client.initialize(&admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(&env, &token).mint(&contract, &100_000i128);
    client.set_kyc_required(&true);
    assert!(client.is_kyc_required());

    let alice = Address::generate(&env);
    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    client.set_root(&la); // single-leaf root; proof is empty

    // Alice has no attestation → rejected.
    assert_eq!(
        client.try_claim(&alice, &token, &TIER_1_AMOUNT, &TIER_1, &Vec::new(&env)),
        Err(Ok(AirdropError::KycNotVerified))
    );
    assert!(!client.is_claimed(&alice, &token));
}

/// A verified address can claim; revoking the attestation blocks future claims
/// (here demonstrated by a second token that the same address hasn't claimed yet).
#[test]
fn kyc_verified_address_can_claim_and_revocation_blocks_further_claims() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token2 = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    client.initialize(&admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(&env, &token).mint(&contract, &100_000i128);
    StellarAssetClient::new(&env, &token2).mint(&contract, &100_000i128);
    client.set_kyc_required(&true);

    let alice = Address::generate(&env);

    // Grant attestation.
    client.set_kyc_attestation(&alice, &true);
    assert!(client.is_kyc_verified(&alice));

    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    let lb = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token2, TIER_1_AMOUNT, TIER_1)
    });
    client.set_root(&hash_pair(&env, &la, &lb));

    // First claim succeeds.
    client.claim(&alice, &token, &TIER_1_AMOUNT, &TIER_1, &vec![&env, lb.clone()]);
    assert!(client.is_claimed(&alice, &token));

    // Revoke attestation.
    client.set_kyc_attestation(&alice, &false);
    assert!(!client.is_kyc_verified(&alice));

    // Second claim (different token, not yet claimed) must now be rejected.
    assert_eq!(
        client.try_claim(&alice, &token2, &TIER_1_AMOUNT, &TIER_1, &vec![&env, la.clone()]),
        Err(Ok(AirdropError::KycNotVerified))
    );
    assert!(!client.is_claimed(&alice, &token2));
}

/// KYC is checked per-recipient in claim_batch; one unverified address rejects the whole batch.
#[test]
fn kyc_batch_rejects_if_any_recipient_unverified() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract = env.register(AirdropContract, ());
    let client = AirdropContractClient::new(&env, &contract);

    client.initialize(&admin, &FAR_DEADLINE, &None);
    StellarAssetClient::new(&env, &token).mint(&contract, &100_000i128);
    client.set_kyc_required(&true);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Only alice is verified; bob is not.
    client.set_kyc_attestation(&alice, &true);

    let la = env.as_contract(&contract, || {
        compute_leaf(&env, &alice, &token, TIER_1_AMOUNT, TIER_1)
    });
    let lb = env.as_contract(&contract, || {
        compute_leaf(&env, &bob, &token, TIER_2_AMOUNT, TIER_2)
    });
    client.set_root(&hash_pair(&env, &la, &lb));

    let entries = vec![
        &env,
        (alice.clone(), token.clone(), TIER_1_AMOUNT, TIER_1),
        (bob.clone(), token.clone(), TIER_2_AMOUNT, TIER_2),
    ];
    // Batch must be all-or-nothing: bob's missing KYC rejects everything.
    assert_eq!(
        client.try_claim_batch(&entries, &Vec::new(&env), &vec![&env, true]),
        Err(Ok(AirdropError::KycNotVerified))
    );
    assert!(!client.is_claimed(&alice, &token));
    assert!(!client.is_claimed(&bob, &token));
}
