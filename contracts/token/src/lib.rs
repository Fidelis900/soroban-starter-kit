#![no_std]

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, String,
};
use token::TokenInterface as _;

mod admin;
mod errors;
mod events;
mod storage;

#[cfg(test)]
mod test;
mod prop_test;

use admin::require_admin;
use errors::TokenError;
use storage::{AllowanceDataKey, AllowanceValue, DataKey, MetadataKey};

const LEDGER_LIFETIME_THRESHOLD: u32 = 120_960;
const LEDGER_BUMP_AMOUNT: u32 = 518_400;

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

#[contract]
pub struct TokenContract;

#[contractimpl]
impl TokenContract {
    /// Initialize the token with metadata and an admin. Must be called once.
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
        max_supply: Option<i128>,
    ) -> Result<(), TokenError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TokenError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Metadata(MetadataKey::Name), &name);
        env.storage()
            .instance()
            .set(&DataKey::Metadata(MetadataKey::Symbol), &symbol);
        env.storage()
            .instance()
            .set(&DataKey::Metadata(MetadataKey::Decimals), &decimals);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        let _ = max_supply; // reserved for future max-supply enforcement
        bump_instance(&env);
        events::initialized(&env, &admin, name, symbol, decimals);
        Ok(())
    }

    /// Mint `amount` tokens to `to`. Admin only.
    pub fn mint(env: Env, to: Address, amount: i128) -> Result<(), TokenError> {
        let admin = require_admin(&env)?;
        admin.require_auth();
        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        let new_balance = balance.checked_add(amount).ok_or(TokenError::Overflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance);
        bump_persistent(&env, &DataKey::Balance(to.clone()));
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply + amount));
        bump_instance(&env);
        events::minted(&env, &to, amount);
        Ok(())
    }

    /// Burn `amount` tokens from `from`. Admin only.
    pub fn admin_burn(env: Env, from: Address, amount: i128) -> Result<(), TokenError> {
        let admin = require_admin(&env)?;
        admin.require_auth();
        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        if balance < amount {
            return Err(TokenError::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(balance - amount));
        bump_persistent(&env, &DataKey::Balance(from.clone()));
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply - amount));
        bump_instance(&env);
        events::burned(&env, &from, amount);
        Ok(())
    }

    /// Transfer admin role to `new_admin`. Current admin only.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), TokenError> {
        let old_admin = require_admin(&env)?;
        old_admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        bump_instance(&env);
        events::admin_changed(&env, &old_admin, &new_admin);
        Ok(())
    }

    /// Return the current admin address.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap()
    }

    /// Return the current total token supply.
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }
}

#[contractimpl]
impl token::TokenInterface for TokenContract {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
        match val {
            Some(v) if env.ledger().sequence() <= v.expiration_ledger => v.amount,
            _ => 0,
        }
    }

    fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        from.require_auth();
        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        env.storage()
            .temporary()
            .set(&key, &AllowanceValue { amount, expiration_ledger });
        if expiration_ledger > env.ledger().sequence() {
            env.storage()
                .temporary()
                .extend_ttl(&key, expiration_ledger, expiration_ledger);
        }
        // Emit a distinct revoke event when amount == 0 (allowance revocation),
        // so off-chain systems can distinguish revocations from normal approvals.
        if amount == 0 {
            events::revoked(&env, &from, &spender);
        } else {
            events::approved(&env, &from, &spender, amount);
        }
    }

    fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if let Err(e) = Self::transfer_impl(&env, from, to, amount) {
            panic_with_error!(&env, e);
        }
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
        let allowance = match val {
            Some(v) if env.ledger().sequence() <= v.expiration_ledger => v.amount,
            _ => 0,
        };
        if allowance < amount {
            panic_with_error!(&env, TokenError::InsufficientAllowance);
        }
        env.storage().temporary().set(
            &key,
            &AllowanceValue {
                amount: allowance - amount,
                expiration_ledger: env.ledger().sequence() + LEDGER_BUMP_AMOUNT,
            },
        );
        if let Err(e) = Self::transfer_impl(&env, from, to, amount) {
            panic_with_error!(&env, e);
        }
    }

    fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, TokenError::InvalidAmount);
        }
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        if balance < amount {
            panic_with_error!(&env, TokenError::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(balance - amount));
        bump_persistent(&env, &DataKey::Balance(from.clone()));
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply - amount));
        bump_instance(&env);
        events::burned(&env, &from, amount);
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        let key = DataKey::Allowance(AllowanceDataKey {
            from: from.clone(),
            spender: spender.clone(),
        });
        let val: Option<AllowanceValue> = env.storage().temporary().get(&key);
        let allowance = match val {
            Some(v) if env.ledger().sequence() <= v.expiration_ledger => v.amount,
            _ => 0,
        };
        if allowance < amount {
            panic_with_error!(&env, TokenError::InsufficientAllowance);
        }
        env.storage().temporary().set(
            &key,
            &AllowanceValue {
                amount: allowance - amount,
                expiration_ledger: env.ledger().sequence() + LEDGER_BUMP_AMOUNT,
            },
        );
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        if balance < amount {
            panic_with_error!(&env, TokenError::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(balance - amount));
        bump_persistent(&env, &DataKey::Balance(from.clone()));
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply - amount));
        bump_instance(&env);
        events::burned(&env, &from, amount);
    }

    fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Metadata(MetadataKey::Decimals))
            .unwrap()
    }

    fn name(env: Env) -> String {
        env.storage()
            .instance()
            .get(&DataKey::Metadata(MetadataKey::Name))
            .unwrap()
    }

    fn symbol(env: Env) -> String {
        env.storage()
            .instance()
            .get(&DataKey::Metadata(MetadataKey::Symbol))
            .unwrap()
    }
}

impl TokenContract {
    fn transfer_impl(env: &Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }
        let from_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        bump_persistent(env, &DataKey::Balance(from.clone()));
        let to_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(to_balance + amount));
        bump_persistent(env, &DataKey::Balance(to.clone()));
        events::transferred(env, &from, &to, amount);
        Ok(())
    }
}
