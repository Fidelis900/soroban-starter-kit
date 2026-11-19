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
    Address, Bytes, BytesN, Env, Vec,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    xdr::ToXdr,
};

// ---------------------------------------------------------------------------
// Merkle tree helpers (replicates on-chain logic for test setup)
// ---------------------------------------------------------------------------

fn sha256(env: &Env, data: &Bytes) -> BytesN<32> {
    env.crypto().sha256(data).into()
}

fn leaf(env: &Env, recipient: &Address, amount: i128) -> BytesN<32> {
    let mut data = Bytes::new(env);
    data.append(&recipient.clone().to_xdr(env));
    let amount_bytes: [u8; 16] = amount.to_be_bytes();
    data.append(&Bytes::from_slice(env, &amount_bytes));
    sha256(env, &data)
}

fn hash_pair(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::new(env);
    if a.to_array() <= b.to_array() {
        data.append(&Bytes::from(a.clone()));
        data.append(&Bytes::from(b.clone()));
    } else {
        data.append(&Bytes::from(b.clone()));
        data.append(&Bytes::from(a.clone()));
    }
    sha256(env, &data)
}

/// Build a two-leaf tree. Returns (root, proof_for_leaf_0, proof_for_leaf_1).
fn two_leaf_tree(
    env: &Env,
    leaf0: BytesN<32>,
    leaf1: BytesN<32>,
) -> (BytesN<32>, Vec<BytesN<32>>, Vec<BytesN<32>>) {
    let root = hash_pair(env, &leaf0, &leaf1);
    let mut proof0 = Vec::new(env);
    proof0.push_back(leaf1.clone());
    let mut proof1 = Vec::new(env);
    proof1.push_back(leaf0.clone());
    (root, proof0, proof1)
}

// ---------------------------------------------------------------------------
// Setup — now passes a claim_deadline far in the future by default
// ---------------------------------------------------------------------------

const FAR_DEADLINE: u32 = 1_000_000;

struct TestEnv<'a> {
    env: Env,
    client: AirdropContractClient<'a>,
    token: Address,
    admin: Address,
    alice: Address,
    bob: Address,
}

fn setup<'a>(env: &'a Env) -> TestEnv<'a> {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let alice = Address::generate(env);
    let bob = Address::generate(env);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let airdrop = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(env, &airdrop);
    client.initialize(&admin, &token, &FAR_DEADLINE);

    // Fund the airdrop contract with tokens
    StellarAssetClient::new(env, &token).mint(&airdrop, &100_000i128);

    TestEnv {
        env: env.clone(),
        client,
        token,
        admin,
        alice,
        bob,
    }
}

// ---------------------------------------------------------------------------
// Existing tests (updated for new initialize signature)
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_rejects_duplicate() {
    let env = Env::default();
    let t = setup(&env);
    let res = t.client.try_initialize(&t.admin, &t.token, &FAR_DEADLINE);
    assert!(res.is_err());
}

#[test]
fn test_claim_happy_path() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 1_000i128;
    let bob_amount = 2_000i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, _) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    let before = TokenClient::new(&env, &t.token).balance(&t.alice);
    t.client.claim(&1u32, &t.alice, &alice_amount, &proof_a);
    assert_eq!(
        TokenClient::new(&env, &t.token).balance(&t.alice),
        before + alice_amount
    );
    assert!(t.client.is_claimed(&1u32, &t.alice));
}

#[test]
fn test_duplicate_claim_rejected() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 500i128;
    let bob_amount = 500i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, _) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);
    t.client.claim(&1u32, &t.alice, &alice_amount, &proof_a);

    let res = t.client.try_claim(&1u32, &t.alice, &alice_amount, &proof_a);
    assert!(res.is_err());
}

#[test]
fn test_invalid_proof_rejected() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 1_000i128;
    let bob_amount = 2_000i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, _proof_a, proof_b) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    // Bob's proof used for Alice's claim — must fail
    let res = t.client.try_claim(&1u32, &t.alice, &alice_amount, &proof_b);
    assert!(res.is_err());
}

#[test]
fn test_wrong_amount_rejected() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 1_000i128;
    let bob_amount = 2_000i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, _) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    // Wrong amount
    let res = t.client.try_claim(&1u32, &t.alice, &999i128, &proof_a);
    assert!(res.is_err());
}

#[test]
fn test_zero_amount_rejected() {
    let env = Env::default();
    let t = setup(&env);
    let root = BytesN::from_array(&env, &[0u8; 32]);
    t.client.set_root(&1u32, &root);
    let proof = Vec::new(&env);
    let res = t.client.try_claim(&1u32, &t.alice, &0i128, &proof);
    assert!(res.is_err());
}

#[test]
fn test_claim_without_root_fails() {
    let env = Env::default();
    let t = setup(&env);
    let proof = Vec::new(&env);
    let res = t.client.try_claim(&1u32, &t.alice, &1_000i128, &proof);
    assert!(res.is_err());
}

#[test]
fn test_both_recipients_claim() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 300i128;
    let bob_amount = 700i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, proof_b) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);
    t.client.claim(&1u32, &t.alice, &alice_amount, &proof_a);
    t.client.claim(&1u32, &t.bob, &bob_amount, &proof_b);

    assert_eq!(
        TokenClient::new(&env, &t.token).balance(&t.alice),
        alice_amount
    );
    assert_eq!(TokenClient::new(&env, &t.token).balance(&t.bob), bob_amount);
}

// ---------------------------------------------------------------------------
// Lenient batch claim tests — #1150
// ---------------------------------------------------------------------------

/// A batch containing a duplicate recipient should still fulfil the valid
/// entries and return only the successfully claimed addresses.
#[test]
fn test_claim_batch_lenient_skips_duplicates() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 400i128;
    let bob_amount = 600i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, proof_b) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    // Alice appears twice; the second entry must be skipped, not revert.
    let mut claims = Vec::new(&env);
    claims.push_back((t.alice.clone(), alice_amount, proof_a.clone()));
    claims.push_back((t.alice.clone(), alice_amount, proof_a.clone()));
    claims.push_back((t.bob.clone(), bob_amount, proof_b.clone()));

    let claimed = t.client.claim_batch_lenient(&1u32, &claims);

    assert_eq!(claimed.len(), 2);
    assert!(claimed.contains(&t.alice));
    assert!(claimed.contains(&t.bob));
    assert_eq!(
        TokenClient::new(&env, &t.token).balance(&t.alice),
        alice_amount
    );
    assert_eq!(TokenClient::new(&env, &t.token).balance(&t.bob), bob_amount);
}

/// A batch containing an invalid proof should skip that entry while still
/// fulfilling the valid ones.
#[test]
fn test_claim_batch_lenient_skips_invalid_proof() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 250i128;
    let bob_amount = 750i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, proof_b) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    // Alice's entry uses Bob's proof — invalid, must be skipped.
    let mut claims = Vec::new(&env);
    claims.push_back((t.alice.clone(), alice_amount, proof_b.clone()));
    claims.push_back((t.bob.clone(), bob_amount, proof_b.clone()));

    let claimed = t.client.claim_batch_lenient(&1u32, &claims);

    assert_eq!(claimed.len(), 1);
    assert!(claimed.contains(&t.bob));
    assert!(!claimed.contains(&t.alice));
    assert_eq!(TokenClient::new(&env, &t.token).balance(&t.alice), 0);
    assert_eq!(TokenClient::new(&env, &t.token).balance(&t.bob), bob_amount);
}

/// Already-claimed recipients in a lenient batch are skipped without
/// reverting the whole batch.
#[test]
fn test_claim_batch_lenient_skips_already_claimed() {
    let env = Env::default();
    let t = setup(&env);

    let alice_amount = 100i128;
    let bob_amount = 200i128;

    let leaf_a = leaf(&env, &t.alice, alice_amount);
    let leaf_b = leaf(&env, &t.bob, bob_amount);
    let (root, proof_a, proof_b) = two_leaf_tree(&env, leaf_a, leaf_b);

    t.client.set_root(&1u32, &root);

    // Alice claims first via the strict path.
    t.client.claim(&1u32, &t.alice, &alice_amount, &proof_a);

    let mut claims = Vec::new(&env);
    claims.push_back((t.alice.clone(), alice_amount, proof_a.clone()));
    claims.push_back((t.bob.clone(), bob_amount, proof_b.clone()));

    let claimed = t.client.claim_batch_lenient(&1u32, &claims);

    assert_eq!(claimed.len(), 1);
    assert!(claimed.contains(&t.bob));
    assert_eq!(
        TokenClient::new(&env, &t.token).balance(&t.alice),
        alice_amount
    );
    assert_eq!(TokenClient::new(&env, &t.token).balance(&t.bob), bob_amount);
}

// ---------------------------------------------------------------------------
// Claim deadline tests — #780
// ---------------------------------------------------------------------------

/// Claim succeeds when ledger sequence < deadline.
#[test]
fn test_claim_before_deadline_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    // Start at ledger 100; deadline = 200
    env.ledger().with_mut(|l| l.sequence_number = 100);

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let airdrop = env.register_contract(None, AirdropContract);
    let client = AirdropContractClient::new(&env, &airdrop);
    client.initialize(&admin, &token, &200u32);
    StellarAssetClient::new(&env, &token).mint(&airdrop, &10_000i128);

    let leaf_a = leaf(&env, &alice, 500i128);
    let leaf_b = leaf(&env, &bob, 500i128);
    let (root, proof_a, _) = two_leaf_tree(&env, leaf_a, leaf_b);
    client.set_root(&1u32, &root);

    // ledger 100 < 200 — should succeed
    client.claim(&1u32, &alice, &500i128, &proof_a);
    assert_eq!(TokenClient::new(&env, &token).balance(&alice), 500i128);
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
        client.try_claim(&recipient, &token, &100, &proof),
        Err(Ok(AirdropError::Paused))
    );

    client.set_paused(&false);
    assert!(!client.is_paused());
    // With claims resumed the pause error is gone (the root is not set yet).
    assert_eq!(
        client.try_claim(&recipient, &token, &100, &proof),
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

    // Inside the full-reward window: the whole allocation is claimable.
    env.ledger().with_mut(|li| li.sequence_number = 50);
    assert_eq!(decayed_amount(&env, 1_000).unwrap(), (1_000, 0));

    // Three decay steps elapsed: 70% to claimant, 30% back to the treasury.
    env.ledger().with_mut(|li| li.sequence_number = 130);
    assert_eq!(decayed_amount(&env, 1_000).unwrap(), (700, 300));
    assert_eq!(decay_bps(&config, 130), 7_000);

    // The schedule never pays less than the configured floor.
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
        assert!(env.storage().instance().has(&DataKey::MerkleRoot(1)));
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
        assert!(env.storage().instance().has(&DataKey::MerkleRoot(2)));
        assert!(env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Claimed(alice.clone(), token.clone()))
            .unwrap());
    });
}
