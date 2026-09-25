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

        /// Set (or replace) the merkle root. Only the admin may call this.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if the contract has not been initialized.
        /// Returns [`AirdropError::Unauthorized`] if caller is not the admin.
        pub fn set_root(env: Env, root: BytesN<32>) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;

            admin.require_auth();

            let root_bytes = Bytes::from(root.clone());
            env.storage()
                .instance()
                .set(&DataKey::MerkleRoot, &root_bytes);
            bump_instance(&env);

            events::root_set(&env, &root_bytes);
            Ok(())
        }

        /// Claim `amount` of `token` by supplying a valid merkle proof.
        ///
        /// The tree must contain the leaf `(recipient, token, amount)`. A
        /// recipient may claim each token in the tree once.
        ///
        /// # Errors
        ///
        /// Returns [`AirdropError::NotInitialized`] if not initialized.
        /// Returns [`AirdropError::RootNotSet`] if no merkle root has been set.
        /// Returns [`AirdropError::ClaimWindowClosed`] if the claim deadline has passed.
        /// Returns [`AirdropError::InvalidAmount`] if `amount <= 0`.
        /// Returns [`AirdropError::AlreadyClaimed`] if this `(recipient, token)` already claimed.
        /// Returns [`AirdropError::InvalidProof`] if the merkle proof does not verify.
        /// Returns [`AirdropError::InsufficientBalance`] if the contract cannot cover the claim.
        pub fn claim(
            env: Env,
            recipient: Address,
            token: Address,
            amount: i128,
            proof: Vec<BytesN<32>>,
        ) -> Result<(), AirdropError> {
            let root = load_claim_context(&env)?;

            if amount <= 0 {
                return Err(AirdropError::InvalidAmount);
            }

            recipient.require_auth();

            if is_claimed(&env, &recipient, &token) {
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

#[cfg(test)]
mod prop_test;
