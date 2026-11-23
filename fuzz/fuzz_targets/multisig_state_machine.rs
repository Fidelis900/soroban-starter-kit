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
//! 5. After a signer replacement, no pending transaction retains a signature
//!    from the replaced signer (pending proposals are invalidated).
//! 6. Batch execution is all-or-nothing: a batch either executes every
//!    proposal or leaves all of them unexecuted.
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
    /// Whether the transaction has been invalidated by a signer replacement.
    invalidated: bool,
}

/// Minimal model of the multisig contract state.
struct Model {
    env: Env,
    signers: Vec<Address>,
    weights: Vec<u32>,
    threshold: u32,
    txs: Vec<Tx>,
    next_id: u64,
    /// Contract configuration nonce, bumped on every signer replacement.
    config_nonce: u64,
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
            config_nonce: 0,
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
            invalidated: false,
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

        if tx.executed || tx.invalidated || now >= tx.expires_at {
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

        if tx.executed || tx.invalidated || now >= tx.expires_at {
            return false;
        }
        if tx.accumulated_weight < self.threshold {
            return false;
        }

        tx.executed = true;
        self.txs.set(idx, tx);
        true
    }

    /// Attempt to execute a batch of transactions atomically.
    ///
    /// Mirrors `execute_batch`: every proposal must be executable (not
    /// executed, not invalidated, not expired, and meeting threshold) or the
    /// whole batch is rejected and no state changes.
    fn execute_batch(&mut self, ids: &Vec<u64>, now: u32) -> bool {
        // First pass: validate every proposal without mutating state.
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let idx = match self.tx_index(id) {
                Some(idx) => idx,
                None => return false,
            };
            let tx = self.txs.get(idx).unwrap();
            if tx.executed || tx.invalidated || now >= tx.expires_at {
                return false;
            }
            if tx.accumulated_weight < self.threshold {
                return false;
            }
        }

        // Second pass: commit every proposal.
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let idx = self.tx_index(id).unwrap();
            let mut tx = self.txs.get(idx).unwrap();
            tx.executed = true;
            self.txs.set(idx, tx);
        }
        true
    }

    /// Replace `old_signer` with `new_signer`.
    ///
    /// Mirrors `replace_signer`: the new signer must not already be registered,
    /// the old signer must exist, and every pending transaction signed by the
    /// old signer is invalidated. Returns `true` if the replacement happened.
    fn replace_signer(&mut self, old_signer: &Address, new_signer: &Address) -> bool {
        if self.weight_of(new_signer).is_some() {
            return false;
        }
        let old_idx = match self.weight_of(old_signer) {
            Some(_) => {
                let mut found = None;
                for i in 0..self.signers.len() {
                    if self.signers.get(i).unwrap() == *old_signer {
                        found = Some(i);
                        break;
                    }
                }
                match found {
                    Some(i) => i,
                    None => return false,
                }
            }
            None => return false,
        };

        // Invalidate every pending transaction that carries the old signature.
        for i in 0..self.txs.len() {
            let mut tx = self.txs.get(i).unwrap();
            if tx.executed {
                continue;
            }
            let mut signed_by_old = false;
            for j in 0..tx.signatures.len() {
                if tx.signatures.get(j).unwrap() == *old_signer {
                    signed_by_old = true;
                    break;
                }
            }
            if signed_by_old {
                tx.invalidated = true;
                tx.signatures = Vec::new(&self.env);
                tx.accumulated_weight = 0;
                self.txs.set(i, tx);
            }
        }

        self.signers.set(old_idx, new_signer.clone());
        self.config_nonce += 1;
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
                // Invariant 5: executed transactions were never invalidated.
                assert!(
                    !tx.invalidated,
                    "invalidated transaction executed"
                );
            }

            if tx.invalidated {
                // Invariant 5: invalidated transactions carry no signatures.
                assert_eq!(
                    tx.signatures.len(),
                    0,
                    "invalidated transaction retains signatures"
                );
                assert_eq!(
                    tx.accumulated_weight, 0,
                    "invalidated transaction retains weight"
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
        let op = next() % 6;
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
            3 => {
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
            4 => {
                // Execute a batch of random transactions atomically.
                if model.txs.len() > 0 {
                    let batch_len = (next() as u32 % 3) + 1;
                    let mut ids = Vec::new(&env);
                    for _ in 0..batch_len {
                        if model.txs.len() == 0 {
                            break;
                        }
                        let id = model
                            .txs
                            .get(next() as u32 % model.txs.len())
                            .unwrap()
                            .id;
                        ids.push_back(id);
                    }
                    let before: Vec<bool> = {
                        let mut v = Vec::new(&env);
                        for i in 0..ids.len() {
                            let id = ids.get(i).unwrap();
                            let executed = match model.tx_index(id) {
                                Some(idx) => model.txs.get(idx).unwrap().executed,
                                None => false,
                            };
                            v.push_back(executed);
                        }
                        v
                    };
                    let ok = model.execute_batch(&ids, now);
                    // Invariant 6: batch execution is all-or-nothing.
                    for i in 0..ids.len() {
                        let id = ids.get(i).unwrap();
                        let executed = match model.tx_index(id) {
                            Some(idx) => model.txs.get(idx).unwrap().executed,
                            None => false,
                        };
                        if ok {
                            assert!(
                                executed,
                                "successful batch left a proposal unexecuted"
                            );
                        } else {
                            assert_eq!(
                                executed,
                                before.get(i).unwrap(),
                                "failed batch partially executed"
                            );
                        }
                    }
                }
            }
            _ => {
                // Replace a signer with a fresh address.
                if model.signers.len() > 0 {
                    let old_signer = model
                        .signers
                        .get(next() as u32 % model.signers.len())
                        .unwrap();
                    let new_signer = Address::generate(&env);
                    let nonce_before = model.config_nonce;
                    let replaced = model.replace_signer(&old_signer, &new_signer);
                    if replaced {
                        assert_eq!(
                            model.config_nonce,
                            nonce_before + 1,
                            "signer replacement must bump config nonce"
                        );
                        // Invariant 5: no pending tx retains the old signature.
                        for i in 0..model.txs.len() {
                            let tx = model.txs.get(i).unwrap();
                            if tx.executed {
                                continue;
                            }
                            for j in 0..tx.signatures.len() {
                                assert_ne!(
                                    tx.signatures.get(j).unwrap(),
                                    old_signer,
                                    "replaced signer still present in pending tx"
                                );
                            }
                        }
                    }
                }
            }
        }

        model.check_invariants(now);
    }
});
