#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, String, Symbol,
};

/// Token contract implementing the Soroban Token Interface
/// 
/// This contract provides a complete implementation of a fungible token with:
/// - Standard token operations (transfer, balance, approve)
/// - Administrative controls (mint, burn, set_admin)
/// - Metadata support (name, symbol, decimals)
#[contract]
pub struct TokenContract;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Balance(Address),
    Allowance(AllowanceDataKey),
    Metadata(MetadataKey),
    TotalSupply,
}

#[contracttype]
#[derive(Clone)]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[contracttype]
#[derive(Clone)]
pub enum MetadataKey {
    Name,
    Symbol,
    Decimals,
}

/// Custom errors for the token contract
#[contracttype]
pub enum TokenError {
    InsufficientBalance = 1,
    InsufficientAllowance = 2,
    Unauthorized = 3,
    AlreadyInitialized = 4,
    NotInitialized = 5,
}

#[contractimpl]
impl TokenContract {
    /// Initialize the token with metadata and admin
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) -> Result<(), TokenError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TokenError::AlreadyInitialized);
        }

        admin.require_auth();

        // Set admin
        env.storage().instance().set(&DataKey::Admin, &admin);
        
        // Set metadata
        env.storage().instance().set(&DataKey::Metadata(MetadataKey::Name), &name);
        env.storage().instance().set(&DataKey::Metadata(MetadataKey::Symbol), &symbol);
        env.storage().instance().set(&DataKey::Metadata(MetadataKey::Decimals), &decimals);
        
        // Initialize total supply to 0
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);

        // Emit initialization event
        env.events().publish((Symbol::new(&env, "initialize"), admin.clone()), (name, symbol, decimals));

        Ok(())
    }

    /// Mint new tokens to a recipient (admin only)
    pub fn mint(env: Env, to: Address, amount: i128) -> Result<(), TokenError> {
        let admin: Address = env.storage().instance()
            .get(&DataKey::Admin)
            .ok_or(TokenError::NotInitialized)?;
        
        admin.require_auth();

        if amount < 0 {
            panic!("Amount must be non-negative");
        }

        // Update recipient balance
        let balance = Self::balance_of(env.clone(), to.clone());
        let new_balance = balance + amount;
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &new_balance);

        // Update total supply
        let total_supply: i128 = env.storage().instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalSupply, &(total_supply + amount));

        // Emit event
        env.events().publish((Symbol::new(&env, "mint"), to), amount);

        Ok(())
    }

    /// Burn tokens from an account (admin only)
    pub fn burn(env: Env, from: Address, amount: i128) -> Result<(), TokenError> {
        let admin: Address = env.storage().instance()
            .get(&DataKey::Admin)
            .ok_or(TokenError::NotInitialized)?;
        
        admin.require_auth();

        if amount < 0 {
            panic!("Amount must be non-negative");
        }

        let balance = Self::balance_of(env.clone(), from.clone());
        if balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balance
        let new_balance = balance - amount;
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &new_balance);

        // Update total supply
        let total_supply: i128 = env.storage().instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalSupply, &(total_supply - amount));

        // Emit event
        env.events().publish((Symbol::new(&env, "burn"), from), amount);

        Ok(())
    }

    /// Set a new admin (current admin only)
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), TokenError> {
        let admin: Address = env.storage().instance()
            .get(&DataKey::Admin)
            .ok_or(TokenError::NotInitialized)?;
        
        admin.require_auth();
        
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        
        // Emit event
        env.events().publish((Symbol::new(&env, "set_admin"),), new_admin);

        Ok(())
    }

    /// Get the current admin
    pub fn admin(env: Env) -> Address {
        env.storage().instance()
            .get(&DataKey::Admin)
            .unwrap()
    }

    /// Get token name
    pub fn name(env: Env) -> String {
        env.storage().instance()
            .get(&DataKey::Metadata(MetadataKey::Name))
            .unwrap()
    }

    /// Get token symbol
    pub fn symbol(env: Env) -> String {
        env.storage().instance()
            .get(&DataKey::Metadata(MetadataKey::Symbol))
            .unwrap()
    }

    /// Get token decimals
    pub fn decimals(env: Env) -> u32 {
        env.storage().instance()
            .get(&DataKey::Metadata(MetadataKey::Decimals))
            .unwrap()
    }

    /// Get total supply
    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }
}

// Implement the Soroban Token Interface
#[contractimpl]
impl token::Interface for TokenContract {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        let key = DataKey::Allowance(AllowanceDataKey { from, spender });
        env.storage().temporary().get(&key).unwrap_or(0)
    }

    fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        from.require_auth();

        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });

        env.storage().temporary().set(&key, &amount);
        if expiration_ledger > env.ledger().sequence() {
            env.storage().temporary().extend_ttl(&key, expiration_ledger, expiration_ledger);
        }

        env.events().publish(
            (Symbol::new(&env, "approve"), from, spender),
            amount,
        );
    }

    fn balance(env: Env, id: Address) -> i128 {
        Self::balance_of(env, id)
    }

    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        Self::transfer_impl(env, from, to, amount).unwrap();
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();

        // Check allowance
        let allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowance < amount {
            panic!("Insufficient allowance");
        }

        // Update allowance
        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        env.storage().temporary().set(&key, &(allowance - amount));

        // Perform transfer
        Self::transfer_impl(env, from, to, amount).unwrap();
    }
}

impl TokenContract {
    fn balance_of(env: Env, id: Address) -> i128 {
        env.storage().persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    fn transfer_impl(env: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if amount < 0 {
            panic!("Amount must be non-negative");
        }

        let from_balance = Self::balance_of(env.clone(), from.clone());
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }

        // Update balances
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        
        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(to_balance + amount));

        // Emit event
        env.events().publish((Symbol::new(&env, "transfer"), from, to), amount);

        Ok(())
    }
}

mod test;