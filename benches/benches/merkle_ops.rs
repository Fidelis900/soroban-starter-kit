#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::integer_division,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]
//! Criterion benchmarks: individual Merkle proofs vs. one multi-proof.
//!
//! Closes #1152. Verifies a batch of 50 leaves in a 1024-leaf (depth 10)
//! sorted-pair SHA-256 tree both ways, prints the Soroban CPU instruction
//! count of each (the on-chain cost proxy) and the relative saving, then
//! times both with Criterion.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use soroban_sdk::{Bytes, BytesN, Env, Vec};

use soroban_common::{hash_pair_sorted, verify_merkle_multi_proof, verify_merkle_proof};

const TREE_LEAVES: usize = 1024;
const BATCH: usize = 50;

/// Complete binary tree in array layout: root at 0, children of `i` at
/// `2i + 1` / `2i + 2`, leaves occupying the last `TREE_LEAVES` slots.
struct Tree {
    nodes: std::vec::Vec<BytesN<32>>,
}

impl Tree {
    fn build(env: &Env, n: usize) -> Self {
        assert!(n.is_power_of_two());
        let mut nodes = std::vec::Vec::with_capacity(2 * n - 1);
        nodes.resize(2 * n - 1, BytesN::from_array(env, &[0u8; 32]));
        for i in 0..n {
            let preimage = Bytes::from_slice(env, &(i as u64).to_be_bytes());
            nodes[n - 1 + i] = env.crypto().sha256(&preimage).into();
        }
        for i in (0..n - 1).rev() {
            nodes[i] = hash_pair_sorted(env, &nodes[2 * i + 1], &nodes[2 * i + 2]);
        }
        Self { nodes }
    }

    fn root(&self) -> BytesN<32> {
        self.nodes[0].clone()
    }

    fn leaf_index(&self, leaf: usize) -> usize {
        self.nodes.len() / 2 + leaf
    }

    fn sibling(i: usize) -> usize {
        if i % 2 == 1 { i + 1 } else { i - 1 }
    }

    fn proof(&self, env: &Env, leaf: usize) -> Vec<BytesN<32>> {
        let mut proof = Vec::new(env);
        let mut i = self.leaf_index(leaf);
        while i > 0 {
            proof.push_back(self.nodes[Self::sibling(i)].clone());
            i = (i - 1) / 2;
        }
        proof
    }

    /// Multi-proof in `OpenZeppelin` `getMultiProof` format. Returns
    /// `(leaves_in_proof_order, proof, proof_flags)`.
    fn multi_proof(
        &self,
        env: &Env,
        leaves: &[usize],
    ) -> (Vec<BytesN<32>>, Vec<BytesN<32>>, Vec<bool>) {
        let mut indices: std::vec::Vec<usize> =
            leaves.iter().map(|&l| self.leaf_index(l)).collect();
        indices.sort_unstable_by(|a, b| b.cmp(a));

        let mut stack: std::collections::VecDeque<usize> = indices.iter().copied().collect();
        let mut proof = Vec::new(env);
        let mut flags = Vec::new(env);
        while let Some(&j) = stack.front() {
            if j == 0 {
                break;
            }
            stack.pop_front();
            let s = Self::sibling(j);
            if stack.front() == Some(&s) {
                flags.push_back(true);
                stack.pop_front();
            } else {
                flags.push_back(false);
                proof.push_back(self.nodes[s].clone());
            }
            stack.push_back((j - 1) / 2);
        }

        let mut ordered = Vec::new(env);
        for i in indices {
            ordered.push_back(self.nodes[i].clone());
        }
        (ordered, proof, flags)
    }
}

struct Fixture {
    env: Env,
    root: BytesN<32>,
    leaves: std::vec::Vec<BytesN<32>>,
    proofs: std::vec::Vec<Vec<BytesN<32>>>,
    multi_leaves: Vec<BytesN<32>>,
    multi_proof: Vec<BytesN<32>>,
    multi_flags: Vec<bool>,
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.budget().reset_unlimited();
    let tree = Tree::build(&env, TREE_LEAVES);

    // A contiguous claim window, as produced by a relayer batching recent claimants.
    let selected: std::vec::Vec<usize> = (0..BATCH).collect();
    let leaves = selected
        .iter()
        .map(|&l| tree.nodes[tree.leaf_index(l)].clone())
        .collect();
    let proofs = selected.iter().map(|&l| tree.proof(&env, l)).collect();
    let (multi_leaves, multi_proof, multi_flags) = tree.multi_proof(&env, &selected);

    Fixture {
        root: tree.root(),
        env,
        leaves,
        proofs,
        multi_leaves,
        multi_proof,
        multi_flags,
    }
}

fn verify_individually(f: &Fixture) -> bool {
    f.leaves
        .iter()
        .zip(&f.proofs)
        .all(|(leaf, proof)| verify_merkle_proof(&f.env, leaf, proof, &f.root))
}

fn verify_multi(f: &Fixture) -> bool {
    verify_merkle_multi_proof(
        &f.env,
        &f.multi_leaves,
        &f.multi_proof,
        &f.multi_flags,
        &f.root,
    )
}

fn cpu_instructions(f: &Fixture, run: impl FnOnce(&Fixture) -> bool) -> u64 {
    f.env.budget().reset_unlimited();
    assert!(run(f), "proof must verify");
    f.env.budget().cpu_instruction_cost()
}

fn bench_merkle(c: &mut Criterion) {
    let f = fixture();

    let individual = cpu_instructions(&f, verify_individually);
    let multi = cpu_instructions(&f, verify_multi);
    println!(
        "merkle: {BATCH} leaves / {TREE_LEAVES}-leaf tree | individual: {} hashes, {individual} cpu insns | \
         multi-proof: {} hashes, {multi} cpu insns | saving: {:.1}%",
        f.proofs.iter().map(Vec::len).sum::<u32>(),
        f.multi_flags.len(),
        100.0 * (1.0 - multi as f64 / individual as f64),
    );

    let mut group = c.benchmark_group("merkle::verify_50_of_1024");
    group.bench_function("individual_proofs", |b| {
        b.iter(|| assert!(verify_individually(black_box(&f))));
    });
    group.bench_function("multi_proof", |b| {
        b.iter(|| assert!(verify_multi(black_box(&f))));
    });
    group.finish();
}

criterion_group!(benches, bench_merkle);
criterion_main!(benches);
