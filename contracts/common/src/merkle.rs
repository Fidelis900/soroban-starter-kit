//! SHA-256 sorted-pair Merkle tree verification.
//!
//! Provides single-proof verification and gas-optimized **multi-proof**
//! verification. A multi-proof proves several leaves against one root at the
//! same time: every intermediate node shared between the leaves' paths is
//! hashed exactly once, so a batch of `k` leaves in a tree of depth `d` costs
//! at most `k + |proof| - 1` hashes instead of `k * d`.
//!
//! Internal nodes are `sha256(min(a, b) || max(a, b))` (commutative, "sorted
//! pair") — the same layout as the `OpenZeppelin` `MerkleProof` library, but
//! with `SHA-256` instead of `Keccak-256`. The multi-proof format (leaves +
//! proof hashes + boolean flags) is identical to `OpenZeppelin`'s
//! `multiProofVerify`, so off-chain tooling that emits those multi-proofs can
//! be reused with a `SHA-256` hasher.
//!
//! # Soundness
//!
//! Callers must make sure a leaf hash can never equal an internal node hash.
//! Internal-node preimages are exactly 64 bytes; hashing leaves from a
//! preimage of a different length (for example one prefixed with a domain tag)
//! is sufficient.

use soroban_sdk::{Bytes, BytesN, Env, Vec};

/// Hash two nodes in sorted order: `sha256(min(a, b) || max(a, b))`.
#[must_use]
pub fn hash_pair_sorted(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let (a, b) = (a.to_array(), b.to_array());
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut data = Bytes::new(env);
    data.extend_from_array(&lo);
    data.extend_from_array(&hi);
    env.crypto().sha256(&data).into()
}

/// Verify a single Merkle proof.
///
/// `proof` lists the sibling hashes from the leaf up to (but excluding) the
/// root. Costs `proof.len()` hashes.
#[must_use]
pub fn verify_merkle_proof(
    env: &Env,
    leaf: &BytesN<32>,
    proof: &Vec<BytesN<32>>,
    root: &BytesN<32>,
) -> bool {
    let mut current = leaf.clone();
    for sibling in proof.iter() {
        current = hash_pair_sorted(env, &current, &sibling);
    }
    &current == root
}

/// Verify that every leaf in `leaves` is part of the tree with root `root`,
/// using a single shared multi-proof.
///
/// The tree is rebuilt bottom-up by consuming a queue that starts as `leaves`
/// and is extended with each freshly computed node. For step `i`:
///
/// * the left operand is the next item of the queue;
/// * the right operand is the next item of the queue if `proof_flags[i]` is
///   `true`, otherwise the next hash from `proof`.
///
/// The last computed node must equal `root`. `leaves` must be supplied in the
/// order produced by the multi-proof generator (tree order), not arbitrary
/// order.
///
/// Costs exactly `proof_flags.len()` hashes (`= leaves.len() + proof.len() - 1`).
///
/// Returns `false` (never panics) for any malformed input: an empty leaf set,
/// inconsistent lengths, a flag sequence that reads a node before it is
/// computed, or a proof whose hashes are not all consumed.
#[must_use]
pub fn verify_merkle_multi_proof(
    env: &Env,
    leaves: &Vec<BytesN<32>>,
    proof: &Vec<BytesN<32>>,
    proof_flags: &Vec<bool>,
    root: &BytesN<32>,
) -> bool {
    let leaves_len = leaves.len();
    let proof_len = proof.len();
    let total_hashes = proof_flags.len();

    // Rejecting an empty leaf set closes the degenerate "prove nothing against
    // proof[0] == root" case.
    if leaves_len == 0 {
        return false;
    }

    // Each hash consumes two operands and produces one node, and the final
    // node is the root: leaves + proof = hashes + 1.
    match leaves_len.checked_add(proof_len) {
        Some(n) if total_hashes.checked_add(1) == Some(n) => {}
        _ => return false,
    }

    if total_hashes == 0 {
        // Single-leaf tree: the leaf is the root.
        return leaves.get(0).as_ref() == Some(root);
    }

    let mut hashes: Vec<BytesN<32>> = Vec::new(env);
    let mut leaf_pos: u32 = 0;
    let mut hash_pos: u32 = 0;
    let mut proof_pos: u32 = 0;

    // Pop the next node from the leaves, then from computed hashes. Reading a
    // hash that has not been computed yet yields `None` and aborts.
    let mut next_node = |hashes: &Vec<BytesN<32>>| -> Option<BytesN<32>> {
        if leaf_pos < leaves_len {
            let node = leaves.get(leaf_pos);
            leaf_pos = leaf_pos.saturating_add(1);
            node
        } else {
            let node = hashes.get(hash_pos);
            hash_pos = hash_pos.saturating_add(1);
            node
        }
    };

    for flag in proof_flags.iter() {
        let Some(a) = next_node(&hashes) else {
            return false;
        };
        let b = if flag {
            next_node(&hashes)
        } else {
            let node = proof.get(proof_pos);
            proof_pos = proof_pos.saturating_add(1);
            node
        };
        let Some(b) = b else {
            return false;
        };
        hashes.push_back(hash_pair_sorted(env, &a, &b));
    }

    // Every proof hash must be consumed; otherwise unrelated data could ride
    // along with a valid proof.
    if proof_pos != proof_len {
        return false;
    }

    hashes.last().as_ref() == Some(root)
}
