#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, Symbol,
};
 /// script
/// Escrow contract for secure two-party transactions
/// 
/// This contract holds funds in escrow until conditions are met:
/// - Buyer deposits funds
/// - Seller can claim after buyer approval or timeout
/// - Buyer can get refund if seller doesn't deliver
/// - Arbiter can resolve disputes
#[contract]
pub struct EscrowContract;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Buyer,
    Seller,
    Arbiter,
    TokenContract,
    Amount,
    Deadline,
    State,
    BuyerApproved,
    SellerDelivered,
}

#[contracttype]
#[derive(Clone, PartialEq)]
pub enum EscrowState {
    Created = 0,
    Funded = 1,
    Delivered = 2,
    Completed = 3,
    Refunded = 4,
    Disputed = 5,
}

/// Custom errors for the escrow contract
#[contracttype]
pub enum EscrowError {
    NotAuthorized = 1,
    InvalidState = 2,
    DeadlinePassed = 3,
    DeadlineNotReached = 4,
    AlreadyInitialized = 5,
    NotInitialized = 6,
    InsufficientFunds = 7,
}

#[contractimpl]
impl EscrowContract {
    /// Initialize the escrow with parties and terms
    pub fn initialize(
        env: Env,
        buyer: Address,
        seller: Address,
        arbiter: Address,
        token_contract: Address,
        amount: i128,
        deadline_ledger: u32,
    ) -> Result<(), EscrowError> {
        if env.storage().instance().has(&DataKey::State) {
            return Err(EscrowError::AlreadyInitialized);
        }

        // Verify deadline is in the future
        if deadline_ledger <= env.ledger().sequence() {
            panic!("Deadline must be in the future");
        }

        // Store escrow details
        env.storage().instance().set(&DataKey::Buyer, &buyer);
        env.storage().instance().set(&DataKey::Seller, &seller);
        env.storage().instance().set(&DataKey::Arbiter, &arbiter);
        env.storage().instance().set(&DataKey::TokenContract, &token_contract);
        env.storage().instance().set(&DataKey::Amount, &amount);
        env.storage().instance().set(&DataKey::Deadline, &deadline_ledger);
        env.storage().instance().set(&DataKey::State, &EscrowState::Created);
        env.storage().instance().set(&DataKey::BuyerApproved, &false);
        env.storage().instance().set(&DataKey::SellerDelivered, &false);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "escrow_created"), buyer.clone(), seller.clone()),
            amount,
        );

        Ok(())
    }

    /// Buyer funds the escrow
    pub fn fund(env: Env) -> Result<(), EscrowError> {
        let state: EscrowState = env.storage().instance()
            .get(&DataKey::State)
            .ok_or(EscrowError::NotInitialized)?;

        if state != EscrowState::Created {
            return Err(EscrowError::InvalidState);
        }

        let buyer: Address = env.storage().instance().get(&DataKey::Buyer).unwrap();
        let token_contract: Address = env.storage().instance().get(&DataKey::TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&DataKey::Amount).unwrap();

        buyer.require_auth();

        // Transfer tokens from buyer to contract
        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        // Update state
        env.storage().instance().set(&DataKey::State, &EscrowState::Funded);

        // Emit event
        env.events().publish((Symbol::new(&env, "escrow_funded"), buyer), amount);

        Ok(())
    }

    /// Seller marks delivery as complete
    pub fn mark_delivered(env: Env) -> Result<(), EscrowError> {
        let state: EscrowState = env.storage().instance()
            .get(&DataKey::State)
            .ok_or(EscrowError::NotInitialized)?;

        if state != EscrowState::Funded {
            return Err(EscrowError::InvalidState);
        }

        let seller: Address = env.storage().instance().get(&DataKey::Seller).unwrap();
        seller.require_auth();

        // Mark as delivered
        env.storage().instance().set(&DataKey::SellerDelivered, &true);
        env.storage().instance().set(&DataKey::State, &EscrowState::Delivered);

        // Emit event
        env.events().publish((Symbol::new(&env, "delivery_marked"), seller), ());

        Ok(())
    }

    /// Buyer approves delivery and releases funds
    pub fn approve_delivery(env: Env) -> Result<(), EscrowError> {
        let state: EscrowState = env.storage().instance()
            .get(&DataKey::State)
            .ok_or(EscrowError::NotInitialized)?;

        if state != EscrowState::Delivered {
            return Err(EscrowError::InvalidState);
        }

        let buyer: Address = env.storage().instance().get(&DataKey::Buyer).unwrap();
        buyer.require_auth();

        // Release funds to seller
        Self::release_to_seller(env)?;

        Ok(())
    }

    /// Buyer requests refund (only before delivery or after deadline)
    pub fn request_refund(env: Env) -> Result<(), EscrowError> {
        let state: EscrowState = env.storage().instance()
            .get(&DataKey::State)
            .ok_or(EscrowError::NotInitialized)?;

        let buyer: Address = env.storage().instance().get(&DataKey::Buyer).unwrap();
        let deadline: u32 = env.storage().instance().get(&DataKey::Deadline).unwrap();

        buyer.require_auth();

        // Check conditions for refund
        let can_refund = match state {
            EscrowState::Funded => {
                // Can refund if deadline passed and seller hasn't delivered
                env.ledger().sequence() > deadline
            }
            EscrowState::Delivered => {
                // Can refund if deadline passed and buyer hasn't approved
                env.ledger().sequence() > deadline
            }
            _ => false,
        };

        if !can_refund {
            return Err(EscrowError::DeadlineNotReached);
        }

        // Process refund
        Self::refund_to_buyer(env)?;

        Ok(())
    }

    /// Arbiter resolves dispute (can release to either party)
    pub fn resolve_dispute(env: Env, release_to_seller: bool) -> Result<(), EscrowError> {
        let state: EscrowState = env.storage().instance()
            .get(&DataKey::State)
            .ok_or(EscrowError::NotInitialized)?;

        if !matches!(state, EscrowState::Funded | EscrowState::Delivered) {
            return Err(EscrowError::InvalidState);
        }

        let arbiter: Address = env.storage().instance().get(&DataKey::Arbiter).unwrap();
        arbiter.require_auth();

        if release_to_seller {
            Self::release_to_seller(env)?;
        } else {
            Self::refund_to_buyer(env)?;
        }

        Ok(())
    }

    /// Get escrow details
    pub fn get_escrow_info(env: Env) -> (Address, Address, Address, Address, i128, u32, EscrowState) {
        let buyer: Address = env.storage().instance().get(&DataKey::Buyer).unwrap();
        let seller: Address = env.storage().instance().get(&DataKey::Seller).unwrap();
        let arbiter: Address = env.storage().instance().get(&DataKey::Arbiter).unwrap();
        let token_contract: Address = env.storage().instance().get(&DataKey::TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&DataKey::Amount).unwrap();
        let deadline: u32 = env.storage().instance().get(&DataKey::Deadline).unwrap();
        let state: EscrowState = env.storage().instance().get(&DataKey::State).unwrap();

        (buyer, seller, arbiter, token_contract, amount, deadline, state)
    }

    /// Get current state
    pub fn get_state(env: Env) -> EscrowState {
        env.storage().instance()
            .get(&DataKey::State)
            .unwrap_or(EscrowState::Created)
    }

    /// Check if deadline has passed
    pub fn is_deadline_passed(env: Env) -> bool {
        let deadline: u32 = env.storage().instance()
            .get(&DataKey::Deadline)
            .unwrap_or(0);
        env.ledger().sequence() > deadline
    }

    // Internal helper functions
    fn release_to_seller(env: Env) -> Result<(), EscrowError> {
        let seller: Address = env.storage().instance().get(&DataKey::Seller).unwrap();
        let token_contract: Address = env.storage().instance().get(&DataKey::TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&DataKey::Amount).unwrap();

        // Transfer tokens to seller
        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&env.current_contract_address(), &seller, &amount);

        // Update state
        env.storage().instance().set(&DataKey::State, &EscrowState::Completed);

        // Emit event
        env.events().publish((Symbol::new(&env, "funds_released"), seller), amount);

        Ok(())
    }

    fn refund_to_buyer(env: Env) -> Result<(), EscrowError> {
        let buyer: Address = env.storage().instance().get(&DataKey::Buyer).unwrap();
        let token_contract: Address = env.storage().instance().get(&DataKey::TokenContract).unwrap();
        let amount: i128 = env.storage().instance().get(&DataKey::Amount).unwrap();

        // Transfer tokens back to buyer
        let token_client = token::Client::new(&env, &token_contract);
        token_client.transfer(&env.current_contract_address(), &buyer, &amount);

        // Update state
        env.storage().instance().set(&DataKey::State, &EscrowState::Refunded);

        // Emit event
        env.events().publish((Symbol::new(&env, "funds_refunded"), buyer), amount);

        Ok(())
    }
}

mod test;