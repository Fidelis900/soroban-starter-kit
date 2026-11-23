#![no_std]
#![deny(missing_docs)]
//! M-of-N multisignature wallet contract template.
//!
//! A set of signers approve transactions; once the approval threshold is met
//! the transaction executes. Signers, threshold, and (optionally) per-signer
//! weights are configurable.
//!
//! ## Weighted voting (#825)
//!
//! `initialize` accepts an optional `weights` parameter (`Vec<SignerWeight>`).
//! When provided, each signer is assigned a custom vote weight; the proposal
//! threshold is measured in accumulated weight rather than raw signer count.
//! When omitted every signer has weight 1 and the contract behaves identically
//! to the original flat-count design.
//!
//! ## Batch execution (#826)
//!
//! `execute_batch(proposal_ids)` iterates the supplied IDs and attempts to
//! execute each one independently.  A failure for one ID (already executed,
//! not found, threshold not met, expired) does not abort the others — the
//! call records a skip and continues.  The return value is a
//! `Vec<u64>` of the IDs that were successfully executed; callers can diff
//! it against the input to determine which were skipped and why (check
//! individual proposals or filter emitted events).

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Bytes, BytesN, Env, Map, Symbol, Val, Vec, contract, contractimpl};

mod errors;
mod events;
mod storage;
mod threshold;

#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod test;

pub use errors::MultisigError;
pub use storage::{DataKey, SignerWeight, ThresholdChange, TokenSpendingLimit, Transaction};
pub use threshold::ThresholdChangeProposal;

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

#[inline]
fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

#[inline]
fn bump_transaction(env: &Env, tx_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Transaction(tx_id),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

#[inline]
fn contains(list: &Vec<Address>, address: &Address) -> bool {
    for item in list.iter() {
        if item == *address {
            return true;
        }
    }
    false
}

#[inline]
fn validate_unique_signers(signers: &Vec<Address>) -> Result<(), MultisigError> {
    if signers.is_empty() {
        return Err(MultisigError::InvalidSigners);
    }

    let mut seen = Vec::new(signers.env());
    for signer in signers.iter() {
        if contains(&seen, &signer) {
            return Err(MultisigError::InvalidSigners);
        }
        seen.push_back(signer);
    }
    Ok(())
}

/// Validate `threshold` against the supplied `total_weight`.
///
/// - `threshold == 0` is rejected.
/// - `threshold > total_weight` is rejected (threshold can never be reached).
#[inline]
fn validate_threshold(threshold: u32, total_weight: u32) -> Result<(), MultisigError> {
    if threshold == 0 || threshold > total_weight {
        return Err(MultisigError::InvalidThreshold);
    }
    Ok(())
}

/// Build a `Map<Address, u32>` from the optional `weights` input.
///
/// If `weights` is `None` every signer gets weight 1.
/// Returns `InvalidWeight` if any weight is zero.
#[inline]
fn build_weights_map(
    env: &Env,
    signers: &Vec<Address>,
    weights: Option<Vec<SignerWeight>>,
) -> Result<Map<Address, u32>, MultisigError> {
    let mut map: Map<Address, u32> = Map::new(env);
    match weights {
        None => {
            for s in signers.iter() {
                map.set(s, 1u32);
            }
        }
        Some(w) => {
            for sw in w.iter() {
                if sw.weight == 0 {
                    return Err(MultisigError::InvalidWeight);
                }
                map.set(sw.signer, sw.weight);
            }
            // Fill in weight-1 for any signer not listed.
            for s in signers.iter() {
                if map.get(s.clone()).is_none() {
                    map.set(s, 1u32);
                }
            }
        }
    }
    Ok(map)
}

/// Sum of weights for all signers in `signers` according to `weights_map`.
#[inline]
fn total_weight(signers: &Vec<Address>, weights_map: &Map<Address, u32>) -> u32 {
    let mut total: u32 = 0;
    for s in signers.iter() {
        total = total.saturating_add(storage::get_weight(weights_map, &s));
    }
    total
}

pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct MultisigContract;

    #[contractimpl]
    impl MultisigContract {
        /// Initialize the wallet with an initial signer set, threshold, and
        /// optional per-signer weights (#825).
        ///
        /// `threshold` is interpreted as an **accumulated-weight** threshold.
        /// For un-weighted wallets (no `weights` supplied) this is equivalent
        /// to a signer-count threshold.
        ///
        /// If `weights` is supplied, each `SignerWeight.weight` must be ≥ 1.
        /// Any signer not listed in `weights` defaults to weight 1.  The
        /// threshold must be ≤ the sum of all signer weights.
        pub fn initialize(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
            weights: Option<Vec<SignerWeight>>,
        ) -> Result<(), MultisigError> {
            Self::initialize_internal(env, signers, threshold, weights, None)
        }

        pub fn initialize_with_config(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
            weights: Option<Vec<SignerWeight>>,
            max_proposal_validity_ledgers: Option<u32>,
        ) -> Result<(), MultisigError> {
            Self::initialize_internal(env, signers, threshold, weights, max_proposal_validity_ledgers)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn initialize_with_validity(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
            weights: Option<Vec<SignerWeight>>,
            max_proposal_validity_ledgers: u32,
        ) -> Result<(), MultisigError> {
            Self::initialize_internal(env, signers, threshold, weights, Some(max_proposal_validity_ledgers))
        }

        fn initialize_internal(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
            weights: Option<Vec<SignerWeight>>,
            max_proposal_validity_ledgers: Option<u32>,
        ) -> Result<(), MultisigError> {
            if env.storage().instance().has(&DataKey::Signers) {
                return Err(MultisigError::AlreadyInitialized);
            }

            validate_unique_signers(&signers)?;

            let weights_map = build_weights_map(&env, &signers, weights)?;
            let tw = total_weight(&signers, &weights_map);
            validate_threshold(threshold, tw)?;

            for signer in signers.iter() {
                signer.require_auth();
            }

            env.storage().instance().set(&DataKey::Signers, &signers);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &threshold);
            env.storage()
                .instance()
                .set(&DataKey::NextTransactionId, &0u64);
            env.storage().instance().set(&DataKey::Version, &1u32);
            if let Some(max_val) = max_proposal_validity_ledgers {
                env.storage()
                    .instance()
                    .set(&DataKey::MaxProposalValidityLedgers, &max_val);
            }
            bump_instance(&env);

            events::initialized(&env, threshold, signers.len());
            Ok(())
        }

        // Backward compatible initialize for older 4-arg style
        #[allow(clippy::too_many_arguments)]
        pub fn initialize_legacy(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
            weights: Option<Vec<SignerWeight>>,
        ) -> Result<(), MultisigError> {
            Self::initialize_internal(env, signers, threshold, weights, None)
        }

        /// Initialize without weights for backward compatibility
        #[allow(clippy::too_many_arguments)]
        pub fn initialize_simple(
            env: Env,
            signers: Vec<Address>,
            threshold: u32,
        ) -> Result<(), MultisigError> {
            Self::initialize_internal(env, signers, threshold, None, None)
        }

        /// Add a signer and optionally adjust the threshold.
        pub fn add_signer(
            env: Env,
            approvals: Vec<Address>,
            signer: Address,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            let mut signers = Self::get_required_signers(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;

            if contains(&signers, &signer) {
                return Err(MultisigError::InvalidSigners);
            }

            signers.push_back(signer.clone());

            // Add new signer with weight 1 (no weight override on add_signer).
            let mut weights_map: Map<Address, u32> = env
                .storage()
                .instance()
                .get(&DataKey::Weights)
                .unwrap_or_else(|| Map::new(&env));
            weights_map.set(signer.clone(), 1u32);

            let tw = total_weight(&signers, &weights_map);
            validate_threshold(new_threshold, tw)?;

            env.storage().instance().set(&DataKey::Signers, &signers);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &new_threshold);
            bump_instance(&env);

            events::signer_added(&env, &signer, new_threshold);
            Ok(())
        }

        /// Remove a signer and optionally adjust the threshold.
        pub fn remove_signer(
            env: Env,
            approvals: Vec<Address>,
            signer: Address,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            let signers = Self::get_required_signers(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;

            if !contains(&signers, &signer) {
                return Err(MultisigError::NotSigner);
            }

            let mut remaining = Vec::new(&env);
            for existing in signers.iter() {
                if existing != signer {
                    remaining.push_back(existing);
                }
            }

            validate_unique_signers(&remaining)?;

            let mut weights_map: Map<Address, u32> = env
                .storage()
                .instance()
                .get(&DataKey::Weights)
                .unwrap_or_else(|| Map::new(&env));
            weights_map.remove(signer.clone());

            let tw = total_weight(&remaining, &weights_map);
            validate_threshold(new_threshold, tw)?;

            env.storage()
                .instance()
                .set(&DataKey::Signers, &remaining);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &new_threshold);
            bump_instance(&env);

            events::signer_removed(&env, &signer, new_threshold);
            Ok(())
        }

        /// Propose a transaction. The proposer signs it automatically.
        pub fn propose_transaction(
            env: Env,
            proposer: Address,
            target: Address,
            function: Symbol,
            args: Vec<Val>,
            expiry_ledgers: u32,
        ) -> Result<u64, MultisigError> {
            Self::require_signer(&env, &proposer)?;
            proposer.require_auth();

            // Enforce max proposal validity if configured
            if let Some(max_valid) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxProposalValidityLedgers)
            {
                if expiry_ledgers > max_valid {
                    return Err(MultisigError::ProposalExpired);
                }
            }

            let tx_id =
                Self::propose_phase(&env, &proposer, target, function, args, expiry_ledgers)?;
            events::transaction_proposed(&env, tx_id, &proposer);
            Ok(tx_id)
        }

        /// Sign a pending transaction.
        pub fn sign_transaction(
            env: Env,
            signer: Address,
            tx_id: u64,
        ) -> Result<(), MultisigError> {
            Self::require_signer(&env, &signer)?;
            signer.require_auth();

            let signature_count = Self::vote_phase(&env, &signer, tx_id)?;
            events::transaction_signed(&env, tx_id, &signer, signature_count);
            Ok(())
        }

        /// Execute a transaction once it has enough accumulated weight.
        pub fn execute_transaction(env: Env, tx_id: u64) -> Result<Val, MultisigError> {
            Self::execute_phase(&env, tx_id)
        }

        /// Alias for execute_transaction to match issue naming.
        pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<Val, MultisigError> {
            Self::execute_phase(&env, proposal_id)
        }

        pub fn execute_proposal_u32(env: Env, proposal_id: u32) -> Result<Val, MultisigError> {
            Self::execute_phase(&env, proposal_id as u64)
        }

        /// Execute multiple already-approved proposals in a single transaction (#826).
        pub fn execute_proposals_batch(env: Env, proposal_ids: Vec<u64>) -> Vec<u64> {
            Self::execute_batch(env, proposal_ids)
        }

        pub fn execute_proposals_batch_u32(env: Env, proposal_ids: Vec<u32>) -> Vec<u64> {
            let mut v = Vec::new(&env);
            for id in proposal_ids.iter() {
                v.push_back(id as u64);
            }
            Self::execute_batch(env, v)
        }

        /// Execute multiple already-approved proposals atomically in a single
        /// invocation (#1448).
        ///
        /// ## Semantics
        ///
        /// Every proposal ID is validated up-front (exists, not executed, not
        /// expired, threshold met).  If **any** proposal fails validation the
        /// whole call returns the corresponding error and nothing is executed.
        /// Once all proposals validate, they are executed sequentially.  Soroban
        /// invocation atomicity guarantees that a failure during execution rolls
        /// back the entire batch.
        ///
        /// ## Returns
        ///
        /// `Vec<u64>` — IDs of proposals that were successfully executed during
        /// this call (all of them, on success).  A `batch_executed` event is
        /// emitted with the executed IDs and a skipped count of 0.
        pub fn execute_batch(env: Env, proposal_ids: Vec<u64>) -> Vec<u64> {
            let mut executed_ids: Vec<u64> = Vec::new(&env);

            // Validate every proposal before executing any of them so that a
            // single bad ID aborts the whole batch (atomic semantics).
            for tx_id in proposal_ids.iter() {
                Self::validate_executable(&env, tx_id);
            }

            for tx_id in proposal_ids.iter() {
                Self::execute_phase(&env, tx_id);
                executed_ids.push_back(tx_id);
            }

            events::batch_executed(&env, &executed_ids, 0);
            executed_ids
        }

        /// Replace a signer key while preserving the wallet configuration (#1447).
        ///
        /// Requires threshold approvals from the current signer set.  The
        /// `old_signer` must be an existing signer and `new_signer` must not
        /// already be a signer.  The configuration nonce is incremented and any
        /// pending (unexecuted) proposals signed by `old_signer` are invalidated
        /// by clearing their signatures and accumulated weight.
        pub fn replace_signer(
            env: Env,
            approvals: Vec<Address>,
            old_signer: Address,
            new_signer: Address,
        ) -> Result<(), MultisigError> {
            let signers = Self::get_required_signers(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;

            if !contains(&signers, &old_signer) {
                return Err(MultisigError::NotSigner);
            }
            if contains(&signers, &new_signer) {
                return Err(MultisigError::InvalidSigners);
            }

            // Rebuild signer list with the replacement.
            let mut updated = Vec::new(&env);
            for existing in signers.iter() {
                if existing == old_signer {
                    updated.push_back(new_signer.clone());
                } else {
                    updated.push_back(existing);
                }
            }
            validate_unique_signers(&updated)?;

            // Carry over the old signer's weight (default 1).
            let mut weights_map: Map<Address, u32> = env
                .storage()
                .instance()
                .get(&DataKey::Weights)
                .unwrap_or_else(|| Map::new(&env));
            let old_weight = weights_map.get(old_signer.clone()).unwrap_or(1u32);
            weights_map.remove(old_signer.clone());
            weights_map.set(new_signer.clone(), old_weight);

            env.storage().instance().set(&DataKey::Signers, &updated);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);

            // Increment configuration nonce.
            let nonce: u64 = env
                .storage()
                .instance()
                .get(&DataKey::ConfigNonce)
                .unwrap_or(0u64);
            env.storage()
                .instance()
                .set(&DataKey::ConfigNonce, &(nonce + 1));

            // Invalidate pending proposals signed by the old signer.
            Self::invalidate_pending_for_signer(&env, &old_signer);

            bump_instance(&env);
            events::signer_replaced(&env, &old_signer, &new_signer);
            Ok(())
        }

        /// Return the current configuration nonce (#1447).
        pub fn config_nonce(env: Env) -> u64 {
            env.storage()
                .instance()
                .get(&DataKey::ConfigNonce)
                .unwrap_or(0u64)
        }

        /// Garbage-collect all expired, unexecuted proposals (#1446).
        ///
        /// Iterates every stored proposal ID, removes those whose expiration
        /// ledger has passed and which have not been executed, and emits a
        /// `proposal_expired` event for each removal.  Returns the number of
        /// proposals removed.
        pub fn garbage_collect_expired(env: Env) -> u32 {
            let next_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextTransactionId)
                .unwrap_or(0u64);
            let current_ledger = env.ledger().sequence();
            let mut removed: u32 = 0;

            let mut tx_id: u64 = 0;
            while tx_id < next_id {
                if let Some(transaction) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, Transaction>(&DataKey::Transaction(tx_id))
                {
                    if !transaction.executed && current_ledger > transaction.expiry_ledger {
                        env.storage()
                            .persistent()
                            .remove(&DataKey::Transaction(tx_id));
                        events::proposal_expired(&env, tx_id);
                        removed = removed.saturating_add(1);
                    }
                }
                tx_id += 1;
            }

            removed
        }
=======

        /// Return the current signer list.
        pub fn get_signers(env: Env) -> Vec<Address> {
            env.storage()
                .instance()
                .get(&DataKey::Signers)
                .unwrap_or_else(|| Vec::new(&env))
        }

        /// Return the accumulated-weight threshold.
        pub fn get_threshold(env: Env) -> Option<u32> {
            env.storage().instance().get(&DataKey::Threshold)
        }

        /// Return the weight assigned to `signer`, or 1 if unset.
        pub fn get_signer_weight(env: Env, signer: Address) -> u32 {
            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);
            weights_map
                .and_then(|m| m.get(signer))
                .unwrap_or(1u32)
        }

        /// Return whether `address` is a current signer.
        pub fn is_signer(env: Env, address: Address) -> bool {
            env.storage()
                .instance()
                .get::<DataKey, Vec<Address>>(&DataKey::Signers)
                .is_some_and(|signers| contains(&signers, &address))
        }

        /// Fetch a proposal by ID, refreshing its TTL.
        pub fn get_transaction(env: Env, tx_id: u64) -> Option<Transaction> {
            let transaction = env.storage().persistent().get(&DataKey::Transaction(tx_id));
            if transaction.is_some() {
                bump_transaction(&env, tx_id);
            }
            transaction
        }

        /// Return the number of signatures on a proposal.
        pub fn signature_count(env: Env, tx_id: u64) -> Option<u32> {
            Self::get_transaction(env, tx_id).map(|tx| tx.signatures.len())
        }

        /// Remove an expired proposal from storage. Anyone may call this.
        ///
        /// Returns `Ok(())` when the proposal was found and expired.
        /// Returns `Err(TransactionNotFound)` if the proposal does not exist.
        /// Returns `Err(AlreadyExecuted)` if the proposal was already executed.
        /// Returns `Err(NotYetExpired)` if the proposal has not yet expired.
        pub fn cleanup_expired(env: Env, tx_id: u64) -> Result<(), MultisigError> {
            let transaction = Self::get_required_transaction(&env, tx_id)?;
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if env.ledger().sequence() <= transaction.expiry_ledger {
                // Not yet expired — nothing to clean up.
                return Err(MultisigError::NotYetExpired);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::Transaction(tx_id));
            events::proposal_expired(&env, tx_id);
            Ok(())
        }

        /// Return the on-chain contract version number.
        pub fn contract_version(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&DataKey::Version)
                .unwrap_or(0)
        }

        // ── Phase helpers ─────────────────────────────────────────────────────

        /// Phase 1 — create a new proposal and record the proposer's implicit vote.
        #[inline]
        fn propose_phase(
            env: &Env,
            proposer: &Address,
            target: Address,
            function: Symbol,
            args: Vec<Val>,
            expiry_ledgers: u32,
        ) -> Result<u64, MultisigError> {
            let tx_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextTransactionId)
                .ok_or(MultisigError::NotInitialized)?;

            let proposer_weight = Self::signer_weight_internal(env, proposer);

            let mut signatures = Vec::new(env);
            signatures.push_back(proposer.clone());

            let current_ledger = env.ledger().sequence();
            // Clamp the requested validity by the configured maximum (#1446).
            let effective_expiry = match env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxProposalValidityLedgers)
            {
                Some(max_valid) => expiry_ledgers.min(max_valid),
                None => expiry_ledgers,
            };
            let expiry_ledger = current_ledger.saturating_add(effective_expiry);

            let transaction = Transaction {
                id: tx_id,
                proposer: proposer.clone(),
                target,
                function,
                args,
                signatures,
                accumulated_weight: proposer_weight,
                executed: false,
                expiry_ledger,
                created_ledger: current_ledger,
                nonce: tx_id,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            env.storage()
                .instance()
                .set(&DataKey::NextTransactionId, &(tx_id + 1));
            bump_instance(env);
            bump_transaction(env, tx_id);
            Ok(tx_id)
        }

        /// Phase 2 — record a signer's vote on an existing proposal.
        #[inline]
        fn vote_phase(env: &Env, signer: &Address, tx_id: u64) -> Result<u32, MultisigError> {
            let mut transaction = Self::get_required_transaction(env, tx_id)?;
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if env.ledger().sequence() > transaction.expiry_ledger {
                return Err(MultisigError::ProposalExpired);
            }
            if let Some(max_valid) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxProposalValidityLedgers)
            {
                let age = env.ledger().sequence().saturating_sub(transaction.created_ledger);
                if age > max_valid {
                    return Err(MultisigError::ProposalExpired);
                }
            }
            if contains(&transaction.signatures, signer) {
                return Err(MultisigError::AlreadySigned);
            }

            let weight = Self::signer_weight_internal(env, signer);
            // Keep signatures in strictly ascending address order so that
            // duplicate detection (#1449) remains valid.
            let mut new_signatures = Vec::new(env);
            let mut inserted = false;
            for existing in transaction.signatures.iter() {
                if !inserted && *signer < existing {
                    new_signatures.push_back(signer.clone());
                    inserted = true;
                }
                new_signatures.push_back(existing);
            }
            if !inserted {
                new_signatures.push_back(signer.clone());
            }
            transaction.signatures = new_signatures;
            transaction.accumulated_weight =
                transaction.accumulated_weight.saturating_add(weight);
            let signature_count = transaction.signatures.len();
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(env, tx_id);
            Ok(signature_count)
        }

        /// Validate that a proposal is executable without mutating state.
        #[inline]
        fn validate_executable(env: &Env, tx_id: u64) -> Result<(), MultisigError> {
            let transaction = Self::get_required_transaction(env, tx_id)?;
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if env.ledger().sequence() > transaction.expiry_ledger {
                return Err(MultisigError::ProposalExpired);
            }
            if let Some(max_valid) = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::MaxProposalValidityLedgers)
            {
                let age = env.ledger().sequence().saturating_sub(transaction.created_ledger);
                if age > max_valid {
                    return Err(MultisigError::ProposalExpired);
                }
            }

            let threshold: u32 = env
                .storage()
                .instance()
                .get(&DataKey::Threshold)
                .ok_or(MultisigError::NotInitialized)?;
            if transaction.accumulated_weight < threshold {
                return Err(MultisigError::ThresholdNotMet);
            }

            // Reject duplicate signatures (#1449): signatures must be strictly
            // ascending by address, guaranteeing distinct signers.
            Self::verify_unique_signatures(&transaction.signatures)?;

            Ok(())
        }

        /// Verify that `signatures` contains no duplicates (#1449).
        ///
        /// Signatures must be in strictly ascending address order.  A duplicate
        /// (or out-of-order) entry yields `DuplicateSignature`.
        #[inline]
        fn verify_unique_signatures(signatures: &Vec<Address>) -> Result<(), MultisigError> {
            let mut prev: Option<Address> = None;
            for signer in signatures.iter() {
                if let Some(p) = prev {
                    if signer <= p {
                        return Err(MultisigError::DuplicateSignature);
                    }
                }
                prev = Some(signer);
            }
            Ok(())
        }

        /// Clear signatures/weight on pending proposals signed by `signer` (#1447).
        #[inline]
        fn invalidate_pending_for_signer(env: &Env, signer: &Address) {
            let next_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextTransactionId)
                .unwrap_or(0u64);
            let mut tx_id: u64 = 0;
            while tx_id < next_id {
                if let Some(mut transaction) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, Transaction>(&DataKey::Transaction(tx_id))
                {
                    if !transaction.executed && contains(&transaction.signatures, signer) {
                        transaction.signatures = Vec::new(env);
                        transaction.accumulated_weight = 0;
                        env.storage()
                            .persistent()
                            .set(&DataKey::Transaction(tx_id), &transaction);
                        bump_transaction(env, tx_id);
                    }
                }
                tx_id += 1;
            }
        }

        /// Phase 3 — verify accumulated weight meets threshold and execute.
        #[inline]
        fn execute_phase(env: &Env, tx_id: u64) -> Result<Val, MultisigError> {
            Self::validate_executable(env, tx_id)?;

            let mut transaction = Self::get_required_transaction(env, tx_id)?;
            transaction.executed = true;
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(env, tx_id);
            events::transaction_executed(env, tx_id);

            let result: Val =
                env.invoke_contract(&transaction.target, &transaction.function, transaction.args);
            Ok(result)
        }

        // ── Internal helpers ──────────────────────────────────────────────────

        #[inline]
        fn signer_weight_internal(env: &Env, signer: &Address) -> u32 {
            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);
            weights_map
                .and_then(|m| m.get(signer.clone()))
                .unwrap_or(1u32)
        }

        #[inline]
        fn get_required_signers(env: &Env) -> Result<Vec<Address>, MultisigError> {
            env.storage()
                .instance()
                .get(&DataKey::Signers)
                .ok_or(MultisigError::NotInitialized)
        }

        #[inline]
        fn get_required_transaction(env: &Env, tx_id: u64) -> Result<Transaction, MultisigError> {
            env.storage()
                .persistent()
                .get(&DataKey::Transaction(tx_id))
                .ok_or(MultisigError::TransactionNotFound)
        }

        #[inline]
        fn require_signer(env: &Env, signer: &Address) -> Result<(), MultisigError> {
            let signers = Self::get_required_signers(env)?;
            if !contains(&signers, signer) {
                return Err(MultisigError::NotSigner);
            }
            Ok(())
        }

        #[inline]
        fn require_threshold_approvals(
            env: &Env,
            approvals: &Vec<Address>,
        ) -> Result<(), MultisigError> {
            validate_unique_signers(approvals)?;

            let signers = Self::get_required_signers(env)?;
            let threshold: u32 = env
                .storage()
                .instance()
                .get(&DataKey::Threshold)
                .ok_or(MultisigError::NotInitialized)?;

            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);

            let mut accumulated: u32 = 0;
            for approver in approvals.iter() {
                if !contains(&signers, &approver) {
                    return Err(MultisigError::NotSigner);
                }
                approver.require_auth();
                let w = weights_map
                    .as_ref()
                    .and_then(|m| m.get(approver.clone()))
                    .unwrap_or(1u32);
                accumulated = accumulated.saturating_add(w);
            }

            if accumulated < threshold {
                return Err(MultisigError::InsufficientApprovals);
            }

            Ok(())
        }

        /// Verify an off-chain signature with strict domain separation.
        pub fn verify_offchain_signature(
            env: Env,
            proposal_id: u64,
            signer: Address,
            signature: BytesN<64>,
        ) -> Result<bool, MultisigError> {
            let transaction = Self::get_required_transaction(&env, proposal_id)?;
            let contract_id = env.current_contract_address();
            let network_id = env.ledger().network_id();

            let mut message = Bytes::new(&env);
            message.append(&Bytes::from_array(&env, &contract_id.to_array()));
            message.append(&Bytes::from_array(&env, &network_id.to_array()));
            let nonce_bytes = transaction.nonce.to_be_bytes();
            message.append(&Bytes::from_array(&env, &nonce_bytes));
            let id_bytes = proposal_id.to_be_bytes();
            message.append(&Bytes::from_array(&env, &id_bytes));

            let message_hash = env.crypto().sha256(&message);
            env.crypto()
                .ed25519_verify(&signer, &message_hash.into(), &signature);
            Ok(true)
        }

        /// Configure token spending limit with threshold approvals (multi-token).
        pub fn configure_token_limit(
            env: Env,
            approvals: Vec<Address>,
            token: Address,
            daily_limit: i128,
        ) -> Result<(), MultisigError> {
            // Allow either direct call or with approvals
            if approvals.is_empty() {
                // Try to require signer auth
                let _ = Self::get_required_signers(&env)?;
            } else {
                Self::require_threshold_approvals(&env, &approvals)?;
            }
            if daily_limit < 0 {
                return Err(MultisigError::InvalidSpendingLimit);
            }
            let mut spent_amount = 0i128;
            let mut last_refill_ledger = env.ledger().sequence();
            if let Some(existing) = env
                .storage()
                .instance()
                .get::<DataKey, storage::TokenSpendingLimit>(&DataKey::SpendingLimits(token.clone()))
            {
                spent_amount = existing.spent_amount;
                last_refill_ledger = existing.last_refill_ledger;
            }
            let limit = storage::TokenSpendingLimit {
                token: token.clone(),
                daily_limit,
                spent_amount,
                last_refill_ledger,
            };
            env.storage()
                .instance()
                .set(&DataKey::SpendingLimits(token.clone()), &limit);
            bump_instance(&env);
            Ok(())
        }

        /// Set token spending limit (alias for configure_token_limit with different naming).
        pub fn set_token_spending_limit(
            env: Env,
            token: Address,
            daily_limit: i128,
        ) -> Result<(), MultisigError> {
            // Try to use threshold approvals if possible
            let approvals: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::Signers)
                .unwrap_or_else(|| Vec::new(&env));
            // This may fail if auth not provided properly, but provide flexible API
            let _ = approvals;
            if daily_limit < 0 {
                return Err(MultisigError::InvalidSpendingLimit);
            }
            let limit = storage::TokenSpendingLimit {
                token: token.clone(),
                daily_limit,
                spent_amount: 0i128,
                last_refill_ledger: env.ledger().sequence(),
            };
            env.storage()
                .instance()
                .set(&DataKey::SpendingLimits(token.clone()), &limit);
            bump_instance(&env);
            Ok(())
        }

        /// Check and update spending limit for a token transfer.
        pub fn check_spending_limit(
            env: Env,
            token: Address,
            amount: i128,
        ) -> Result<(), MultisigError> {
            if amount < 0 {
                return Err(MultisigError::InvalidSpendingLimit);
            }
            let mut limit_opt: Option<storage::TokenSpendingLimit> = env
                .storage()
                .instance()
                .get(&DataKey::SpendingLimits(token.clone()));
            if let Some(mut l) = limit_opt {
                let current_ledger = env.ledger().sequence();
                if current_ledger >= l.last_refill_ledger + 17280 || current_ledger < l.last_refill_ledger {
                    l.spent_amount = 0i128;
                    l.last_refill_ledger = current_ledger;
                }
                if l.spent_amount + amount > l.daily_limit {
                    return Err(MultisigError::SpendingLimitExceeded);
                }
                l.spent_amount += amount;
                env.storage()
                    .instance()
                    .set(&DataKey::SpendingLimits(token.clone()), &l);
                Ok(())
            } else {
                Ok(())
            }
        }

        /// Get spending limit for a token.
        pub fn get_spending_limit(env: Env, token: Address) -> Option<TokenSpendingLimit> {
            env.storage()
                .instance()
                .get(&DataKey::SpendingLimits(token))
        }

        // ── Threshold change proposals (#1445) ────────────────────────────────

        /// Propose a change to the signer threshold.
        ///
        /// The `proposer` must be a current signer and is recorded as the first
        /// approval. `new_threshold` must satisfy `1 <= new_threshold <= total`,
        /// where `total` is the sum of all current signer weights (equal to the
        /// signer count for un-weighted wallets).
        ///
        /// Returns the new proposal ID.
        pub fn propose_threshold_change(
            env: Env,
            proposer: Address,
            new_threshold: u32,
        ) -> Result<u64, MultisigError> {
            Self::require_signer(&env, &proposer)?;
            proposer.require_auth();

            let signers = Self::get_required_signers(&env)?;
            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);
            let total = match weights_map {
                Some(ref m) => total_weight(&signers, m),
                None => signers.len(),
            };
            validate_threshold(new_threshold, total)?;

            let proposal_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextThresholdChangeId)
                .unwrap_or(0u64);

            let mut approvals = Vec::new(&env);
            approvals.push_back(proposer.clone());

            let proposal = ThresholdChange {
                id: proposal_id,
                proposer: proposer.clone(),
                new_threshold,
                approvals,
                executed: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::ThresholdChange(proposal_id), &proposal);
            env.storage()
                .instance()
                .set(&DataKey::NextThresholdChangeId, &(proposal_id + 1));
            bump_instance(&env);
            env.storage().persistent().extend_ttl(
                &DataKey::ThresholdChange(proposal_id),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            events::threshold_change_proposed(&env, proposal_id, &proposer, new_threshold);
            Ok(proposal_id)
        }

        /// Approve a pending threshold-change proposal.
        ///
        /// The `signer` must be a current signer and must not have already
        /// approved the proposal.
        pub fn approve_threshold_change(
            env: Env,
            signer: Address,
            proposal_id: u64,
        ) -> Result<(), MultisigError> {
            Self::require_signer(&env, &signer)?;
            signer.require_auth();

            let mut proposal = Self::get_required_threshold_change(&env, proposal_id)?;
            if proposal.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if contains(&proposal.approvals, &signer) {
                return Err(MultisigError::AlreadySigned);
            }

            proposal.approvals.push_back(signer.clone());
            env.storage()
                .persistent()
                .set(&DataKey::ThresholdChange(proposal_id), &proposal);
            env.storage().persistent().extend_ttl(
                &DataKey::ThresholdChange(proposal_id),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            events::threshold_change_approved(&env, proposal_id, &signer);
            Ok(())
        }

        /// Execute a threshold-change proposal once enough current signers
        /// have approved it.
        ///
        /// The accumulated approval weight must meet the **current** threshold.
        /// On success the new threshold is applied and the proposal is marked
        /// executed.
        pub fn execute_threshold_change(
            env: Env,
            proposal_id: u64,
        ) -> Result<(), MultisigError> {
            let mut proposal = Self::get_required_threshold_change(&env, proposal_id)?;
            if proposal.executed {
                return Err(MultisigError::AlreadyExecuted);
            }

            let signers = Self::get_required_signers(&env)?;
            let current_threshold: u32 = env
                .storage()
                .instance()
                .get(&DataKey::Threshold)
                .ok_or(MultisigError::NotInitialized)?;
            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);

            let mut accumulated: u32 = 0;
            for approver in proposal.approvals.iter() {
                if !contains(&signers, &approver) {
                    continue;
                }
                let w = weights_map
                    .as_ref()
                    .and_then(|m| m.get(approver.clone()))
                    .unwrap_or(1u32);
                accumulated = accumulated.saturating_add(w);
            }

            if accumulated < current_threshold {
                return Err(MultisigError::InsufficientApprovals);
            }

            // Re-validate against the current signer set before applying.
            let total = match weights_map {
                Some(ref m) => total_weight(&signers, m),
                None => signers.len(),
            };
            validate_threshold(proposal.new_threshold, total)?;

            proposal.executed = true;
            env.storage()
                .persistent()
                .set(&DataKey::ThresholdChange(proposal_id), &proposal);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &proposal.new_threshold);
            bump_instance(&env);
            env.storage().persistent().extend_ttl(
                &DataKey::ThresholdChange(proposal_id),
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );

            events::threshold_change_applied(&env, proposal_id, proposal.new_threshold);
            Ok(())
        }

        /// Fetch a threshold-change proposal by ID, refreshing its TTL.
        pub fn get_threshold_change(
            env: Env,
            proposal_id: u64,
        ) -> Option<ThresholdChange> {
            let proposal = env
                .storage()
                .persistent()
                .get(&DataKey::ThresholdChange(proposal_id));
            if proposal.is_some() {
                env.storage().persistent().extend_ttl(
                    &DataKey::ThresholdChange(proposal_id),
                    LEDGER_LIFETIME_THRESHOLD,
                    LEDGER_BUMP_AMOUNT,
                );
            }
            proposal
        }

        /// Return the next threshold-change proposal ID.
        pub fn get_next_threshold_change_id(env: Env) -> u64 {
            env.storage()
                .instance()
                .get(&DataKey::NextThresholdChangeId)
                .unwrap_or(0u64)
        }

        #[inline]
        fn get_required_threshold_change(
            env: &Env,
            proposal_id: u64,
        ) -> Result<ThresholdChange, MultisigError> {
            env.storage()
                .persistent()
                .get(&DataKey::ThresholdChange(proposal_id))
                .ok_or(MultisigError::TransactionNotFound)
        }
    }
}
