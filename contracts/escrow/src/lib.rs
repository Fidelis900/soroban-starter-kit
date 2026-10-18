#![no_std]
#![deny(missing_docs)]
//! Two-party escrow contract template.
//!
//! Funds move through `Created → Funded → Delivered → Completed`, with deadline
//! refunds, pre-fund cancellation, and optional dispute resolution.

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, contract, contractimpl};

mod admin;
mod dispute;
mod errors;
mod events;
mod lifecycle;
mod queries;
mod storage;

pub use errors::EscrowError;
pub use storage::{DataKey, EscrowInfo, EscrowState, Milestone};

#[cfg(feature = "pausable")]
use admin::require_admin;

/// Escrow contract for secure two-party transactions.
///
/// Lifecycle: `Created → Funded → Delivered → Completed`
/// with side exits to `Refunded` (deadline-based) or `Cancelled` (pre-fund).
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct EscrowContract;

    #[contractimpl]
    impl EscrowContract {
        #[allow(clippy::too_many_arguments)]
        pub fn initialize(
            env: Env,
            admin: Address,
            buyer: Address,
            seller: Address,
            arbiter: Address,
            token_contract: Address,
            amount: i128,
            deadline_ledger: u32,
            dispute_timeout_ledgers: u32,
            grace_period_ledgers: Option<u32>,
            metadata_hash: Option<soroban_sdk::BytesN<32>>,
            arbiter_fee_bps: Option<u32>,
        ) -> Result<(), EscrowError> {
            lifecycle::initialize(
                env,
                admin,
                buyer,
                seller,
                arbiter,
                token_contract,
                amount,
                deadline_ledger,
                dispute_timeout_ledgers,
                grace_period_ledgers,
                metadata_hash,
                arbiter_fee_bps,
            )
        }

        #[allow(clippy::too_many_arguments)]
        #[allow(clippy::too_many_arguments)]
        pub fn initialize_with_abandonment_buffer(
            env: Env,
            admin: Address,
            buyer: Address,
            seller: Address,
            arbiter: Address,
            token_contract: Address,
            amount: i128,
            deadline_ledger: u32,
            dispute_timeout_ledgers: u32,
            abandonment_buffer: u32,
            metadata_hash: Option<soroban_sdk::BytesN<32>>,
        ) -> Result<(), EscrowError> {
            lifecycle::initialize_with_abandonment_buffer(
                env,
                admin,
                buyer,
                seller,
                arbiter,
                token_contract,
                amount,
                deadline_ledger,
                dispute_timeout_ledgers,
                abandonment_buffer,
                metadata_hash,
            )
        }

        #[allow(clippy::too_many_arguments)]
        pub fn initialize_with_arbiters(
            env: Env,
            admin: Address,
            buyer: Address,
            seller: Address,
            arbiters: soroban_sdk::Vec<Address>,
            token_contract: Address,
            amount: i128,
            deadline_ledger: u32,
            required_signatures: u32,
            dispute_timeout_ledgers: u32,
            metadata_hash: Option<soroban_sdk::BytesN<32>>,
            arbiter_fee_bps: Option<u32>,
        ) -> Result<(), EscrowError> {
            lifecycle::initialize_with_arbiters(
                env,
                admin,
                buyer,
                seller,
                arbiters,
                token_contract,
                amount,
                deadline_ledger,
                required_signatures,
                dispute_timeout_ledgers,
                metadata_hash,
                arbiter_fee_bps,
            )
        }

        pub fn update_amount(env: Env, new_amount: i128) -> Result<(), EscrowError> {
            lifecycle::update_amount(env, new_amount)
        }

        pub fn fund(env: Env) -> Result<(), EscrowError> {
            lifecycle::fund(env)
        }

        pub fn mark_delivered(env: Env) -> Result<(), EscrowError> {
            lifecycle::mark_delivered(env)
        }

        pub fn approve_delivery(env: Env) -> Result<(), EscrowError> {
            lifecycle::approve_delivery(env)
        }

        pub fn release_partial(env: Env, amount: i128) -> Result<(), EscrowError> {
            lifecycle::release_partial(env, amount)
        }

        pub fn request_refund(env: Env) -> Result<(), EscrowError> {
            lifecycle::request_refund(env)
        }

        pub fn request_partial_refund(env: Env) -> Result<(), EscrowError> {
            lifecycle::request_partial_refund(env)
        }

        pub fn claim_expired_refund(env: Env) -> Result<(), EscrowError> {
            lifecycle::claim_expired_refund(env)
        }

        pub fn raise_dispute(env: Env, caller: Address) -> Result<(), EscrowError> {
            dispute::raise_dispute(env, caller)
        }

        pub fn resolve_dispute(
            env: Env,
            caller: Address,
            release_to_seller: bool,
        ) -> Result<(), EscrowError> {
            dispute::resolve_dispute(env, caller, release_to_seller)
        }

        /// Record an arbiter's approval for the current dispute verdict.
        ///
        /// In a multi-arbiter (M-of-N) escrow, each arbiter must approve the
        /// same verdict until `threshold` distinct approvals are collected,
        /// at which point the dispute is resolved automatically.
        pub fn approve_dispute(
            env: Env,
            arbiter: Address,
            release_to_seller: bool,
        ) -> Result<(), EscrowError> {
            dispute::approve_dispute(env, arbiter, release_to_seller)
        }

        pub fn claim_dispute_timeout(env: Env) -> Result<(), EscrowError> {
            dispute::claim_dispute_timeout(env)
        }

        pub fn cancel(env: Env) -> Result<(), EscrowError> {
            lifecycle::cancel(env)
        }

        /// Cancel a funded escrow by mutual agreement of both parties.
        ///
        /// Requires authentication from both the buyer (depositor) and the
        /// seller (beneficiary).  All unreleased funds are refunded to the
        /// depositor and the escrow transitions to `Cancelled`.
        pub fn mutual_cancel(
            env: Env,
            buyer: Address,
            seller: Address,
        ) -> Result<(), EscrowError> {
            lifecycle::mutual_cancel(env, buyer, seller)
        }

        pub fn extend_deadline(env: Env, new_deadline: u32) -> Result<(), EscrowError> {
            lifecycle::extend_deadline(env, new_deadline)
        }

        pub fn bump(env: Env) -> Result<(), EscrowError> {
            queries::bump(env)
        }

        /// Initialize an escrow with a list of independent milestones.
        ///
        /// The total escrowed amount is the sum of all milestone amounts.
        /// Each milestone can be released individually via [`release_milestone`].
        #[allow(clippy::too_many_arguments)]
        pub fn initialize_with_milestones(
            env: Env,
            admin: Address,
            buyer: Address,
            seller: Address,
            arbiter: Address,
            token_contract: Address,
            milestones: soroban_sdk::Vec<Milestone>,
            deadline_ledger: u32,
            dispute_timeout_ledgers: u32,
            metadata_hash: Option<soroban_sdk::BytesN<32>>,
        ) -> Result<(), EscrowError> {
            lifecycle::initialize_with_milestones(
                env,
                admin,
                buyer,
                seller,
                arbiter,
                token_contract,
                milestones,
                deadline_ledger,
                dispute_timeout_ledgers,
                metadata_hash,
            )
        }

        /// Release a single milestone's funds to the seller.
        ///
        /// `caller` must be the buyer or the arbiter.
        /// `milestone_index` is the zero-based position in the milestone list.
        pub fn release_milestone(
            env: Env,
            caller: Address,
            milestone_index: u32,
        ) -> Result<(), EscrowError> {
            lifecycle::release_milestone(env, caller, milestone_index)
        }

        /// Approve a partial release of a single milestone.
        ///
        /// `release_amount` is paid to the seller and the remainder of the
        /// milestone allocation is refunded to the buyer.  `release_amount`
        /// must not exceed the milestone's allocated amount.
        pub fn approve_milestone_partial(
            env: Env,
            milestone_id: u32,
            release_amount: i128,
        ) -> Result<(), EscrowError> {
            lifecycle::approve_milestone_partial(env, milestone_id, release_amount)
        }

        /// Configure the fee deducted from each release and the treasury address
        /// it is routed to.  Must be called by the buyer.
        ///
        /// `fee_bps` is in basis points (0 = no fee, 10 000 = 100 %).
        pub fn set_fee_config(
            env: Env,
            fee_bps: u32,
            treasury: Address,
        ) -> Result<(), EscrowError> {
            lifecycle::set_fee_config(env, fee_bps, treasury)
        }

        /// Return the current fee in basis points and treasury address, or
        /// `(0, None)` if not configured.
        #[must_use]
        pub fn get_fee_config(env: Env) -> (u32, Option<Address>) {
            let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
            let treasury: Option<Address> = env.storage().instance().get(&DataKey::Treasury);
            (fee_bps, treasury)
        }

        /// Return the list of milestones, or an empty list for a standard (non-milestone) escrow.
        #[must_use]
        pub fn get_milestones(env: Env) -> soroban_sdk::Vec<Milestone> {
            lifecycle::get_milestones(env)
        }

        // ── Multi-token entry points (#1192) ──────────────────────────────

        /// Initialize a multi-token escrow holding a basket of SEP-41 tokens.
        ///
        /// * `token_contracts` – ordered list of token contract addresses.
        /// * `token_amounts`   – parallel list of amounts for each token.
        ///
        /// Both slices must be non-empty and of equal length.  The first entry
        /// acts as the "lead" token for fee accounting and event emission.
        #[allow(clippy::too_many_arguments)]
        pub fn initialize_with_tokens(
            env: Env,
            admin: Address,
            buyer: Address,
            seller: Address,
            arbiter: Address,
            token_contracts: soroban_sdk::Vec<Address>,
            token_amounts: soroban_sdk::Vec<i128>,
            deadline_ledger: u32,
            dispute_timeout_ledgers: u32,
            metadata_hash: Option<soroban_sdk::BytesN<32>>,
        ) -> Result<(), EscrowError> {
            lifecycle::initialize_with_tokens(
                env,
                admin,
                buyer,
                seller,
                arbiter,
                token_contracts,
                token_amounts,
                deadline_ledger,
                dispute_timeout_ledgers,
                metadata_hash,
            )
        }

        /// Fund a multi-token escrow by transferring all basket tokens from
        /// the buyer to the contract.  Falls back to `fund` for single-token
        /// escrows.
        pub fn fund_multi_token(env: Env) -> Result<(), EscrowError> {
            lifecycle::fund_multi_token(env)
        }

        /// Release all basket tokens to the seller.
        /// Falls back to `release_to_seller` for single-token escrows.
        pub fn release_multi_token(env: Env) -> Result<(), EscrowError> {
            lifecycle::release_multi_token(env)
        }

        /// Refund all basket tokens to the buyer.
        /// Falls back to `refund_to_buyer` for single-token escrows.
        pub fn refund_multi_token(env: Env) -> Result<(), EscrowError> {
            lifecycle::refund_multi_token(env)
        }

        #[must_use]
        pub fn get_escrow_info(env: Env) -> Result<EscrowInfo, EscrowError> {
            queries::get_escrow_info(env)
        }

        #[must_use]
        pub fn get_state(env: Env) -> Option<EscrowState> {
            queries::get_state(env)
        }

        #[must_use]
        pub fn is_deadline_passed(env: Env) -> bool {
            queries::is_deadline_passed(env)
        }

        pub fn get_remaining_ledgers(env: Env) -> i64 {
            queries::get_remaining_ledgers(env)
        }

        /// Return the on-chain contract version number.
        pub fn contract_version(env: Env) -> u32 {
            env.storage().instance().get(&DataKey::Version).unwrap_or(0)
        }
    }

    /// Pause / unpause — only compiled when the `pausable` feature is enabled.
    #[cfg(feature = "pausable")]
    #[contractimpl]
    impl EscrowContract {
        const UPGRADE_DELAY_LEDGERS: u32 = 17_280;

        pub fn pause(env: Env) -> Result<(), EscrowError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &true);
            lifecycle::extend_ttl(&env);
            events::paused(&env, &admin);
            Ok(())
        }

        pub fn unpause(env: Env) -> Result<(), EscrowError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            env.storage().instance().set(&DataKey::Paused, &false);
            lifecycle::extend_ttl(&env);
            events::unpaused(&env, &admin);
            Ok(())
        }

        #[must_use]
        pub fn version(env: Env) -> soroban_sdk::String {
            soroban_sdk::String::from_str(&env, env!("GIT_HASH"))
        }

        pub fn propose_upgrade(
            env: Env,
            wasm_hash: soroban_sdk::BytesN<32>,
        ) -> Result<(), EscrowError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            let ready_after = env.ledger().sequence() + Self::UPGRADE_DELAY_LEDGERS;
            env.storage()
                .instance()
                .set(&DataKey::PendingUpgrade, &(wasm_hash.clone(), ready_after));
            lifecycle::extend_ttl(&env);
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "upgrade_proposed"), admin),
                (wasm_hash, ready_after),
            );
            Ok(())
        }

        pub fn execute_upgrade(env: Env) -> Result<(), EscrowError> {
            let admin = require_admin(&env)?;
            admin.require_auth();
            let (wasm_hash, ready_after): (soroban_sdk::BytesN<32>, u32) = env
                .storage()
                .instance()
                .get(&DataKey::PendingUpgrade)
                .ok_or(EscrowError::NotAuthorized)?;
            if env.ledger().sequence() < ready_after {
                return Err(EscrowError::NotAuthorized);
            }
            env.storage().instance().remove(&DataKey::PendingUpgrade);
            events::upgraded(&env, &admin, &wasm_hash);
            env.events().publish(
                (soroban_sdk::Symbol::new(&env, "upgrade_executed"), admin),
                wasm_hash.clone(),
            );
            env.deployer().update_current_contract_wasm(wasm_hash);
            Ok(())
        }
    }

    impl EscrowContract {
        #[cfg(feature = "pausable")]
        pub(crate) fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
            if env
                .storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false)
            {
                return Err(EscrowError::NotAuthorized);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod test;
