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
//!
//! ## Reentrancy guard (#1114)
//!
//! A global lock is held in instance storage for the duration of every
//! external contract call made by the wallet (`execute_transaction`,
//! `execute_batch`, `spend_allowance`). While the lock is held every
//! state-changing entry point rejects with `Reentrant`, so an invoked contract
//! cannot execute other proposals, spend the allowance, or alter the signer
//! set / configuration before the first dispatch returns.
//!
//! ## Daily spending allowance (#1115)
//!
//! `set_spending_limit` (full threshold approval) designates a spending
//! operator, a token and a `daily_limit`. The operator may then call
//! `spend_allowance` to transfer up to `daily_limit` tokens per
//! [`SPENDING_WINDOW_LEDGERS`] window without collecting signatures. The
//! number of spends per window is additionally capped via
//! `soroban_common::check_and_record`.
//!
//! ## Timelock (#1116)
//!
//! `set_timelock_delay` (full threshold approval) configures a delay in
//! ledgers. When a proposal reaches threshold it is *queued*; it can only be
//! executed once `queued_ledger + timelock_delay` has passed. During the
//! delay any current signer may `cancel_transaction`. A delay of `0` (the
//! default) preserves the original immediate-execution behaviour.
//!
//! ## Proposal enumeration (#1117)
//!
//! `get_transactions(cursor, limit, status, descending)` pages through
//! proposals using `soroban_common::paginate`, optionally filtering by
//! [`TxStatus`], and refreshes the TTL of every record it reads.
//! ## Cancellation and revocation (#1113)
//!
//! The original proposer may `cancel_proposal` a pending transaction, and any
//! signer may `revoke_signature` to withdraw their vote. Both are rejected
//! once the transaction has executed.
//!
//! ## Signer management (#1111, #1112)
//!
//! `add_signer` takes a custom weight and `update_signer_weight` changes an
//! existing signer's weight; both require synchronous threshold approval.
//! Signer additions, removals, weight updates, and threshold changes can
//! also be proposed asynchronously with `propose_signer_change`, signed over
//! multiple ledgers with `sign_signer_change`, and applied with
//! `execute_signer_change` once the threshold weight is reached.

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, Map, Symbol, Val, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod test;

pub use errors::MultisigError;
pub use storage::{
    DataKey, SignerWeight, SpendingConfig, Transaction, TransactionPage, TxStatus,
};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, check_and_record, paginate};

/// Length of the spending-allowance window in ledgers (~24h at 5s/ledger).
pub const SPENDING_WINDOW_LEDGERS: u32 = 17_280;

/// Maximum number of `spend_allowance` calls per window.
pub const MAX_SPENDS_PER_WINDOW: u32 = 24;
pub use storage::{DataKey, SignerChange, SignerProposal, SignerWeight, Transaction};

/// Maximum page size accepted by `get_transactions`.
pub const MAX_PAGE_SIZE: u32 = 50;

/// `check_and_record` namespace for allowance spends.
const SPEND_RATE_NAMESPACE: u32 = 1_115;

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
fn is_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::ReentrancyLock)
        .unwrap_or(false)
}

/// Reject the call if an external invocation is currently in progress.
#[inline]
fn require_unlocked(env: &Env) -> Result<(), MultisigError> {
    if is_locked(env) {
        return Err(MultisigError::Reentrant);
    }
    Ok(())
}

#[inline]
fn set_lock(env: &Env, locked: bool) {
    if locked {
        env.storage().instance().set(&DataKey::ReentrancyLock, &true);
    } else {
        env.storage().instance().remove(&DataKey::ReentrancyLock);
    }
}

#[inline]
fn timelock_delay(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TimelockDelay)
        .unwrap_or(0)
}

/// Derive the lifecycle status of a proposal at the current ledger.
fn status_of(env: &Env, tx: &Transaction) -> TxStatus {
    if tx.executed {
        TxStatus::Executed
    } else if tx.cancelled {
        TxStatus::Cancelled
    } else if env.ledger().sequence() > tx.expiry_ledger {
        TxStatus::Expired
    } else if tx.queued_ledger.is_some() {
        TxStatus::Queued
    } else {
        TxStatus::Pending
    }
fn bump_signer_proposal(env: &Env, proposal_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::SignerProposal(proposal_id),
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
            bump_instance(&env);

            events::initialized(&env, threshold, signers.len());
            Ok(())
        }

        /// Add a signer with a custom `weight` and set the new threshold (#1111).
        ///
        /// `weight` must be ≥ 1 and `new_threshold` must not exceed the new
        /// total signer weight. Requires synchronous threshold approval via
        /// `approvals`; see `propose_signer_change` for the asynchronous flow.
        pub fn add_signer(
            env: Env,
            approvals: Vec<Address>,
            signer: Address,
            weight: u32,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            require_unlocked(&env)?;
            let mut signers = Self::get_required_signers(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;
            Self::apply_add_signer(&env, &signer, weight, new_threshold)
        }

        /// Remove a signer and optionally adjust the threshold.
        pub fn remove_signer(
            env: Env,
            approvals: Vec<Address>,
            signer: Address,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            Self::require_threshold_approvals(&env, &approvals)?;
            Self::apply_remove_signer(&env, &signer, new_threshold)
        }

        /// Update an existing signer's weight (#1111).
        ///
        /// `new_weight` must be ≥ 1, and the current threshold must still be
        /// reachable (≤ total weight) after the change. Requires synchronous
        /// threshold approval via `approvals`.
        pub fn update_signer_weight(
            env: Env,
            approvals: Vec<Address>,
            signer: Address,
            new_weight: u32,
        ) -> Result<(), MultisigError> {
            Self::require_threshold_approvals(&env, &approvals)?;
            Self::apply_update_signer_weight(&env, &signer, new_weight)
        }

        // ── Asynchronous signer management (#1112) ────────────────────────────

        /// Propose a signer-set, weight, or threshold change to be approved
        /// asynchronously. The proposer signs it automatically.
        ///
        /// Other signers approve over subsequent ledgers with
        /// `sign_signer_change`; once the accumulated weight reaches the
        /// threshold anyone may call `execute_signer_change`.
        pub fn propose_signer_change(
            env: Env,
            proposer: Address,
            change: SignerChange,
            expiry_ledgers: u32,
        ) -> Result<u64, MultisigError> {
            Self::require_signer(&env, &proposer)?;
            proposer.require_auth();

            let proposal_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextSignerProposalId)
                .unwrap_or(0);

            let mut signatures = Vec::new(&env);
            signatures.push_back(proposer.clone());

            let proposal = SignerProposal {
                id: proposal_id,
                proposer: proposer.clone(),
                change,
                signatures,
                accumulated_weight: Self::signer_weight_internal(&env, &proposer),
                executed: false,
                expiry_ledger: env.ledger().sequence().saturating_add(expiry_ledgers),
            };

            env.storage()
                .persistent()
                .set(&DataKey::SignerProposal(proposal_id), &proposal);
            env.storage()
                .instance()
                .set(&DataKey::NextSignerProposalId, &(proposal_id + 1));
            bump_instance(&env);
            bump_signer_proposal(&env, proposal_id);

            events::signer_change_proposed(&env, proposal_id, &proposer);
            Ok(proposal_id)
        }

        /// Sign a pending signer-management proposal.
        pub fn sign_signer_change(
            env: Env,
            signer: Address,
            proposal_id: u64,
        ) -> Result<(), MultisigError> {
            require_unlocked(&env)?;
            let signers = Self::get_required_signers(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;
            Self::require_signer(&env, &signer)?;
            signer.require_auth();

            let mut proposal = Self::get_required_signer_proposal(&env, proposal_id)?;
            if proposal.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if env.ledger().sequence() > proposal.expiry_ledger {
                return Err(MultisigError::ProposalExpired);
            }
            if contains(&proposal.signatures, &signer) {
                return Err(MultisigError::AlreadySigned);
            }

            proposal.signatures.push_back(signer.clone());
            proposal.accumulated_weight = proposal
                .accumulated_weight
                .saturating_add(Self::signer_weight_internal(&env, &signer));
            let signature_count = proposal.signatures.len();

            env.storage()
                .persistent()
                .set(&DataKey::SignerProposal(proposal_id), &proposal);
            bump_signer_proposal(&env, proposal_id);

            events::signer_change_signed(&env, proposal_id, &signer, signature_count);
            Ok(())
        }

        /// Apply a signer-management proposal once its accumulated weight
        /// meets the current threshold. Anyone may call this.
        pub fn execute_signer_change(env: Env, proposal_id: u64) -> Result<(), MultisigError> {
            let mut proposal = Self::get_required_signer_proposal(&env, proposal_id)?;
            if proposal.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if env.ledger().sequence() > proposal.expiry_ledger {
                return Err(MultisigError::ProposalExpired);
            }

            let threshold = Self::get_required_threshold(&env)?;
            if proposal.accumulated_weight < threshold {
                return Err(MultisigError::ThresholdNotMet);
            }

            proposal.executed = true;
            env.storage()
                .persistent()
                .set(&DataKey::SignerProposal(proposal_id), &proposal);
            bump_signer_proposal(&env, proposal_id);

            match proposal.change {
                SignerChange::AddSigner(sw, new_threshold) => {
                    Self::apply_add_signer(&env, &sw.signer, sw.weight, new_threshold)?
                }
                SignerChange::RemoveSigner(signer, new_threshold) => {
                    Self::apply_remove_signer(&env, &signer, new_threshold)?
                }
                SignerChange::UpdateSignerWeight(sw) => {
                    Self::apply_update_signer_weight(&env, &sw.signer, sw.weight)?
                }
                SignerChange::ChangeThreshold(new_threshold) => {
                    Self::apply_change_threshold(&env, new_threshold)?
                }
            }

            events::signer_change_executed(&env, proposal_id);
            Ok(())
        }

        /// Fetch a signer-management proposal by ID, refreshing its TTL.
        pub fn get_signer_proposal(env: Env, proposal_id: u64) -> Option<SignerProposal> {
            let proposal = env
                .storage()
                .persistent()
                .get(&DataKey::SignerProposal(proposal_id));
            if proposal.is_some() {
                bump_signer_proposal(&env, proposal_id);
            }
            proposal
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
            require_unlocked(&env)?;
            Self::require_signer(&env, &proposer)?;
            proposer.require_auth();

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
            require_unlocked(&env)?;
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

        /// Cancel a pending transaction (#1113).
        ///
        /// Only the original proposer may cancel, and only before execution.
        /// The proposal is removed from storage.
        pub fn cancel_proposal(
            env: Env,
            proposer: Address,
            tx_id: u64,
        ) -> Result<(), MultisigError> {
            proposer.require_auth();

            let transaction = Self::get_required_transaction(&env, tx_id)?;
            if transaction.proposer != proposer {
                return Err(MultisigError::NotProposer);
            }
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }

            env.storage()
                .persistent()
                .remove(&DataKey::Transaction(tx_id));
            events::transaction_cancelled(&env, tx_id, &proposer);
            Ok(())
        }

        /// Withdraw a previously recorded signature from a pending transaction (#1113).
        ///
        /// Rejected once the transaction has executed. `accumulated_weight` is
        /// recomputed from the remaining signatures using current signer
        /// weights, so a weight change between signing and revoking can never
        /// leave stale weight on the proposal.
        pub fn revoke_signature(
            env: Env,
            signer: Address,
            tx_id: u64,
        ) -> Result<(), MultisigError> {
            signer.require_auth();

            let mut transaction = Self::get_required_transaction(&env, tx_id)?;
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }

            let mut remaining = Vec::new(&env);
            let mut found = false;
            for existing in transaction.signatures.iter() {
                if existing == signer {
                    found = true;
                } else {
                    remaining.push_back(existing);
                }
            }
            if !found {
                return Err(MultisigError::NotSigned);
            }

            let mut accumulated: u32 = 0;
            for s in remaining.iter() {
                accumulated = accumulated.saturating_add(Self::signer_weight_internal(&env, &s));
            }
            transaction.signatures = remaining;
            transaction.accumulated_weight = accumulated;
            let signature_count = transaction.signatures.len();

            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(&env, tx_id);

            events::signature_revoked(&env, tx_id, &signer, signature_count, accumulated);
            Ok(())
        }

        /// Execute multiple already-approved proposals in a single transaction (#826).
        ///
        /// ## Semantics
        ///
        /// Each proposal ID is validated and attempted **independently**.  A
        /// failure for one proposal does **not** abort the others:
        ///
        /// - If a proposal does not exist (`TransactionNotFound`) it is silently
        ///   skipped.
        /// - If a proposal is already executed (`AlreadyExecuted`), expired
        ///   (`ProposalExpired`), or has insufficient weight (`ThresholdNotMet`)
        ///   it is silently skipped.
        ///
        /// The caller should diff the returned `Vec<u64>` against the input to
        /// identify which proposals were skipped.  Individual proposal state can
        /// be inspected via `get_transaction`.
        ///
        /// ## Returns
        ///
        /// `Vec<u64>` — IDs of proposals that were successfully executed during
        /// this call.  A `batch_executed` event is emitted with the executed IDs
        /// and the count of skipped proposals.
        ///
        /// Panics with `Reentrant` if called while an external invocation is
        /// in progress, rather than silently skipping every proposal.
        pub fn execute_batch(env: Env, proposal_ids: Vec<u64>) -> Vec<u64> {
            if is_locked(&env) {
                soroban_sdk::panic_with_error!(&env, MultisigError::Reentrant);
            }
            let mut executed_ids: Vec<u64> = Vec::new(&env);
            let mut skipped_count: u32 = 0;

            for tx_id in proposal_ids.iter() {
                match Self::execute_phase(&env, tx_id) {
                    Ok(_) => {
                        executed_ids.push_back(tx_id);
                    }
                    Err(_) => {
                        skipped_count = skipped_count.saturating_add(1);
                    }
                }
            }

            events::batch_executed(&env, &executed_ids, skipped_count);
            executed_ids
        }

        /// Queue a proposal whose accumulated weight meets the threshold but
        /// which was not queued automatically (e.g. because the threshold was
        /// lowered after the last signature). Anyone may call this (#1116).
        pub fn queue_transaction(env: Env, tx_id: u64) -> Result<u32, MultisigError> {
            require_unlocked(&env)?;
            let mut transaction = Self::get_required_transaction(&env, tx_id)?;
            Self::require_open(&env, &transaction)?;
            if let Some(q) = transaction.queued_ledger {
                return Ok(q.saturating_add(timelock_delay(&env)));
            }
            if transaction.accumulated_weight < Self::threshold(&env)? {
                return Err(MultisigError::ThresholdNotMet);
            }
            let executable_at = Self::queue(&env, &mut transaction);
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(&env, tx_id);
            Ok(executable_at)
        }

        /// Cancel a pending or queued proposal (#1116).
        ///
        /// Any current signer may cancel. This gives honest signers a veto
        /// during the timelock window if a quorum of keys is compromised and
        /// used to queue a malicious transaction.
        pub fn cancel_transaction(
            env: Env,
            signer: Address,
            tx_id: u64,
        ) -> Result<(), MultisigError> {
            require_unlocked(&env)?;
            Self::require_signer(&env, &signer)?;
            signer.require_auth();

            let mut transaction = Self::get_required_transaction(&env, tx_id)?;
            Self::require_open(&env, &transaction)?;

            transaction.cancelled = true;
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(&env, tx_id);
            events::transaction_cancelled(&env, tx_id, &signer);
            Ok(())
        }

        /// Set the timelock delay in ledgers. Requires full threshold
        /// approval (#1116). `0` disables the timelock.
        pub fn set_timelock_delay(
            env: Env,
            approvals: Vec<Address>,
            delay: u32,
        ) -> Result<(), MultisigError> {
            require_unlocked(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;
            env.storage().instance().set(&DataKey::TimelockDelay, &delay);
            bump_instance(&env);
            events::timelock_updated(&env, delay);
            Ok(())
        }

        /// Return the configured timelock delay in ledgers (0 when disabled).
        pub fn get_timelock_delay(env: Env) -> u32 {
            timelock_delay(&env)
        }

        /// Configure the daily spending allowance. Requires full threshold
        /// approval (#1115). A `daily_limit` of `0` disables spending.
        ///
        /// Changing the configuration resets the current spending window.
        pub fn set_spending_limit(
            env: Env,
            approvals: Vec<Address>,
            operator: Address,
            token: Address,
            daily_limit: i128,
        ) -> Result<(), MultisigError> {
            require_unlocked(&env)?;
            Self::require_threshold_approvals(&env, &approvals)?;
            if daily_limit < 0 {
                return Err(MultisigError::InvalidAmount);
            }
            let config = SpendingConfig {
                operator: operator.clone(),
                token: token.clone(),
                daily_limit,
            };
            env.storage()
                .instance()
                .set(&DataKey::SpendingConfig, &config);
            env.storage().instance().remove(&DataKey::SpendingWindow);
            bump_instance(&env);
            events::spending_limit_updated(&env, &operator, &token, daily_limit);
            Ok(())
        }

        /// Return the spending allowance configuration, if any.
        pub fn get_spending_config(env: Env) -> Option<SpendingConfig> {
            env.storage().instance().get(&DataKey::SpendingConfig)
        }

        /// Return the amount still spendable in the current window.
        pub fn remaining_allowance(env: Env) -> i128 {
            let Some(config) = Self::get_spending_config(env.clone()) else {
                return 0;
            };
            let (_, spent) = Self::current_window(&env);
            config.daily_limit.saturating_sub(spent).max(0)
        }

        /// Transfer `amount` of the configured token to `to` without
        /// threshold approval, provided the operator stays within the daily
        /// allowance and the per-window call cap (#1115).
        ///
        /// Returns the total spent in the current window after this transfer.
        pub fn spend_allowance(
            env: Env,
            operator: Address,
            to: Address,
            amount: i128,
        ) -> Result<i128, MultisigError> {
            require_unlocked(&env)?;
            let config: SpendingConfig = env
                .storage()
                .instance()
                .get(&DataKey::SpendingConfig)
                .ok_or(MultisigError::SpendingNotConfigured)?;
            if operator != config.operator {
                return Err(MultisigError::NotSpendingOperator);
            }
            operator.require_auth();
            if amount <= 0 {
                return Err(MultisigError::InvalidAmount);
            }

            let (window_start, spent) = Self::current_window(&env);
            let new_spent = spent
                .checked_add(amount)
                .ok_or(MultisigError::DailyLimitExceeded)?;
            if new_spent > config.daily_limit {
                return Err(MultisigError::DailyLimitExceeded);
            }
            if !check_and_record(
                &env,
                SPEND_RATE_NAMESPACE,
                &operator,
                SPENDING_WINDOW_LEDGERS,
                MAX_SPENDS_PER_WINDOW,
            ) {
                return Err(MultisigError::RateLimited);
            }

            env.storage()
                .instance()
                .set(&DataKey::SpendingWindow, &(window_start, new_spent));
            bump_instance(&env);

            set_lock(&env, true);
            token::Client::new(&env, &config.token).transfer(
                &env.current_contract_address(),
                &to,
                &amount,
            );
            set_lock(&env, false);

            events::allowance_spent(&env, &operator, &to, amount, new_spent);
            Ok(new_spent)
        }

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
            weights_map.and_then(|m| m.get(signer)).unwrap_or(1u32)
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

        /// Return the lifecycle status of a proposal, refreshing its TTL.
        pub fn get_transaction_status(env: Env, tx_id: u64) -> Option<TxStatus> {
            Self::get_transaction(env.clone(), tx_id).map(|tx| status_of(&env, &tx))
        }

        /// Page through proposals (#1117).
        ///
        /// - `cursor` — first proposal ID to visit. When `descending` is true
        ///   a cursor beyond the newest ID starts at the newest proposal, so
        ///   `u64::MAX` means "latest first".
        /// - `limit` — clamped to `[1, MAX_PAGE_SIZE]`.
        /// - `status` — when `Some`, only proposals in that status are
        ///   returned; the scan still advances over non-matching IDs.
        /// - `descending` — scan from newer to older IDs.
        ///
        /// Pass the returned `next_cursor` back to fetch the following page;
        /// `None` means the scan is exhausted. Every proposal record read is
        /// TTL-bumped. Removed (cleaned-up) proposals are skipped.
        pub fn get_transactions(
            env: Env,
            cursor: u64,
            limit: u32,
            status: Option<TxStatus>,
            descending: bool,
        ) -> TransactionPage {
            let next_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextTransactionId)
                .unwrap_or(0);

            let start = if descending {
                if next_id == 0 {
                    None
                } else {
                    Some(cursor.min(next_id.saturating_sub(1)))
                }
            } else if cursor < next_id {
                Some(cursor)
            } else {
                None
            };

            let Some(start) = start else {
                return TransactionPage {
                    items: Vec::new(&env),
                    next_cursor: None,
                };
            };

            let page = paginate(
                &env,
                start,
                limit,
                MAX_PAGE_SIZE,
                |c: u64| {
                    if descending {
                        c.checked_sub(1)
                    } else {
                        c.checked_add(1).filter(|n| *n < next_id)
                    }
                },
                |c: u64| {
                    let tx: Transaction =
                        env.storage().persistent().get(&DataKey::Transaction(c))?;
                    bump_transaction(&env, c);
                    match status {
                        Some(wanted) if status_of(&env, &tx) != wanted => None,
                        _ => Some(tx),
                    }
                },
            );

            TransactionPage {
                items: page.items,
                next_cursor: page.next_cursor,
            }
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
            require_unlocked(&env)?;
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
            env.storage().instance().get(&DataKey::Version).unwrap_or(0)
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

            let expiry_ledger = env.ledger().sequence().saturating_add(expiry_ledgers);

            let mut transaction = Transaction {
                id: tx_id,
                proposer: proposer.clone(),
                target,
                function,
                args,
                signatures,
                accumulated_weight: proposer_weight,
                executed: false,
                expiry_ledger,
                queued_ledger: None,
                cancelled: false,
            };
            if proposer_weight >= Self::threshold(env)? {
                Self::queue(env, &mut transaction);
            }

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
            Self::require_open(env, &transaction)?;
            if contains(&transaction.signatures, signer) {
                return Err(MultisigError::AlreadySigned);
            }

            let weight = Self::signer_weight_internal(env, signer);
            transaction.signatures.push_back(signer.clone());
            transaction.accumulated_weight = transaction.accumulated_weight.saturating_add(weight);
            let signature_count = transaction.signatures.len();
            if transaction.queued_ledger.is_none()
                && transaction.accumulated_weight >= Self::threshold(env)?
            {
                Self::queue(env, &mut transaction);
            }
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(env, tx_id);
            Ok(signature_count)
        }

        /// Phase 3 — verify accumulated weight meets threshold and execute.
        #[inline]
        ///
        /// A global reentrancy lock is held across the external invocation so
        /// the target cannot re-enter any state-changing entry point (#1114).
        /// When a timelock is configured the proposal must have been queued
        /// and its delay must have elapsed (#1116).
        fn execute_phase(env: &Env, tx_id: u64) -> Result<Val, MultisigError> {
            require_unlocked(env)?;
            let mut transaction = Self::get_required_transaction(env, tx_id)?;
            Self::require_open(env, &transaction)?;

            if transaction.accumulated_weight < Self::threshold(env)? {
                return Err(MultisigError::ThresholdNotMet);
            }

            let delay = timelock_delay(env);
            if delay > 0 {
                let queued = transaction.queued_ledger.ok_or(MultisigError::NotQueued)?;
                if env.ledger().sequence() < queued.saturating_add(delay) {
                    return Err(MultisigError::TimelockNotElapsed);
                }
            }

            transaction.executed = true;
            env.storage()
                .persistent()
                .set(&DataKey::Transaction(tx_id), &transaction);
            bump_transaction(env, tx_id);
            events::transaction_executed(env, tx_id);

            set_lock(env, true);
            let result: Val =
                env.invoke_contract(&transaction.target, &transaction.function, transaction.args);
            set_lock(env, false);
            Ok(result)
        }

        // ── Internal helpers ──────────────────────────────────────────────────

        #[inline]
        fn threshold(env: &Env) -> Result<u32, MultisigError> {
            env.storage()
                .instance()
                .get(&DataKey::Threshold)
                .ok_or(MultisigError::NotInitialized)
        }

        /// Reject proposals that are executed, cancelled or expired.
        #[inline]
        fn require_open(env: &Env, transaction: &Transaction) -> Result<(), MultisigError> {
            if transaction.executed {
                return Err(MultisigError::AlreadyExecuted);
            }
            if transaction.cancelled {
                return Err(MultisigError::TransactionCancelled);
            }
            if env.ledger().sequence() > transaction.expiry_ledger {
                return Err(MultisigError::ProposalExpired);
            }
            Ok(())
        }

        /// Mark `transaction` as queued at the current ledger and emit the
        /// `queued` event. Returns the first ledger at which it is executable.
        fn queue(env: &Env, transaction: &mut Transaction) -> u32 {
            let now = env.ledger().sequence();
            transaction.queued_ledger = Some(now);
            let executable_at = now.saturating_add(timelock_delay(env));
            events::transaction_queued(env, transaction.id, executable_at);
            executable_at
        }

        /// Current spending window `(start, spent)`, rolled over when the
        /// window has elapsed.
        fn current_window(env: &Env) -> (u32, i128) {
            let now = env.ledger().sequence();
            let window: Option<(u32, i128)> =
                env.storage().instance().get(&DataKey::SpendingWindow);
            match window {
                Some((start, spent)) if now < start.saturating_add(SPENDING_WINDOW_LEDGERS) => {
                    (start, spent)
                }
                _ => (now, 0),
            }
        }

        #[inline]
        fn signer_weight_internal(env: &Env, signer: &Address) -> u32 {
            let weights_map: Option<Map<Address, u32>> =
                env.storage().instance().get(&DataKey::Weights);
            weights_map
                .and_then(|m| m.get(signer.clone()))
                .unwrap_or(1u32)
        }

        /// Shared by `add_signer` and `execute_signer_change`.
        fn apply_add_signer(
            env: &Env,
            signer: &Address,
            weight: u32,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            if weight == 0 {
                return Err(MultisigError::InvalidWeight);
            }

            let mut signers = Self::get_required_signers(env)?;
            if contains(&signers, signer) {
                return Err(MultisigError::InvalidSigners);
            }
            signers.push_back(signer.clone());

            let mut weights_map = Self::get_weights_map(env);
            weights_map.set(signer.clone(), weight);

            let tw = total_weight(&signers, &weights_map);
            validate_threshold(new_threshold, tw)?;

            env.storage().instance().set(&DataKey::Signers, &signers);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &new_threshold);
            bump_instance(env);

            events::signer_added(env, signer, weight, new_threshold);
            Ok(())
        }

        /// Shared by `remove_signer` and `execute_signer_change`.
        fn apply_remove_signer(
            env: &Env,
            signer: &Address,
            new_threshold: u32,
        ) -> Result<(), MultisigError> {
            let signers = Self::get_required_signers(env)?;
            if !contains(&signers, signer) {
                return Err(MultisigError::NotSigner);
            }

            let mut remaining = Vec::new(env);
            for existing in signers.iter() {
                if existing != *signer {
                    remaining.push_back(existing);
                }
            }

            validate_unique_signers(&remaining)?;

            let mut weights_map = Self::get_weights_map(env);
            weights_map.remove(signer.clone());

            let tw = total_weight(&remaining, &weights_map);
            validate_threshold(new_threshold, tw)?;

            env.storage().instance().set(&DataKey::Signers, &remaining);
            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &new_threshold);
            bump_instance(env);

            events::signer_removed(env, signer, new_threshold);
            Ok(())
        }

        /// Shared by `update_signer_weight` and `execute_signer_change`.
        fn apply_update_signer_weight(
            env: &Env,
            signer: &Address,
            new_weight: u32,
        ) -> Result<(), MultisigError> {
            if new_weight == 0 {
                return Err(MultisigError::InvalidWeight);
            }

            let signers = Self::get_required_signers(env)?;
            if !contains(&signers, signer) {
                return Err(MultisigError::NotSigner);
            }

            let mut weights_map = Self::get_weights_map(env);
            let old_weight = storage::get_weight(&weights_map, signer);
            weights_map.set(signer.clone(), new_weight);

            // Revalidate the current threshold against the new total weight.
            let threshold = Self::get_required_threshold(env)?;
            validate_threshold(threshold, total_weight(&signers, &weights_map))?;

            env.storage()
                .instance()
                .set(&DataKey::Weights, &weights_map);
            bump_instance(env);

            events::signer_weight_updated(env, signer, old_weight, new_weight);
            Ok(())
        }

        /// Used by `execute_signer_change` for `SignerChange::ChangeThreshold`.
        fn apply_change_threshold(env: &Env, new_threshold: u32) -> Result<(), MultisigError> {
            let signers = Self::get_required_signers(env)?;
            let weights_map = Self::get_weights_map(env);
            validate_threshold(new_threshold, total_weight(&signers, &weights_map))?;

            let old_threshold = Self::get_required_threshold(env)?;
            env.storage()
                .instance()
                .set(&DataKey::Threshold, &new_threshold);
            bump_instance(env);

            events::threshold_changed(env, old_threshold, new_threshold);
            Ok(())
        }

        #[inline]
        fn get_weights_map(env: &Env) -> Map<Address, u32> {
            env.storage()
                .instance()
                .get(&DataKey::Weights)
                .unwrap_or_else(|| Map::new(env))
        }

        #[inline]
        fn get_required_threshold(env: &Env) -> Result<u32, MultisigError> {
            env.storage()
                .instance()
                .get(&DataKey::Threshold)
                .ok_or(MultisigError::NotInitialized)
        }

        #[inline]
        fn get_required_signer_proposal(
            env: &Env,
            proposal_id: u64,
        ) -> Result<SignerProposal, MultisigError> {
            env.storage()
                .persistent()
                .get(&DataKey::SignerProposal(proposal_id))
                .ok_or(MultisigError::TransactionNotFound)
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
    }
}
