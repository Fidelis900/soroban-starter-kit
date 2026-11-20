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
//! Every leaf is bound to the network, to this contract instance, to the
//! token being distributed, and to the allocation tier, so a proof cannot be
//! replayed on another network, another airdrop contract, for another token,
//! or under a different tier:
//!
//! ```text
//! leaf = sha256(
//!     "SOROBAN_AIRDROP_LEAF_V2"      // 23-byte ASCII domain separator
//!  || network_id                     // 32 bytes (sha256 of the network passphrase)
//!  || xdr(ScVal::Address(contract))  // this airdrop contract
//!  || xdr(ScVal::Address(recipient))
//!  || xdr(ScVal::Address(token))
//!  || amount                         // i128, 16-byte big-endian
//!  || tier                           // u32, 4-byte big-endian tier identifier
//! )
//! ```
//!
//! # Tiers
//!
//! Campaigns may distribute tiered allocations (e.g. tier 1 = 1,000 tokens,
//! tier 2 = 5,000 tokens). The tier identifier is part of the leaf, so a
//! recipient can only claim under the tier the tree assigns them. The claimed
//! tier is recorded on-chain (see `get_claimed_tier`) and emitted on the
//! `claimed` event for analytics indexing.
//!
//! # Front-running protection
//!
//! Every claim requires `recipient.require_auth()`, and tokens are only ever
//! paid to the recipient committed in the leaf. A bot that copies a pending
//! proof from the mempool can neither submit it without the recipient's
//! signature nor redirect the payout to itself: substituting its own address
//! changes the leaf and the proof no longer verifies.
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
pub use storage::{DataKey, DecayConfig, VestingConfig, VestingSchedule};

use soroban_common::{
    BPS_DENOMINATOR, LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, apply_bps_fee,
    verify_merkle_multi_proof, verify_merkle_proof,
};

/// Domain separator prepended to every leaf preimage.
pub const LEAF_DOMAIN: &[u8] = b"SOROBAN_AIRDROP_LEAF_V2";

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

/// Compute the merkle leaf for `(recipient, token, amount, tier)` as described
/// in the crate-level docs.
pub fn compute_leaf(
    env: &Env,
    recipient: &Address,
    token: &Address,
    amount: i128,
    tier: u32,
) -> BytesN<32> {
    let mut data = Bytes::from_slice(env, LEAF_DOMAIN);
    data.append(&Bytes::from(env.ledger().network_id()));
    data.append(&env.current_contract_address().to_xdr(env));
    data.append(&recipient.clone().to_xdr(env));
    data.append(&token.clone().to_xdr(env));
    data.extend_from_array(&amount.to_be_bytes());
    data.extend_from_array(&tier.to_be_bytes());
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

/// Reject claims while the airdrop is emergency-paused.
fn require_not_paused(env: &Env) -> Result<(), AirdropError> {
    if env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(AirdropError::Paused);
    }
    Ok(())
}

/// Load the root, and reject if the claim window has closed or claims are paused.
fn load_claim_context(env: &Env) -> Result<BytesN<32>, AirdropError> {
    require_not_paused(env)?;
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

/// Claimable share (in basis points) for a claim at ledger `now`.
fn decay_bps(cfg: &DecayConfig, now: u32) -> u32 {
    if now <= cfg.start_ledger + cfg.full_window_ledgers {
        return BPS_DENOMINATOR as u32;
    }
    let elapsed = now - cfg.start_ledger - cfg.full_window_ledgers;
    let steps = elapsed / cfg.decay_step_ledgers;
    let reduction = steps
        .saturating_mul(cfg.decay_bps_per_step)
        .min(BPS_DENOMINATOR as u32 - cfg.floor_bps);
    (BPS_DENOMINATOR as u32 - cfg.floor_bps - reduction).max(cfg.floor_bps)
}

/// Apply the configured time decay to `amount`, returning the claimant's share
/// and the remainder that belongs to the campaign treasury.
fn decayed_amount(env: &Env, amount: i128) -> Result<(i128, i128), AirdropError> {
    let cfg: Option<DecayConfig> = env.storage().instance().get(&DataKey::DecayConfig);
    let Some(cfg) = cfg else {
        return Ok((amount, 0));
    };
    let bps = i128::from(decay_bps(&cfg, env.ledger().sequence()));
    let claimable = amount
        .checked_mul(bps)
        .and_then(|v| v.checked_div(BPS_DENOMINATOR))
        .ok_or(AirdropError::ArithmeticOverflow)?;
    let remainder = amount
        .checked_sub(claimable)
        .ok_or(AirdropError::ArithmeticOverflow)?;
    Ok((claimable, remainder))
}

/// Mark `(recipient, token)` claimed and pay out `amount`: the liquid portion
/// is transferred now and the rest is locked in a vesting schedule. When a
/// decay schedule is configured, the decayed remainder is sent to the treasury.
///
/// Callers must have already validated the proof, the claimed flag, and the
/// available balance.
fn settle_claim(
    env: &Env,
    recipient: &Address,
    token: &Address,
    amount: i128,
    tier: u32,
) -> Result<(), AirdropError> {
    let (claimable, remainder) = decayed_amount(env, amount)?;

    // Checks-effects-interactions: mark claimed before transfer.
    let claimed_key = DataKey::Claimed(recipient.clone(), token.clone());
    env.storage().persistent().set(&claimed_key, &true);
    bump_persistent(env, &claimed_key);

    let tier_key = DataKey::ClaimedTier(recipient.clone(), token.clone());
    env.storage().persistent().set(&tier_key, &tier);
    bump_persistent(env, &tier_key);

    let vesting: Option<VestingConfig> = env.storage().instance().get(&DataKey::VestingConfig);
    let liquid = match &vesting {
        Some(cfg) => apply_bps_fee(claimable, cfg.initial_unlock_bps)
            .ok_or(AirdropError::ArithmeticOverflow)?,
        None => claimable,
    };
    let locked = claimable
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

    if remainder > 0 {
        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(AirdropError::NotInitialized)?;
        token::Client::new(env, token).transfer(
            &env.current_contract_address(),
            &treasury,
            &remainder,
        );
        events::decay_remainder(env, &treasury, token, remainder);
    }

    events::claimed(env, recipient, token, claimable);
    events::claimed(env, recipient, token, amount, tier);
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
/// 3. Each eligible address calls `claim(recipient, token, amount, tier, proof)`, or a
///    relayer calls `claim_batch` with a single multi-proof (each recipient must
///    still authorize their own entry). Duplicate claims are rejected on-chain.
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

        /// Pause or resume all claims. Only the admin may call this.
        ///
        /// Pausing is an emergency brake for a bad Merkle root or leaf list:
        /// `claim`, `claim_for` and `claim_batch` all fail with
        /// [`AirdropError::Paused`] until the admin resumes the airdrop.
        pub fn set_paused(env: Env, paused: bool) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &paused);
            bump_instance(&env);
            events::paused(&env, paused);
            Ok(())
        }

        /// Returns `true` while claims are emergency-paused.
        pub fn is_paused(env: Env) -> bool {
            env.storage()
                .instance()
                .get::<_, bool>(&DataKey::Paused)
                .unwrap_or(false)
        }

        /// Configure time-decaying claim windows and the treasury that receives
        /// the unclaimed decay remainder. Only the admin may call this.
        pub fn set_decay_config(
            env: Env,
            treasury: Address,
            config: DecayConfig,
        ) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;
            admin.require_auth();

            if config.decay_step_ledgers == 0
                || config.floor_bps > BPS_DENOMINATOR as u32
                || config.decay_bps_per_step > BPS_DENOMINATOR as u32
            {
                return Err(AirdropError::InvalidDecayConfig);
            }

            env.storage().instance().set(&DataKey::Treasury, &treasury);
            env.storage().instance().set(&DataKey::DecayConfig, &config);
            bump_instance(&env);
            events::decay_config_set(&env, &treasury, &config);
            Ok(())
        }

        /// Returns the configured decay schedule, if any.
        pub fn get_decay_config(env: Env) -> Option<DecayConfig> {
            env.storage().instance().get(&DataKey::DecayConfig)
        }

        /// Configure the DAO contract authorised to advance Merkle epochs.
        /// Only the admin may call this.
        pub fn set_governance(env: Env, governance: Address) -> Result<(), AirdropError> {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(AirdropError::NotInitialized)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Governance, &governance);
            bump_instance(&env);
            Ok(())
        }

        /// Install a new Merkle root for the next campaign epoch.
        ///
        /// Only the configured governance DAO may call this. The previous
        /// epoch's claimed bitmaps are intentionally left untouched, so a
        /// recipient who already claimed cannot claim again under the new root.
        pub fn update_root_by_governance(
            env: Env,
            new_root: BytesN<32>,
            new_deadline: u32,
        ) -> Result<(), AirdropError> {
            let governance: Address = env
                .storage()
                .instance()
                .get(&DataKey::Governance)
                .ok_or(AirdropError::NotInitialized)?;
            governance.require_auth();

            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(AirdropError::NotInitialized);
            }

            let epoch: u32 = env
                .storage()
                .instance()
                .get(&DataKey::Epoch)
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(AirdropError::ArithmeticOverflow)?;

            let root_bytes = Bytes::from(new_root);
            let key = DataKey::MerkleRoot(epoch);
            env.storage().instance().set(&key, &root_bytes);
            env.storage()
                .instance()
                .set(&DataKey::MerkleRoot, &root_bytes);
            env.storage().instance().set(&DataKey::Epoch, &epoch);
            env.storage()
                .instance()
                .set(&DataKey::ClaimDeadline, &new_deadline);
            bump_instance(&env);

            events::epoch_root_updated(&env, epoch, &root_bytes, new_deadline);
            Ok(())
        }

        /// Returns the current Merkle epoch (0 before the first governance update).
        pub fn get_epoch(env: Env) -> u32 {
            env.storage().instance().get(&DataKey::Epoch).unwrap_or(0)
        }

        /// Returns the configured governance address, if any.
        pub fn get_governance(env: Env) -> Option<Address> {
            env.storage().instance().get(&DataKey::Governance)
        }

        /// Claim `amount` of `token` by supplying a valid merkle proof.
        /// Claim `amount` of `token` under allocation `tier` by supplying a
        /// valid merkle proof.
        ///
        /// The tree must contain the leaf `(recipient, token, amount, tier)`. A
        /// recipient may claim each token in the tree once. `recipient` must
        /// authorize the call, so a copied proof cannot be submitted by a
        /// front-runner, and tokens are only ever paid to `recipient`.
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
            tier: u32,
            proof: Vec<BytesN<32>>,
        ) -> Result<(), AirdropError> {
            // Bind the claim to the recipient before any other work so that a
            // mempool sniper replaying the proof is rejected outright.
            recipient.require_auth();

            let root = load_claim_context(&env)?;

            if amount <= 0 {
                return Err(AirdropError::InvalidAmount);
            }

            if is_claimed(&env, &recipient, &token) {
                return Err(AirdropError::AlreadyClaimed);
            }

            let leaf = compute_leaf(&env, &recipient, &token, amount, tier);
            if !verify_merkle_proof(&env, &leaf, &proof, &root) {
                return Err(AirdropError::InvalidProof);
            }

            ensure_available(&env, &token, amount)?;

            settle_claim(&env, &recipient, &token, amount, tier)?;
            bump_instance(&env);
            Ok(())
        }

        /// Batch-claim for multiple `(recipient, token, amount, tier)` entries using
        /// a single merkle **multi-proof**.
        ///
        /// Intermediate nodes shared between the entries' paths are hashed
        /// only once, so the batch costs `proof_flags.len()` hashes instead of
        /// one full path per entry. `entries` must be in the leaf order
        /// produced by the multi-proof generator; see
        /// [`soroban_common::verify_merkle_multi_proof`] for the proof format.
        ///
        /// A relayer may submit on behalf of recipients, but every recipient
        /// must authorize the invocation (`require_auth`), so a relayer or a
        /// front-runner cannot force a claim the recipient did not sign for.
        /// Tokens always go to the recipient in the leaf. All-or-nothing: if any entry is invalid,
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
            entries: Vec<(Address, Address, i128, u32)>,
            proof: Vec<BytesN<32>>,
            proof_flags: Vec<bool>,
        ) -> Result<(), AirdropError> {
            let root = load_claim_context(&env)?;

            if entries.is_empty() {
                return Err(AirdropError::EmptyBatch);
            }

            // --- Validate all entries before any state change (all-or-nothing) ---
            let mut seen: Map<(Address, Address), bool> = Map::new(&env);
            let mut authorized: Map<Address, bool> = Map::new(&env);
            let mut totals: Map<Address, i128> = Map::new(&env);
            let mut leaves: Vec<BytesN<32>> = Vec::new(&env);

            for (recipient, token, amount, tier) in entries.iter() {
                if !authorized.contains_key(recipient.clone()) {
                    recipient.require_auth();
                    authorized.set(recipient.clone(), true);
                }
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

                leaves.push_back(compute_leaf(&env, &recipient, &token, amount, tier));
            }

            if !verify_merkle_multi_proof(&env, &leaves, &proof, &proof_flags, &root) {
                return Err(AirdropError::InvalidProof);
            }

            for (token, total) in totals.iter() {
                ensure_available(&env, &token, total)?;
            }

            // --- Apply state changes and transfers ---
            for (recipient, token, amount, tier) in entries.iter() {
                settle_claim(&env, &recipient, &token, amount, tier)?;
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

        /// Returns the tier under which `address` claimed its `token`
        /// allocation, or `None` if it has not claimed.
        pub fn get_claimed_tier(env: Env, address: Address, token: Address) -> Option<u32> {
            env.storage()
                .persistent()
                .get(&DataKey::ClaimedTier(address, token))
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
