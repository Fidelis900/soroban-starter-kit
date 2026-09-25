#![no_std]
#![deny(missing_docs)]
//! Merkle-proof airdrop contract template.
//!
//! An admin sets a merkle root describing the distribution; eligible accounts
//! claim their allocation by presenting a merkle proof. A single tree may
//! distribute any number of tokens, and claims may optionally be subject to a
//! linear vesting schedule. Duplicate claims are rejected on-chain.
//!
//! # Leaf format
//!
//! Every leaf is bound to the network, to this contract instance, and to the
//! token being distributed, so a proof cannot be replayed on another network,
//! another airdrop contract, or for another token:
//!
//! ```text
//! leaf = sha256(
//!     "SOROBAN_AIRDROP_LEAF_V1"      // 23-byte ASCII domain separator
//!  || network_id                     // 32 bytes (sha256 of the network passphrase)
//!  || xdr(ScVal::Address(contract))  // this airdrop contract
//!  || xdr(ScVal::Address(recipient))
//!  || xdr(ScVal::Address(token))
//!  || amount                         // i128, 16-byte big-endian
//! )
//! ```
//!
//! Internal nodes are `sha256(min(a, b) || max(a, b))` (see
//! [`soroban_common::merkle`]). Because a leaf preimage is never 64 bytes
//! long, a leaf can never be confused with an internal node.

use soroban_sdk::{
    Address, Bytes, BytesN, Env, Map, Vec, contract, contractimpl, token, xdr::ToXdr,
};

mod errors;
mod events;
mod storage;

pub use errors::AirdropError;
pub use storage::{DataKey, VestingConfig, VestingSchedule};

use soroban_common::{
    BPS_DENOMINATOR, LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee,
    verify_merkle_multi_proof, verify_merkle_proof,
};

/// Domain separator prepended to every leaf preimage.
pub const LEAF_DOMAIN: &[u8] = b"SOROBAN_AIRDROP_LEAF_V1";

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
fn bump_claimed(env: &Env, round_id: u32, recipient: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Claimed(round_id, recipient.clone()),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

/// Compute the merkle leaf for `(recipient, token, amount)` as described in
/// the crate-level docs.
fn compute_leaf(env: &Env, recipient: &Address, token: &Address, amount: i128) -> BytesN<32> {
    let mut data = Bytes::from_slice(env, LEAF_DOMAIN);
    data.append(&Bytes::from(env.ledger().network_id()));
    data.append(&env.current_contract_address().to_xdr(env));
    data.append(&recipient.clone().to_xdr(env));
    data.append(&token.clone().to_xdr(env));
    data.extend_from_array(&amount.to_be_bytes());
    env.crypto().sha256(&data).into()
}

fn is_claimed(env: &Env, recipient: &Address, token: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&DataKey::Claimed(recipient.clone(), token.clone()))
        .unwrap_or(false)
}

fn total_locked(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::TotalLocked(token.clone()))
        .unwrap_or(0)
}

fn set_total_locked(env: &Env, token: &Address, value: i128) {
    let key = DataKey::TotalLocked(token.clone());
    env.storage().persistent().set(&key, &value);
    bump_persistent(env, &key);
}

/// Ensure the contract holds at least `amount` of `token` beyond what is
/// already locked for existing vesting schedules.
fn ensure_available(env: &Env, token: &Address, amount: i128) -> Result<(), AirdropError> {
    let balance = token::Client::new(env, token).balance(&env.current_contract_address());
    let available = balance
        .checked_sub(total_locked(env, token))
        .ok_or(AirdropError::ArithmeticOverflow)?;
    if available < amount {
        return Err(AirdropError::InsufficientBalance);
    }
    Ok(())
}

/// Load the root, and reject if the claim window has closed.
fn load_claim_context(env: &Env) -> Result<BytesN<32>, AirdropError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(AirdropError::NotInitialized);
    }

    let root_bytes: Bytes = env
        .storage()
        .instance()
        .get(&DataKey::MerkleRoot)
        .ok_or(AirdropError::RootNotSet)?;

    let deadline: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ClaimDeadline)
        .ok_or(AirdropError::NotInitialized)?;
    if env.ledger().sequence() > deadline {
        return Err(AirdropError::ClaimWindowClosed);
    }

    root_bytes.try_into().map_err(|_| AirdropError::RootNotSet)
}

/// Mark `(recipient, token)` claimed and pay out `amount`: the liquid portion
/// is transferred now and the rest is locked in a vesting schedule.
///
/// Callers must have already validated the proof, the claimed flag, and the
/// available balance.
fn settle_claim(
    env: &Env,
    recipient: &Address,
    token: &Address,
    amount: i128,
) -> Result<(), AirdropError> {
    // Checks-effects-interactions: mark claimed before transfer.
    let claimed_key = DataKey::Claimed(recipient.clone(), token.clone());
    env.storage().persistent().set(&claimed_key, &true);
    bump_persistent(env, &claimed_key);

    let vesting: Option<VestingConfig> = env.storage().instance().get(&DataKey::VestingConfig);
    let liquid = match &vesting {
        Some(cfg) => apply_bps_fee(amount, cfg.initial_unlock_bps)
            .ok_or(AirdropError::ArithmeticOverflow)?,
        None => amount,
    };
    let locked = amount
        .checked_sub(liquid)
        .ok_or(AirdropError::ArithmeticOverflow)?;

    if let (Some(cfg), true) = (vesting, locked > 0) {
        let schedule_key = DataKey::Vesting(recipient.clone(), token.clone());
        env.storage().persistent().set(
            &schedule_key,
            &VestingSchedule {
                total: locked,
                released: 0,
                start_ledger: env.ledger().sequence(),
                duration_ledgers: cfg.vesting_duration_ledgers,
            },
        );
        bump_persistent(env, &schedule_key);

        let new_total = total_locked(env, token)
            .checked_add(locked)
            .ok_or(AirdropError::ArithmeticOverflow)?;
        set_total_locked(env, token, new_total);

        events::locked(env, recipient, token, locked);
    }

    if liquid > 0 {
        token::Client::new(env, token).transfer(&env.current_contract_address(), recipient, &liquid);
    }

    events::claimed(env, recipient, token, amount);
    Ok(())
}

/// Amount of `schedule` vested at `now`, clamped to `schedule.total`.
fn vested_amount(schedule: &VestingSchedule, now: u32) -> Result<i128, AirdropError> {
    let elapsed = now.saturating_sub(schedule.start_ledger);
    if schedule.duration_ledgers == 0 || elapsed >= schedule.duration_ledgers {
        return Ok(schedule.total);
    }
    schedule
        .total
        .checked_mul(i128::from(elapsed))
        .and_then(|v| v.checked_div(i128::from(schedule.duration_ledgers)))
        .ok_or(AirdropError::ArithmeticOverflow)
}

fn releasable_amount(schedule: &VestingSchedule, now: u32) -> Result<i128, AirdropError> {
    vested_amount(schedule, now)?
        .checked_sub(schedule.released)
        .ok_or(AirdropError::ArithmeticOverflow)
}

/// Merkle-proof airdrop contract.
///
/// Lifecycle:
/// 1. Admin calls `initialize` with the claim deadline and optional vesting configuration.
/// 2. Admin calls `set_root` with the merkle root of the airdrop distribution tree
///    and funds the contract with every token in the tree.
/// 3. Each eligible address calls `claim(recipient, token, amount, proof)`, or a relayer
///    calls `claim_batch` with a single multi-proof. Duplicate claims are rejected on-chain.
/// 4. If vesting is enabled, recipients call `release` to withdraw vested tokens.
/// 1. Admin calls `initialize` to set the token address.
/// 2. Admin calls `set_root` with the merkle root of the airdrop distribution tree.
/// 3. Each eligible address calls `claim(round_id, recipient, amount, proof)` with a
///    pre-computed merkle proof, or signs an off-chain authorization that a sponsor
///    submits via `claim_for`. Duplicate claims are rejected on-chain.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct AirdropContract;

    #[contractimpl]
    impl AirdropContract {
        /// Initialize the airdrop contract.
        ///
        /// `vesting` is optional: when `Some`, every claim transfers
        /// `initial_unlock_bps` of the amount immediately and locks the rest
        /// in a linear vesting schedule of `vesting_duration_ledgers` ledgers.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::AlreadyInitialized`] if already initialized.
        /// Returns [`AirdropError::InvalidVestingConfig`] if `initial_unlock_bps > 10_000`,
        /// or if part of the claim is locked but `vesting_duration_ledgers == 0`.
        pub fn initialize(
            env: Env,
            admin: Address,
            claim_deadline: u32,
            vesting: Option<VestingConfig>,
        ) -> Result<(), AirdropError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(AirdropError::AlreadyInitialized);
            }

            admin.require_auth();

            if let Some(cfg) = &vesting {
                let bps = i128::from(cfg.initial_unlock_bps);
                if bps > BPS_DENOMINATOR
                    || (bps < BPS_DENOMINATOR && cfg.vesting_duration_ledgers == 0)
                {
                    return Err(AirdropError::InvalidVestingConfig);
                }
                env.storage().instance().set(&DataKey::VestingConfig, cfg);
            }

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage()
                .instance()
                .set(&DataKey::ClaimDeadline, &claim_deadline);
            bump_instance(&env);
            Ok(())
        }

        /// Set (or replace) the merkle root for a given round. Only the admin may call this.
        ///
        /// The root of an active, unexpired round cannot be replaced: once a round's
        /// root is set and its claim window is still open, it is immutable. This
        /// prevents an admin from invalidating outstanding proofs mid-round.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if the contract has not been initialized.
        /// Returns [`AirdropError::Unauthorized`] if caller is not the admin.
        /// Returns [`AirdropError::RoundActive`] if the round already has a root and is unexpired.
        pub fn set_root(
            env: Env,
            round_id: u32,
            root: BytesN<32>,
        ) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;

            admin.require_auth();

            // Prevent changing the root of an active, unexpired round.
            let existing: Option<Bytes> = env
                .storage()
                .instance()
                .get(&DataKey::MerkleRoot(round_id));
            if existing.is_some() {
                let deadline: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::ClaimDeadline)
                    .ok_or(AirdropError::NotInitialized)?;
                if env.ledger().sequence() <= deadline {
                    return Err(AirdropError::RoundActive);
                }
            }

            let root_bytes = Bytes::from(root.clone());
            env.storage()
                .instance()
                .set(&DataKey::MerkleRoot(round_id), &root_bytes);
            bump_instance(&env);

            events::root_set(&env, round_id, &root_bytes);
            Ok(())
        }

        /// Claim `amount` of `token` by supplying a valid merkle proof.
        ///
        /// The tree must contain the leaf `(recipient, token, amount)`. A
        /// recipient may claim each token in the tree once.
        /// Claim tokens by supplying a valid merkle proof for a given round.
        ///
        /// The caller must appear in the round's airdrop tree with exactly `amount` tokens.
        /// Claims are tracked per `(round_id, recipient)`, so a recipient may claim
        /// once in each round of a multi-round campaign.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        /// Returns [`AirdropError::InvalidAmount`] if `amount <= 0`.
        /// Returns [`AirdropError::AlreadyClaimed`] if this `(recipient, token)` already claimed.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set for the round.
        /// Returns [`AirdropError::InvalidAmount`] if `amount <= 0`.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        /// Returns [`AirdropError::AlreadyClaimed`] if the address already claimed in this round.
        /// Returns [`AirdropError::InvalidProof`] if the merkle proof does not verify.
        /// Returns [`AirdropError::InsufficientBalance`] if the contract cannot cover the claim.
        pub fn claim(
            env: Env,
            round_id: u32,
            recipient: Address,
            token: Address,
            amount: i128,
            proof: Vec<BytesN<32>>,
        ) -> Result<(), AirdropError> {
            let root = load_claim_context(&env)?;
            let token_addr: Address = env
            recipient.require_auth();
            execute_claim(&env, round_id, &recipient, amount, &proof)?;
            events::claimed(&env, round_id, &recipient, amount);
            Ok(())
        }

        /// Claim on behalf of `recipient` using an off-chain ed25519 signature.
        ///
        /// Lets a relayer or sponsor submit (and pay the fee for) a claim for a
        /// recipient who holds no native XLM. The recipient signs the payload
        /// returned by [`claim_for_payload`](Self::claim_for_payload) with the
        /// ed25519 key behind their `G...` account address; tokens are always
        /// delivered to `recipient`, never to the submitter.
        ///
        /// Replay protection: the signed payload is bound to the network, this
        /// contract, the round, the recipient and the amount, and each
        /// `(round_id, recipient)` pair can be claimed at most once.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::InvalidSignature`] if `recipient` is not an
        /// ed25519 account address. Otherwise returns the same errors as
        /// [`claim`](Self::claim).
        ///
        /// # Panics
        ///
        /// Traps if `signature` does not verify against the recipient's public key.
        pub fn claim_for(
            env: Env,
            round_id: u32,
            recipient: Address,
            amount: i128,
            proof: Vec<BytesN<32>>,
            signature: BytesN<64>,
        ) -> Result<(), AirdropError> {
            let public_key = account_public_key(&env, &recipient)?;
            let payload = claim_for_payload(&env, round_id, &recipient, amount);
            env.crypto().ed25519_verify(&public_key, &payload, &signature);

            execute_claim(&env, round_id, &recipient, amount, &proof)?;
            events::claimed_for(&env, round_id, &recipient, amount);
            Ok(())
        }

        /// Return the exact bytes a recipient must sign to authorize [`claim_for`](Self::claim_for).
        pub fn claim_for_payload(env: Env, round_id: u32, recipient: Address, amount: i128) -> Bytes {
            claim_for_payload(&env, round_id, &recipient, amount)
        }

        /// Sweep any tokens left unclaimed after the claim deadline.
        ///
        /// Only the admin may call this, and only once the claim window has
        /// closed (`env.ledger().sequence() > claim_deadline`). The entire
        /// remaining token balance held by the contract is transferred to
        /// `recipient`, preventing funds from being permanently locked.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::ClaimWindowNotClosed`] if the deadline has not passed.
        /// Returns [`AirdropError::NothingToSweep`] if the contract holds no tokens.
        pub fn sweep_unclaimed(env: Env, recipient: Address) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;

            admin.require_auth();

            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(AirdropError::NotInitialized)?;

            recipient.require_auth();

            if is_claimed(&env, &recipient, &token) {
            // Duplicate-claim prevention, scoped to this round.
            let claimed_key = DataKey::Claimed(round_id, recipient.clone());
            if env
                .storage()
                .persistent()
                .get::<_, bool>(&claimed_key)
                .unwrap_or(false)
            {
                return Err(AirdropError::AlreadyClaimed);
            }

            let leaf = compute_leaf(&env, &recipient, &token, amount);
            if !verify_merkle_proof(&env, &leaf, &proof, &root) {
                return Err(AirdropError::InvalidProof);
            }

            ensure_available(&env, &token, amount)?;

            settle_claim(&env, &recipient, &token, amount)?;
            bump_instance(&env);
            Ok(())
        }

        /// Batch-claim for multiple `(recipient, token, amount)` entries using
        /// a single merkle **multi-proof**.
        ///
        /// Intermediate nodes shared between the entries' paths are hashed
        /// only once, so the batch costs `proof_flags.len()` hashes instead of
        /// one full path per entry. `entries` must be in the leaf order
        /// produced by the multi-proof generator; see
        /// [`soroban_common::verify_merkle_multi_proof`] for the proof format.
        ///
        /// A relayer may submit on behalf of recipients (tokens always go to
        /// the recipient in the leaf). All-or-nothing: if any entry is invalid,
        /// nothing is claimed.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        /// Returns [`AirdropError::EmptyBatch`] if `entries` is empty.
        /// Returns [`AirdropError::InvalidAmount`] if any entry has `amount <= 0`.
        /// Returns [`AirdropError::DuplicateEntry`] if a `(recipient, token)` pair repeats.
        /// Returns [`AirdropError::AlreadyClaimed`] if any entry has already been claimed.
        /// Returns [`AirdropError::InvalidProof`] if the multi-proof does not verify.
        /// Returns [`AirdropError::InsufficientBalance`] if the contract cannot cover a token's total.
        pub fn claim_batch(
            env: Env,
            entries: Vec<(Address, Address, i128)>,
            proof: Vec<BytesN<32>>,
            proof_flags: Vec<bool>,
        ) -> Result<(), AirdropError> {
            let root = load_claim_context(&env)?;

            if entries.is_empty() {
                return Err(AirdropError::EmptyBatch);
            }

            // --- Validate all entries before any state change (all-or-nothing) ---
            let mut seen: Map<(Address, Address), bool> = Map::new(&env);
            let mut totals: Map<Address, i128> = Map::new(&env);
            let mut leaves: Vec<BytesN<32>> = Vec::new(&env);

            for (recipient, token, amount) in entries.iter() {
                if amount <= 0 {
                    return Err(AirdropError::InvalidAmount);
                }
                let pair = (recipient.clone(), token.clone());
                if seen.contains_key(pair.clone()) {
                    return Err(AirdropError::DuplicateEntry);
                }
                seen.set(pair, true);

                if is_claimed(&env, &recipient, &token) {
                    return Err(AirdropError::AlreadyClaimed);
                }

                let total = totals
                    .get(token.clone())
                    .unwrap_or(0)
                    .checked_add(amount)
                    .ok_or(AirdropError::ArithmeticOverflow)?;
                totals.set(token.clone(), total);

                leaves.push_back(compute_leaf(&env, &recipient, &token, amount));
            }

            if !verify_merkle_multi_proof(&env, &leaves, &proof, &proof_flags, &root) {
                return Err(AirdropError::InvalidProof);
            }

            for (token, total) in totals.iter() {
                ensure_available(&env, &token, total)?;
            }

            // --- Apply state changes and transfers ---
            for (recipient, token, amount) in entries.iter() {
                settle_claim(&env, &recipient, &token, amount)?;
            }

            bump_instance(&env);
            Ok(())
        }

        /// Release all currently vested tokens of `token` to `recipient`.
        ///
        /// Vesting continues after the claim deadline, so this may be called
        /// at any time. Returns the amount released.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NoVestingSchedule`] if `recipient` has no schedule for `token`.
        /// Returns [`AirdropError::NothingToRelease`] if nothing new has vested.
        pub fn release(env: Env, recipient: Address, token: Address) -> Result<i128, AirdropError> {
            recipient.require_auth();

            let key = DataKey::Vesting(recipient.clone(), token.clone());
            let mut schedule: VestingSchedule = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(AirdropError::NoVestingSchedule)?;

            let amount = releasable_amount(&schedule, env.ledger().sequence())?;
            if amount <= 0 {
                return Err(AirdropError::NothingToRelease);
            }

            schedule.released = schedule
                .released
                .checked_add(amount)
                .ok_or(AirdropError::ArithmeticOverflow)?;
            env.storage().persistent().set(&key, &schedule);
            bump_persistent(&env, &key);

            let remaining_locked = total_locked(&env, &token)
                .checked_sub(amount)
                .ok_or(AirdropError::ArithmeticOverflow)?;
            set_total_locked(&env, &token, remaining_locked);

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &recipient,
                &amount,
            );

            events::released(&env, &recipient, &token, amount);
            Ok(amount)
        }

        /// Returns `true` if `address` has already claimed its `token` allocation.
        pub fn is_claimed(env: Env, address: Address, token: Address) -> bool {
            is_claimed(&env, &address, &token)
        }

        /// Returns the current merkle root, or `None` if not set.
        pub fn get_root(env: Env) -> Option<Bytes> {
            env.storage().instance().get(&DataKey::MerkleRoot)
        }

        /// Returns the vesting configuration, or `None` if claims are fully liquid.
        pub fn get_vesting_config(env: Env) -> Option<VestingConfig> {
            env.storage().instance().get(&DataKey::VestingConfig)
        }

        /// Returns the vesting schedule for `(recipient, token)`, if any.
        pub fn get_vesting(env: Env, recipient: Address, token: Address) -> Option<VestingSchedule> {
            env.storage()
                .persistent()
                .get(&DataKey::Vesting(recipient, token))
        }

        /// Returns the amount of `token` that `recipient` could `release` right now.
        pub fn releasable(env: Env, recipient: Address, token: Address) -> i128 {
            env.storage()
                .persistent()
                .get::<_, VestingSchedule>(&DataKey::Vesting(recipient, token))
                .and_then(|s| releasable_amount(&s, env.ledger().sequence()).ok())
                .unwrap_or(0)
        }

        /// Returns the total amount of `token` still locked in vesting schedules.
        pub fn total_locked(env: Env, token: Address) -> i128 {
            total_locked(&env, &token)
        }
    }
}

#[cfg(test)]
mod test;
            // Checks-effects-interactions: mark claimed before transfer.
            env.storage().persistent().set(&claimed_key, &true);
            bump_claimed(&env, round_id, &recipient);
            let deadline: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ClaimDeadline)
                .ok_or(AirdropError::NotInitialized)?;
            if env.ledger().sequence() <= deadline {
                return Err(AirdropError::ClaimWindowNotClosed);
            }

            let token_client = token::Client::new(&env, &token_addr);
            let balance = token_client.balance(&env.current_contract_address());
            if balance <= 0 {
                return Err(AirdropError::NothingToSweep);
            }

            token_client.transfer(&env.current_contract_address(), &recipient, &balance);
            bump_instance(&env);

            events::unclaimed_swept(&env, &recipient, balance);
            Ok(())
        }

        /// Claim for many recipients in one transaction, skipping invalid entries.
        ///
        /// Entries with a non-positive amount, an invalid proof, or a recipient that
        /// has already claimed this round are skipped rather than reverting the batch.
        /// Returns the recipients that were successfully paid.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set for the round.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        pub fn claim_batch_lenient(
            env: Env,
            round_id: u32,
            claims: Vec<(Address, i128, Vec<BytesN<32>>)>,
        ) -> Result<Vec<Address>, AirdropError> {
            let (token_addr, root) = load_claim_context(&env, round_id)?;
            let token_client = token::Client::new(&env, &token_addr);

            let mut claimed = Vec::new(&env);
            for (recipient, amount, proof) in claims.iter() {
                if amount <= 0 {
                    continue;
                }
                let claimed_key = DataKey::Claimed(round_id, recipient.clone());
                if env.storage().persistent().has(&claimed_key) {
                    continue;
                }
                let leaf = compute_leaf(&env, &recipient, amount);
                if !verify_proof(&env, leaf, &proof, &root) {
                    continue;
                }

                env.storage().persistent().set(&claimed_key, &true);
                bump_claimed(&env, round_id, &recipient);
                token_client.transfer(&env.current_contract_address(), &recipient, &amount);

                events::batch_claimed(&env, round_id, &recipient, amount);
                claimed.push_back(recipient);
            }

            bump_instance(&env);
            Ok(claimed)
        }

        /// Returns `true` if `address` has already claimed in `round_id`.
        pub fn is_claimed(env: Env, round_id: u32, address: Address) -> bool {
            env.storage()
                .persistent()
                .get::<_, bool>(&DataKey::Claimed(round_id, address))
                .unwrap_or(false)
        }

        /// Returns the merkle root for `round_id`, or `None` if not set.
        pub fn get_root(env: Env, round_id: u32) -> Option<Bytes> {
            env.storage().instance().get(&DataKey::MerkleRoot(round_id))
        }
    }
}

/// Load the token address and merkle root for a round, rejecting claims after the deadline.
fn load_claim_context(env: &Env, round_id: u32) -> Result<(Address, BytesN<32>), AirdropError> {
    let token_addr: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .ok_or(AirdropError::NotInitialized)?;

    let root_bytes: Bytes = env
        .storage()
        .instance()
        .get(&DataKey::MerkleRoot(round_id))
        .ok_or(AirdropError::RootNotSet)?;

    let deadline: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ClaimDeadline)
        .ok_or(AirdropError::NotInitialized)?;
    if env.ledger().sequence() > deadline {
        return Err(AirdropError::ClaimWindowClosed);
    }

    let root: BytesN<32> = root_bytes
        .try_into()
        .map_err(|_| AirdropError::RootNotSet)?;
    Ok((token_addr, root))
}

/// Verify and settle a single claim. The caller is responsible for authorizing `recipient`.
fn execute_claim(
    env: &Env,
    round_id: u32,
    recipient: &Address,
    amount: i128,
    proof: &Vec<BytesN<32>>,
) -> Result<(), AirdropError> {
    if amount <= 0 {
        return Err(AirdropError::InvalidAmount);
    }

    let (token_addr, root) = load_claim_context(env, round_id)?;

    // Duplicate-claim prevention, scoped to this round.
    let claimed_key = DataKey::Claimed(round_id, recipient.clone());
    if env.storage().persistent().has(&claimed_key) {
        return Err(AirdropError::AlreadyClaimed);
    }

    let leaf = compute_leaf(env, recipient, amount);
    if !verify_proof(env, leaf, proof, &root) {
        return Err(AirdropError::InvalidProof);
    }

    // Checks-effects-interactions: mark claimed before transfer.
    env.storage().persistent().set(&claimed_key, &true);
    bump_claimed(env, round_id, recipient);
    bump_instance(env);

    token::Client::new(env, &token_addr).transfer(
        &env.current_contract_address(),
        recipient,
        &amount,
    );
    Ok(())
}

/// Domain-separation tag for [`AirdropContract::claim_for`] payloads.
const CLAIM_FOR_DOMAIN: &[u8] = b"soroban-airdrop:claim_for:v1";

/// Build the payload a recipient signs to authorize a sponsored claim:
/// `domain || network_id || contract_xdr || round_id_be || recipient_xdr || amount_be`.
fn claim_for_payload(env: &Env, round_id: u32, recipient: &Address, amount: i128) -> Bytes {
    let mut payload = Bytes::from_slice(env, CLAIM_FOR_DOMAIN);
    payload.append(&Bytes::from(env.ledger().network_id()));
    payload.append(&env.current_contract_address().to_xdr(env));
    payload.append(&Bytes::from_slice(env, &round_id.to_be_bytes()));
    payload.append(&recipient.clone().to_xdr(env));
    payload.append(&Bytes::from_slice(env, &amount.to_be_bytes()));
    payload
}

/// XDR prefix of an `ScVal::Address(ScAddress::Account(PublicKey::Ed25519(..)))`:
/// ScVal type 18 (address), ScAddress type 0 (account), PublicKey type 0 (ed25519).
const ACCOUNT_ED25519_XDR_PREFIX: [u8; 12] = [0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0, 0];
const ACCOUNT_ED25519_XDR_PREFIX_LEN: u32 = 12;
const ACCOUNT_ED25519_XDR_LEN: u32 = ACCOUNT_ED25519_XDR_PREFIX_LEN + 32;

/// Extract the ed25519 public key behind a `G...` account address.
fn account_public_key(env: &Env, address: &Address) -> Result<BytesN<32>, AirdropError> {
    let xdr = address.clone().to_xdr(env);
    if xdr.len() != ACCOUNT_ED25519_XDR_LEN
        || xdr.slice(..ACCOUNT_ED25519_XDR_PREFIX_LEN)
            != Bytes::from_array(env, &ACCOUNT_ED25519_XDR_PREFIX)
    {
        return Err(AirdropError::InvalidSignature);
    }
    xdr.slice(ACCOUNT_ED25519_XDR_PREFIX_LEN..)
        .try_into()
        .map_err(|_| AirdropError::InvalidSignature)
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;

