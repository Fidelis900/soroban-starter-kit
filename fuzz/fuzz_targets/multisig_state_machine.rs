//! Stateful invariant fuzz testing for `MultisigContract` signature
//! verification and threshold rules.
//!
//! This target drives a small in-memory model of the multisig contract
//! through random `sign` / `execute` / `cleanup` interleavings and asserts
//! the following invariants after every step:
//!
//! 1. `accumulated_weight` always equals the exact sum of the weights of the
//!    signers currently recorded in `signatures`.
//! 2. A transaction can never execute while `accumulated_weight < threshold`.
//! 3. Expired transactions can never execute.
//! 4. A signer can never double-sign the same transaction.
//!
//! The model mirrors the storage layout used by `MultisigContract` so the
//! invariants can be checked without a full Soroban host.

#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Address, Env, Vec};

/// Maximum number of signers tracked by the model. Keeps the fuzz corpus
/// small while still exercising threshold boundaries.
const MAX_SIGNERS: u32 = 8;

/// A single pending transaction in the model.
#[derive(Clone)]
struct Tx {
    id: u64,
    /// Ledger sequence at which the transaction expires.
    expires_at: u32,
    /// Signers that have signed this transaction, in insertion order.
    signatures: Vec<Address>,
    /// Cached sum of the weights of `signatures`.
    accumulated_weight: u32,
    /// Whether the transaction has already been executed.
    executed: bool,
}

/// Minimal model of the multisig contract state.
struct Model {
    env: Env,
    signers: Vec<Address>,
    weights: Vec<u32>,
    threshold: u32,
    txs: Vec<Tx>,
    next_id: u64,
}

impl Model {
    fn new(env: &Env, signers: Vec<Address>, weights: Vec<u32>, threshold: u32) -> Self {
        Model {
            env: env.clone(),
            signers,
            weights,
            threshold,
            txs: Vec::new(env),
            next_id: 0,
        }
    }

    /// Weight of a signer, or `None` if the address is not a registered signer.
    fn weight_of(&self, signer: &Address) -> Option<u32> {
        for i in 0..self.signers.len() {
            if self.signers.get(i).unwrap() == *signer {
                return Some(self.weights.get(i).unwrap());
            }
        }
        None
    }

    fn tx_index(&self, id: u64) -> Option<u32> {
        for i in 0..self.txs.len() {
            if self.txs.get(i).unwrap().id == id {
                return Some(i);
            }
        }
        None
    }

    /// Create a new pending transaction expiring at `expires_at`.
    fn create_tx(&mut self, expires_at: u32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.txs.push_back(Tx {
            id,
            expires_at,
            signatures: Vec::new(&self.env),
            accumulated_weight: 0,
            executed: false,
        });
        id
    }

    /// Record a signature. Returns `true` if the signature was accepted.
    ///
    /// Mirrors the contract rules: only registered signers may sign, a signer
    /// may not sign twice, and expired or executed transactions are rejected.
    fn sign(&mut self, id: u64, signer: &Address, now: u32) -> bool {
        let idx = match self.tx_index(id) {
            Some(i) => i,
            None => return false,
        };
        let mut tx = self.txs.get(idx).unwrap();

        if tx.executed || now >= tx.expires_at {
            return false;
        }
        let weight = match self.weight_of(signer) {
            Some(w) => w,
            None => return false,
        };
        for i in 0..tx.signatures.len() {
            if tx.signatures.get(i).unwrap() == *signer {
                // Double-sign attempt: must be rejected and must not change state.
                return false;
            }
        }

        tx.signatures.push_back(signer.clone());
        tx.accumulated_weight += weight;
        self.txs.set(idx, tx);
        true
    }

    /// Attempt to execute a transaction. Returns `true` if it executed.
    fn execute(&mut self, id: u64, now: u32) -> bool {
        let idx = match self.tx_index(id) {
            Some(i) => i,
            None => return false,
        };
        let mut tx = self.txs.get(idx).unwrap();

        if tx.executed || now >= tx.expires_at {
            return false;
        }
        if tx.accumulated_weight < self.threshold {
            return false;
        }

        tx.executed = true;
        self.txs.set(idx, tx);
        true
    }

    /// Remove an expired transaction from storage.
    fn cleanup(&mut self, id: u64, now: u32) {
        let idx = match self.tx_index(id) {
            Some(i) => i,
            None => return,
        };
        let tx = self.txs.get(idx).unwrap();
        if now >= tx.expires_at {
            self.txs.remove(idx);
        }
    }

    /// Assert every invariant that must hold after each operation.
    fn check_invariants(&self, now: u32) {
        for i in 0..self.txs.len() {
            let tx = self.txs.get(i).unwrap();

            // Invariant 1: accumulated_weight is the exact sum of signer weights.
            let mut expected: u32 = 0;
            for j in 0..tx.signatures.len() {
                let signer = tx.signatures.get(j).unwrap();
                let w = self
                    .weight_of(&signer)
                    .expect("signature from non-signer");
                expected = expected.checked_add(w).expect("weight overflow");
            }
            assert_eq!(
                tx.accumulated_weight, expected,
                "accumulated_weight must equal the sum of signer weights"
            );

            // Invariant 4: no duplicate signers in the signature set.
            for a in 0..tx.signatures.len() {
                for b in (a + 1)..tx.signatures.len() {
                    assert_ne!(
                        tx.signatures.get(a).unwrap(),
                        tx.signatures.get(b).unwrap(),
                        "signer double-signed the same transaction"
                    );
                }
            }

            if tx.executed {
                // Invariant 2: executed implies threshold was met.
                assert!(
                    tx.accumulated_weight >= self.threshold,
                    "transaction executed below threshold"
                );
                // Invariant 3: executed implies the transaction was not expired.
                assert!(
                    now < tx.expires_at,
                    "expired transaction executed"
                );
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    let env = Env::default();

    // Derive a small signer set and threshold from the fuzz input.
    let signer_count = (data[0] as u32 % MAX_SIGNERS) + 1;
    let mut signers = Vec::new(&env);
    let mut weights = Vec::new(&env);
    let mut total_weight: u32 = 0;
    for i in 0..signer_count {
        let signer = Address::generate(&env);
        let weight = (data[(1 + i as usize) % data.len()] as u32 % 5) + 1;
        total_weight += weight;
        signers.push_back(signer);
        weights.push_back(weight);
    }
    let threshold = (data[2] as u32 % (total_weight + 1)).max(1);

    let mut model = Model::new(&env, signers, weights, threshold);

    // Deterministic pseudo-random walk over the remaining input bytes.
    let mut cursor = 3usize;
    let mut now: u32 = 0;
    let mut next = || {
        let b = data[cursor % data.len()];
        cursor = cursor.wrapping_add(1);
        b
    };

    for _ in 0..64 {
        let op = next() % 4;
        match op {
            0 => {
                // Advance ledger time.
                now = now.saturating_add((next() as u32) % 10);
            }
            1 => {
                // Create a transaction with a random expiry.
                let expires_at = now.saturating_add((next() as u32) % 20);
                model.create_tx(expires_at);
            }
            2 => {
                // Sign a random transaction with a random signer.
                if model.txs.len() > 0 {
                    let id = model.txs.get(next() as u32 % model.txs.len()).unwrap().id;
                    let signer = model
                        .signers
                        .get(next() as u32 % model.signers.len())
                        .unwrap();
                    model.sign(id, &signer, now);
                }
            }
            _ => {
                // Execute or clean up a random transaction.
                if model.txs.len() > 0 {
                    let id = model.txs.get(next() as u32 % model.txs.len()).unwrap().id;
                    if next() % 2 == 0 {
                        model.execute(id, now);
                    } else {
                        model.cleanup(id, now);
                    }
                }
            }
        }

        model.check_invariants(now);
    }
});
