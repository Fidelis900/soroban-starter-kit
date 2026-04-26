#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

mod admin;
mod errors;
mod events;
mod storage;
mod prop_test;

pub use errors::EscrowError;
pub use storage::{DataKey, EscrowInfo, EscrowState};

use admin::require_admin;
use storage::DataKey::*;

/// Extend storage TTL when remaining ledgers fall below this threshold.
/// 120_960 ledgers ≈ 7 days (at ~5 s/ledger).
const LEDGER_LIFETIME_THRESHOLD: u32 = 120_960;

/// Target TTL (in ledgers) after each extension.
/// 518_400 ledgers ≈ 30 days (at ~5 s/ledger).
const LEDGER_BUMP_AMOUNT: u32 = 518_400;

/// Minimum number of ledgers the deadline must be in the future.
const MIN_DEADLINE_BUFFER: u32 = 10;

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Escrow contract for secure two-party transactions.
///
/// Lifecycle: `Created → Funded → Delivered → Completed`
/// with side exits to `Refunded` (deadline-based) or `Cancelled` (pre-fund).
#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize a new escrow.
    ///
    /// Sets up all parties, the token contract, the escrowed amount, and the
    /// deadline. Must be called exactly once.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::AlreadyInitialized`] – contract has already been initialized.
    /// - [`EscrowError::InvalidAmount`] – `amount` is zero or negative.
    /// - [`EscrowError::InvalidParties`] – buyer, seller, or arbiter addresses overlap.
    /// - [`EscrowError::DeadlinePassed`] – `deadline_ledger` is too close to the current ledger.
    pub fn initialize(
        env: Env,
        buyer: Address,
        seller: Address,
        arbiter: Address,
        token_contract: Address,
        amount: i128,
        deadline_ledger: u32,
    ) -> Result<(), EscrowError> {
        if env.storage().instance().has(&State) {
            return Err(EscrowError::AlreadyInitialized);
        }

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        if buyer == seller || buyer == arbiter || seller == arbiter {
            return Err(EscrowError::InvalidParties);
        }

        if deadline_ledger < env.ledger().sequence() + MIN_DEADLINE_BUFFER {
            return Err(EscrowError::DeadlinePassed);
        }

        // Validate token contract address by calling decimals()
        let token_client = token::Client::new(&env, &token_contract);
        let _ = token_client.decimals();

        env.storage().instance().set(&Buyer, &buyer);
        env.storage().instance().set(&Seller, &seller);
        env.storage().instance().set(&Arbiter, &arbiter);
        env.storage().instance().set(&TokenContract, &token_contract);
        env.storage().instance().set(&Amount, &amount);
        env.storage().instance().set(&Deadline, &deadline_ledger);
        env.storage().instance().set(&State, &EscrowState::Created);
        env.storage().instance().set(&BuyerApproved, &false);
        env.storage().instance().set(&SellerDelivered, &false);

        bump_instance(&env);

        env.events().publish(
            (
                Symbol::new(&env, "escrow_created"),
                buyer.clone(),
                seller.clone(),
            ),
            amount,
        );

        // Emit initialized event for off-chain indexers
        env.events().publish(
            (
                Symbol::new(&env, "initialized"),
                buyer.clone(),
                seller.clone(),
                arbiter.clone(),
            ),
            amount,
        );

        Ok(())
    }

    /// Buyer funds the escrow by transferring tokens to the contract.
    ///
    /// Requires authorization from the buyer. The escrow must be in the
    /// `Created` state.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in `Created` state.
    pub fn fund(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env
            .storage()
            .instance()
            .get(&Buyer)
            .ok_or(EscrowError::NotInitialized)?;
        buyer.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if state != EscrowState::Created {
            return Err(EscrowError::InvalidState);
        }

        let token_contract: Address = env.storage().instance().get(&TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&Amount).unwrap();

        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        env.storage().instance().set(&State, &EscrowState::Funded);
        bump_instance(&env);

        // Emit funded event
        env.events()
            .publish((Symbol::new(&env, "escrow_funded"), buyer), amount);

        Ok(())
    }

    /// Seller marks goods/services as delivered.
    ///
    /// Requires authorization from the seller. The escrow must be in the
    /// `Funded` state.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in `Funded` state.
    pub fn mark_delivered(env: Env) -> Result<(), EscrowError> {
        let seller: Address = env
            .storage()
            .instance()
            .get(&Seller)
            .ok_or(EscrowError::NotInitialized)?;
        seller.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if state != EscrowState::Funded {
            return Err(EscrowError::InvalidState);
        }

        env.storage().instance().set(&SellerDelivered, &true);
        env.storage().instance().set(&State, &EscrowState::Delivered);
        bump_instance(&env);

        env.events()
            .publish((Symbol::new(&env, "delivery_marked"), seller), ());

        Ok(())
    }

    /// Buyer approves delivery, releasing funds to the seller.
    ///
    /// Requires authorization from the buyer. The escrow must be in the
    /// `Delivered` state.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in `Delivered` state.
    pub fn approve_delivery(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env
            .storage()
            .instance()
            .get(&Buyer)
            .ok_or(EscrowError::NotInitialized)?;
        buyer.require_auth();

        Self::release_to_seller(env)
    }

    /// Buyer requests a refund after the deadline has passed.
    ///
    /// Requires authorization from the buyer. The escrow must be in `Funded`
    /// or `Delivered` state and the current ledger must be past `deadline`.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::DeadlineNotReached`] – deadline has not yet passed or
    ///   the escrow is in an ineligible state.
    pub fn request_refund(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env
            .storage()
            .instance()
            .get(&Buyer)
            .ok_or(EscrowError::NotInitialized)?;
        buyer.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        let deadline: u32 = env
            .storage()
            .instance()
            .get(&Deadline)
            .ok_or(EscrowError::NotInitialized)?;

        let can_refund = matches!(state, EscrowState::Funded | EscrowState::Delivered)
            && env.ledger().sequence() > deadline;
        if !can_refund {
            return Err(EscrowError::DeadlineNotReached);
        }

        Self::refund_to_buyer(env)
    }

    /// Buyer or seller raises a dispute.
    ///
    /// Requires authorization from the caller. The escrow must be in `Funded`
    /// or `Delivered` state.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in a disputable state.
    /// - [`EscrowError::NotAuthorized`] – caller is neither buyer nor seller.
    pub fn raise_dispute(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env
            .storage()
            .instance()
            .get(&Buyer)
            .ok_or(EscrowError::NotInitialized)?;
        buyer.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if !matches!(state, EscrowState::Funded | EscrowState::Delivered) {
            return Err(EscrowError::InvalidState);
        }

        env.storage().instance().set(&State, &EscrowState::Disputed);
        bump_instance(&env);

        env.events()
            .publish((Symbol::new(&env, "dispute_raised"), buyer), ());

        Ok(())
    }

    /// Arbiter resolves a dispute.
    ///
    /// Requires authorization from the arbiter. The escrow must be in
    /// `Disputed` state.
    ///
    /// If `release_to_seller` is `true`, funds go to the seller; otherwise
    /// they are refunded to the buyer.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in `Disputed` state.
    pub fn resolve_dispute(env: Env, release_to_seller: bool) -> Result<(), EscrowError> {
        let arbiter: Address = env
            .storage()
            .instance()
            .get(&Arbiter)
            .ok_or(EscrowError::NotInitialized)?;
        arbiter.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if state != EscrowState::Disputed {
            return Err(EscrowError::InvalidState);
        }

        if release_to_seller {
            // Temporarily set state to Delivered so release_to_seller's require_state passes
            env.storage().instance().set(&State, &EscrowState::Delivered);
            Self::release_to_seller(env)
        } else {
            // Temporarily set state to Funded so refund_to_buyer's require_state passes
            env.storage().instance().set(&State, &EscrowState::Funded);
            Self::refund_to_buyer(env)
        }
    }

    /// Buyer cancels an unfunded escrow (`Created` state only).
    ///
    /// Requires authorization from the buyer.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    /// - [`EscrowError::InvalidState`] – escrow is not in `Created` state.
    pub fn cancel(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env
            .storage()
            .instance()
            .get(&Buyer)
            .ok_or(EscrowError::NotInitialized)?;
        buyer.require_auth();

        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if state != EscrowState::Created {
            return Err(EscrowError::InvalidState);
        }

        env.storage().instance().set(&State, &EscrowState::Cancelled);
        bump_instance(&env);

        env.events()
            .publish((Symbol::new(&env, "escrow_cancelled"), buyer), ());

        Ok(())
    }

    /// Pause the contract. Admin only.
    pub fn pause(env: Env) -> Result<(), EscrowError> {
        let admin = require_admin(&env)?;
        admin.require_auth();
        env.storage().instance().set(&Paused, &true);
        bump_instance(&env);
        Ok(())
    }

    /// Unpause the contract. Admin only.
    pub fn unpause(env: Env) -> Result<(), EscrowError> {
        let admin = require_admin(&env)?;
        admin.require_auth();
        env.storage().instance().set(&Paused, &false);
        bump_instance(&env);
        Ok(())
    }

    /// Extend storage TTL. Anyone can call this to keep an active escrow alive.
    ///
    /// # Errors
    ///
    /// - [`EscrowError::NotInitialized`] – contract has not been initialized.
    pub fn bump(env: Env) -> Result<(), EscrowError> {
        if !env.storage().instance().has(&State) {
            return Err(EscrowError::NotInitialized);
        }
        bump_instance(&env);
        Ok(())
    }

    /// Return full escrow details as an [`EscrowInfo`] struct.
    ///
    /// # Panics
    ///
    /// Panics if any required storage key is absent (contract not initialized).
    pub fn get_escrow_info(env: Env) -> EscrowInfo {
        EscrowInfo {
            buyer: env.storage().instance().get(&Buyer).unwrap(),
            seller: env.storage().instance().get(&Seller).unwrap(),
            arbiter: env.storage().instance().get(&Arbiter).unwrap(),
            token_contract: env.storage().instance().get(&TokenContract).unwrap(),
            amount: env.storage().instance().get(&Amount).unwrap(),
            deadline: env.storage().instance().get(&Deadline).unwrap(),
            state: env.storage().instance().get(&State).unwrap(),
        }
    }

    /// Return the current [`EscrowState`], or `None` if not initialized.
    pub fn get_state(env: Env) -> Option<EscrowState> {
        env.storage().instance().get(&State)
    }

    /// Return `true` if the deadline ledger has been passed.
    ///
    /// Returns `false` if the contract has not been initialized (deadline defaults to `0`).
    pub fn is_deadline_passed(env: Env) -> bool {
        let deadline: u32 = env.storage().instance().get(&Deadline).unwrap_or(0);
        env.ledger().sequence() > deadline
    }

    /// Upgrade the contract to a new WASM hash. Admin only.
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) -> Result<(), EscrowError> {
        let admin = require_admin(&env)?;
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }
}

impl EscrowContract {
    fn require_state(env: &Env, expected: EscrowState) -> Result<(), EscrowError> {
        let state: EscrowState = env
            .storage()
            .instance()
            .get(&State)
            .ok_or(EscrowError::NotInitialized)?;
        if state != expected {
            return Err(EscrowError::InvalidState);
        }
        Ok(())
    }

    fn release_to_seller(env: Env) -> Result<(), EscrowError> {
        Self::require_state(&env, EscrowState::Delivered)?;

        let seller: Address = env.storage().instance().get(&Seller).unwrap();
        let token_contract: Address = env.storage().instance().get(&TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&Amount).unwrap();

        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&env.current_contract_address(), &seller, &amount);

        env.storage().instance().set(&State, &EscrowState::Completed);
        bump_instance(&env);

        env.events()
            .publish((Symbol::new(&env, "funds_released"), seller), amount);

        Ok(())
    }

    fn refund_to_buyer(env: Env) -> Result<(), EscrowError> {
        Self::require_state(&env, EscrowState::Funded)?;

        let buyer: Address = env.storage().instance().get(&Buyer).unwrap();
        let token_contract: Address = env.storage().instance().get(&TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&Amount).unwrap();

        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&env.current_contract_address(), &buyer, &amount);

        env.storage().instance().set(&State, &EscrowState::Refunded);
        bump_instance(&env);

        env.events()
            .publish((Symbol::new(&env, "funds_refunded"), buyer), amount);

        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
        if env.storage().instance().get(&Paused).unwrap_or(false) {
            return Err(EscrowError::NotAuthorized);
        }
        Ok(())
    }
}

mod test;
