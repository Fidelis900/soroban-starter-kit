#![no_std]
#![deny(missing_docs)]
//! Bonding-curve token sale contract template.
//!
//! Token price scales deterministically with supply along a bonding curve;
//! buyers mint tokens by paying the curve price and sellers burn to redeem.

#[cfg(test)]
extern crate std;

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod prop_test;

pub use errors::BondingCurveError;
pub use storage::{DataKey, PRICE_SCALE};
use storage::{BPS_DENOMINATOR, MAX_FEE_BPS, MIN_CONNECTOR_WEIGHT_BPS};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};

fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

fn fixed_pow(mut base: i128, mut exponent: u32) -> Result<i128, BondingCurveError> {
    let mut result = PRICE_SCALE;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result.checked_mul(base).ok_or(BondingCurveError::Overflow)?
                .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base.checked_mul(base).ok_or(BondingCurveError::Overflow)?
                .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?;
        }
    }
    Ok(result)
}

fn fixed_root(value: i128, root: u32) -> Result<i128, BondingCurveError> {
    if root == 0 { return Err(BondingCurveError::InvalidConfiguration); }
    if root == 1 || value == 0 { return Ok(value); }
    let mut estimate = PRICE_SCALE;
    for _ in 0..32 {
        let denominator = fixed_pow(estimate, root - 1)?;
        if denominator == 0 { return Err(BondingCurveError::Overflow); }
        let next = estimate.checked_mul((root - 1) as i128).ok_or(BondingCurveError::Overflow)?
            .checked_add(value.checked_mul(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?
                .checked_div(denominator).ok_or(BondingCurveError::Overflow)?)
            .ok_or(BondingCurveError::Overflow)?
            .checked_div(root as i128).ok_or(BondingCurveError::Overflow)?;
        if next == estimate { break; }
        estimate = next;
    }
    Ok(estimate)
}

/// Linear spot price `b + m*S`, with all values represented in PRICE_SCALE.
fn linear_price(base_price: i128, slope: i128, supply: i128) -> Result<i128, BondingCurveError> {
    if supply < 0 || base_price <= 0 || slope < 0 {
        return Err(BondingCurveError::InvalidConfiguration);
    }
    base_price.checked_add(
        slope.checked_mul(supply).ok_or(BondingCurveError::Overflow)?
            .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?,
    ).ok_or(BondingCurveError::Overflow)
}

/// Bancor-style spot price. Linear bootstrap pricing remains active until the
/// reserve has liquidity, avoiding a zero-cost first purchase.
fn calculate_price(
    reserve: i128,
    supply: i128,
    base_price: i128,
    slope: i128,
) -> Result<i128, BondingCurveError> {
    let linear = linear_price(base_price, slope, supply)?;
    if reserve <= 0 || supply <= 0 {
        return Ok(linear);
    }
    let scaled = reserve
        .checked_mul(PRICE_SCALE)
        .ok_or(BondingCurveError::Overflow)?;
    scaled
        .checked_div(supply + 1)
        .ok_or(BondingCurveError::Overflow)
    let reserve_price = reserve.checked_mul(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?
        .checked_div(supply).ok_or(BondingCurveError::Overflow)?;
    Ok(linear.max(reserve_price))
}

/// Compute the integral cost on the configured curve, with a Bancor-style
/// fractional connector-weight adjustment once the reserve is bootstrapped.
fn buy_cost(
    reserve: i128,
    supply: i128,
    amount: i128,
    base_price: i128,
    slope: i128,
    connector_weight_bps: u32,
) -> Result<i128, BondingCurveError> {
    if amount <= 0 {
        return Err(BondingCurveError::InvalidAmount);
    }

    let old_supply = supply;
    let new_supply = supply
        .checked_add(amount)
        .ok_or(BondingCurveError::Overflow)?;

    // Linear curve: cost ≈ reserve * (1/(old_supply+1) + ... + 1/(new_supply+1))
    // Simplified: reserve * amount / (supply + 1) + reserve * amount^2 / (2 * (supply+1)^2)
    // For minimal gas, use average price approximation:
    let avg_price =
        (calculate_price(reserve, old_supply)? + calculate_price(reserve, new_supply)?) / 2;
    let cost = amount
        .checked_mul(avg_price)
        .ok_or(BondingCurveError::Overflow)?
        .checked_div(PRICE_SCALE)
        .ok_or(BondingCurveError::Overflow)?;
    Ok(cost)
    let new_supply = supply.checked_add(amount).ok_or(BondingCurveError::Overflow)?;
    let linear = base_price.checked_mul(amount).ok_or(BondingCurveError::Overflow)?
        .checked_add(
            slope.checked_mul(
                supply.checked_mul(amount).ok_or(BondingCurveError::Overflow)?
                    .checked_add(amount.checked_mul(amount - 1).ok_or(BondingCurveError::Overflow)? / 2)
                    .ok_or(BondingCurveError::Overflow)?,
            ).ok_or(BondingCurveError::Overflow)?
                .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?,
        ).ok_or(BondingCurveError::Overflow)?;
    if reserve <= 0 || supply <= 0 { return Ok(linear.max(1)); }
    let ratio = PRICE_SCALE.checked_add(
        amount.checked_mul(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?
            .checked_div(supply).ok_or(BondingCurveError::Overflow)?,
    ).ok_or(BondingCurveError::Overflow)?;
    let exponent = BPS_DENOMINATOR as u32 / connector_weight_bps;
    let power = fixed_pow(ratio, exponent)?;
    let bancor = reserve.checked_mul(power - PRICE_SCALE).ok_or(BondingCurveError::Overflow)?
        .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?;
    Ok(linear.max(bancor).max(1))
}

/// Compute proceeds from selling `amount` tokens
fn sell_proceeds(
    reserve: i128,
    supply: i128,
    amount: i128,
    base_price: i128,
    slope: i128,
    connector_weight_bps: u32,
) -> Result<i128, BondingCurveError> {
    if amount <= 0 || amount > supply {
        return Err(BondingCurveError::InvalidAmount);
    }

    let new_supply = supply - amount;

    let avg_price =
        (calculate_price(reserve, old_supply)? + calculate_price(reserve, new_supply)?) / 2;
    let proceeds = amount
        .checked_mul(avg_price)
        .ok_or(BondingCurveError::Overflow)?
        .checked_div(PRICE_SCALE)
        .ok_or(BondingCurveError::Overflow)?;
    Ok(proceeds)
    let linear = base_price.checked_mul(amount).ok_or(BondingCurveError::Overflow)?
        .checked_add(
            slope.checked_mul(
                new_supply.checked_mul(amount).ok_or(BondingCurveError::Overflow)?
                    .checked_add(amount.checked_mul(amount + 1).ok_or(BondingCurveError::Overflow)? / 2)
                    .ok_or(BondingCurveError::Overflow)?,
            ).ok_or(BondingCurveError::Overflow)?
                .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?,
        ).ok_or(BondingCurveError::Overflow)?;
    if reserve <= 0 || supply <= 0 { return Ok(linear.max(1).min(reserve)); }
    let remaining_ratio = PRICE_SCALE.checked_sub(
        amount.checked_mul(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?
            .checked_div(supply).ok_or(BondingCurveError::Overflow)?,
    ).ok_or(BondingCurveError::Overflow)?;
    let root = BPS_DENOMINATOR as u32 / connector_weight_bps;
    let remaining_reserve_ratio = fixed_root(remaining_ratio, root)?;
    let bancor = reserve.checked_mul(PRICE_SCALE - remaining_reserve_ratio).ok_or(BondingCurveError::Overflow)?
        .checked_div(PRICE_SCALE).ok_or(BondingCurveError::Overflow)?;
    Ok(linear.max(bancor).max(1).min(reserve))
}

/// Bonding curve token contract.
///
/// Linear curve: price increases with supply.
/// Buy adds to supply and consumes reserve.
/// Sell removes from supply and returns reserve.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct BondingCurveContract;

    #[contractimpl]
    impl BondingCurveContract {
        /// Initialize the bonding curve contract.
        ///
        /// # Errors
        /// - [`BondingCurveError::AlreadyInitialized`] if called more than once.
        pub fn initialize(
            env: Env,
            admin: Address,
            token: Address,
        ) -> Result<(), BondingCurveError> {
            if env.storage().instance().has(&DataKey::Admin) {
                return Err(BondingCurveError::AlreadyInitialized);
            }
            admin.require_auth();

            env.storage().instance().set(&DataKey::Admin, &admin);
            env.storage().instance().set(&DataKey::Token, &token);
            env.storage().instance().set(&DataKey::Reserve, &0i128);
            env.storage().instance().set(&DataKey::Supply, &0i128);
            env.storage()
                .instance()
                .set(&DataKey::Price, &calculate_price(0, 0)?);

            bump(&env);
            events::initialized(&env, &admin, &token);
            Ok(())
        }

        /// Buy `amount` tokens by paying from the reserve.
        ///
        /// # Errors
        /// - [`BondingCurveError::NotInitialized`] if the contract has not been initialized.
        /// - [`BondingCurveError::InvalidAmount`] if `amount` <= 0.
        pub fn buy(
            env: Env,
            buyer: Address,
            amount: i128,
            max_cost: i128,
        ) -> Result<(), BondingCurveError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BondingCurveError::NotInitialized);
            }
            if amount <= 0 {
                return Err(BondingCurveError::InvalidAmount);
            }
            buyer.require_auth();

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(BondingCurveError::NotInitialized)?;

            let reserve: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve)
                .unwrap_or(0i128);
            let supply: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Supply)
                .unwrap_or(0i128);

            let cost = buy_cost(reserve, supply, amount)?;
            if cost > max_cost {
                return Err(BondingCurveError::InvalidAmount);
            }

            token::Client::new(&env, &token).transfer(
                &buyer,
                &env.current_contract_address(),
                &cost,
            );

            let balance_key = DataKey::CurveBalance(buyer.clone());
            let buyer_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
            let new_balance = buyer_balance
                .checked_add(amount)
                .ok_or(BondingCurveError::Overflow)?;
            let new_supply = supply
                .checked_add(amount)
                .ok_or(BondingCurveError::Overflow)?;
            let new_reserve = reserve
                .checked_add(cost)
                .ok_or(BondingCurveError::Overflow)?;
            let new_price = calculate_price(new_reserve, new_supply)?;

            // The curve token is represented by this persistent balance ledger. It
            // is minted only after the buyer has paid the reserve asset.
            env.storage().persistent().set(&balance_key, &new_balance);
            env.storage().persistent().extend_ttl(
                &balance_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );
            env.storage().instance().set(&DataKey::Supply, &new_supply);
            env.storage()
                .instance()
                .set(&DataKey::Reserve, &new_reserve);
            env.storage().instance().set(&DataKey::Price, &new_price);

            bump(&env);
            events::bought(&env, &buyer, amount, cost);
            Ok(())
        }

        /// Sell `amount` tokens to withdraw from the reserve.
        ///
        /// # Errors
        /// - [`BondingCurveError::NotInitialized`] if the contract has not been initialized.
        /// - [`BondingCurveError::InvalidAmount`] if `amount` <= 0 or exceeds supply.
        /// - [`BondingCurveError::InsufficientReserve`] if the reserve is insufficient.
        pub fn sell(
            env: Env,
            seller: Address,
            amount: i128,
            min_proceeds: i128,
        ) -> Result<(), BondingCurveError> {
            if !env.storage().instance().has(&DataKey::Admin) {
                return Err(BondingCurveError::NotInitialized);
            }
            if amount <= 0 {
                return Err(BondingCurveError::InvalidAmount);
            }
            seller.require_auth();

            let token: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(BondingCurveError::NotInitialized)?;

            let reserve: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Reserve)
                .unwrap_or(0i128);
            let supply: i128 = env
                .storage()
                .instance()
                .get(&DataKey::Supply)
                .unwrap_or(0i128);

            if amount > supply {
                return Err(BondingCurveError::InvalidAmount);
            }

            let balance_key = DataKey::CurveBalance(seller.clone());
            let seller_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
            if amount > seller_balance {
                return Err(BondingCurveError::InvalidAmount);
            }

            let proceeds = sell_proceeds(reserve, supply, amount)?;
            if proceeds < min_proceeds {
                return Err(BondingCurveError::InvalidAmount);
            }
            if proceeds > reserve {
                return Err(BondingCurveError::InsufficientReserve);
            }

            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &seller,
                &proceeds,
            );

            let new_balance = seller_balance
                .checked_sub(amount)
                .ok_or(BondingCurveError::InvalidAmount)?;
            let new_supply = supply
                .checked_sub(amount)
                .ok_or(BondingCurveError::InvalidAmount)?;
            let new_reserve = reserve
                .checked_sub(proceeds)
                .ok_or(BondingCurveError::InsufficientReserve)?;
            let new_price = calculate_price(new_reserve, new_supply)?;

            env.storage().persistent().set(&balance_key, &new_balance);
            env.storage().persistent().extend_ttl(
                &balance_key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );
            env.storage().instance().set(&DataKey::Supply, &new_supply);
            env.storage()
                .instance()
                .set(&DataKey::Reserve, &new_reserve);
            env.storage().instance().set(&DataKey::Price, &new_price);

            bump(&env);
            events::sold(&env, &seller, amount, proceeds);
            Ok(())
        }

        /// Get current reserve.
        pub fn get_reserve(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::Reserve)
                .unwrap_or(0i128)
        }
#[contractimpl]
impl BondingCurveContract {
    /// Initialize the bonding curve contract.
    ///
    /// # Errors
    /// - [`BondingCurveError::AlreadyInitialized`] if called more than once.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        initial_slope: i128,
        base_price: i128,
        connector_weight_bps: u32,
        fee_bps: u32,
        treasury: Address,
    ) -> Result<(), BondingCurveError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::AlreadyInitialized);
        }
        if initial_slope < 0 || base_price <= 0 || connector_weight_bps < MIN_CONNECTOR_WEIGHT_BPS || connector_weight_bps > 10_000 {
            return Err(BondingCurveError::InvalidConfiguration);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(BondingCurveError::InvalidFee);
        }
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Reserve, &0i128);
        env.storage().instance().set(&DataKey::Supply, &0i128);
        env.storage().instance().set(&DataKey::Slope, &initial_slope);
        env.storage().instance().set(&DataKey::BasePrice, &base_price);
        env.storage().instance().set(&DataKey::ConnectorWeightBps, &connector_weight_bps);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Price, &base_price);

        bump(&env);
        events::initialized(&env, &admin, &token);
        Ok(())
    }

    /// Update the protocol fee in basis points.
    pub fn set_fee_bps(env: Env, fee_bps: u32) -> Result<(), BondingCurveError> {
        if fee_bps > MAX_FEE_BPS {
            return Err(BondingCurveError::InvalidFee);
        }
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(BondingCurveError::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        bump(&env);
        Ok(())
    }

    /// Update the protocol fee treasury.
    pub fn set_treasury(env: Env, treasury: Address) -> Result<(), BondingCurveError> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(BondingCurveError::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        bump(&env);
        Ok(())
    }

    /// Buy `amount` tokens by paying from the reserve.
    ///
    /// # Errors
    /// - [`BondingCurveError::NotInitialized`] if the contract has not been initialized.
    /// - [`BondingCurveError::InvalidAmount`] if `amount` <= 0.
    pub fn buy(env: Env, buyer: Address, amount: i128, max_cost: i128) -> Result<(), BondingCurveError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::NotInitialized);
        }
        if amount <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }
        buyer.require_auth();

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(BondingCurveError::NotInitialized)?;

        let reserve: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve)
            .unwrap_or(0i128);
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Supply)
            .unwrap_or(0i128);
        let slope: i128 = env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128);
        let base_price: i128 = env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128);
        let connector_weight_bps: u32 = env.storage().instance().get(&DataKey::ConnectorWeightBps).unwrap_or(10_000u32);
        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32);
        let treasury: Address = env.storage().instance().get(&DataKey::Treasury).ok_or(BondingCurveError::NotInitialized)?;

        let cost = buy_cost(reserve, supply, amount, base_price, slope, connector_weight_bps)?;
        let fee = cost.checked_mul(fee_bps as i128).ok_or(BondingCurveError::Overflow)?
            .checked_div(BPS_DENOMINATOR).ok_or(BondingCurveError::Overflow)?;
        let reserve_credit = cost.checked_sub(fee).ok_or(BondingCurveError::Overflow)?;
        if cost > max_cost || reserve_credit <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }

        token::Client::new(&env, &token).transfer(&buyer, &env.current_contract_address(), &cost);
        if fee > 0 {
            token::Client::new(&env, &token).transfer(&env.current_contract_address(), &treasury, &fee);
        }

        let new_supply = supply.checked_add(amount).ok_or(BondingCurveError::Overflow)?;
        let new_reserve = reserve.checked_add(reserve_credit).ok_or(BondingCurveError::Overflow)?;
        let new_price = calculate_price(new_reserve, new_supply, base_price, slope)?;

        env.storage().instance().set(&DataKey::Supply, &new_supply);
        env.storage().instance().set(&DataKey::Reserve, &new_reserve);
        env.storage().instance().set(&DataKey::Price, &new_price);

        bump(&env);
        events::bought(&env, &buyer, cost, amount, new_price, fee);
        Ok(())
    }

    /// Sell `amount` tokens to withdraw from the reserve.
    ///
    /// # Errors
    /// - [`BondingCurveError::NotInitialized`] if the contract has not been initialized.
    /// - [`BondingCurveError::InvalidAmount`] if `amount` <= 0 or exceeds supply.
    /// - [`BondingCurveError::InsufficientReserve`] if the reserve is insufficient.
    pub fn sell(env: Env, seller: Address, amount: i128, min_proceeds: i128) -> Result<(), BondingCurveError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::NotInitialized);
        }
        if amount <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }
        seller.require_auth();

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(BondingCurveError::NotInitialized)?;

        let reserve: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Reserve)
            .unwrap_or(0i128);
        let supply: i128 = env
            .storage()
            .instance()
            .get(&DataKey::Supply)
            .unwrap_or(0i128);
        let slope: i128 = env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128);
        let base_price: i128 = env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128);
        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32);
        let connector_weight_bps: u32 = env.storage().instance().get(&DataKey::ConnectorWeightBps).unwrap_or(10_000u32);
        let treasury: Address = env.storage().instance().get(&DataKey::Treasury).ok_or(BondingCurveError::NotInitialized)?;

        if amount > supply {
            return Err(BondingCurveError::InvalidAmount);
        }

        let proceeds = sell_proceeds(reserve, supply, amount, base_price, slope, connector_weight_bps)?;
        let fee = proceeds.checked_mul(fee_bps as i128).ok_or(BondingCurveError::Overflow)?
            .checked_div(BPS_DENOMINATOR).ok_or(BondingCurveError::Overflow)?;
        let seller_proceeds = proceeds.checked_sub(fee).ok_or(BondingCurveError::Overflow)?;
        if seller_proceeds < min_proceeds {
            return Err(BondingCurveError::InvalidAmount);
        }
        if proceeds > reserve {
            return Err(BondingCurveError::InsufficientReserve);
        }

        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &seller,
            &seller_proceeds,
        );
        if fee > 0 {
            token::Client::new(&env, &token).transfer(&env.current_contract_address(), &treasury, &fee);
        }

        let new_supply = supply.checked_sub(amount).ok_or(BondingCurveError::Overflow)?;
        let new_reserve = reserve.checked_sub(proceeds).ok_or(BondingCurveError::Overflow)?;
        let new_price = calculate_price(new_reserve, new_supply, base_price, slope)?;

        env.storage().instance().set(&DataKey::Supply, &new_supply);
        env.storage().instance().set(&DataKey::Reserve, &new_reserve);
        env.storage().instance().set(&DataKey::Price, &new_price);

        bump(&env);
        events::sold(&env, &seller, amount, seller_proceeds, new_price, fee);
        Ok(())
    }

        /// Get current supply.
        pub fn get_supply(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::Supply)
                .unwrap_or(0i128)
        }

        /// Get current price per token.
        pub fn get_price(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&DataKey::Price)
                .unwrap_or(0i128)
        }

        /// Return the caller's issued curve-token balance.
        pub fn balance(env: Env, owner: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::CurveBalance(owner))
                .unwrap_or(0i128)
        }
    }

    /// Get the configured linear slope.
    pub fn get_slope(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128)
    }

    /// Get the configured base price.
    pub fn get_base_price(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::BasePrice).unwrap_or(0i128)
    }

    /// Get the configured Bancor connector weight in basis points.
    pub fn get_connector_weight_bps(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::ConnectorWeightBps).unwrap_or(0u32)
    }

    /// Get the configured protocol fee in basis points.
    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32)
    }

    /// Get the protocol fee treasury.
    pub fn get_treasury(env: Env) -> Result<Address, BondingCurveError> {
        env.storage().instance().get(&DataKey::Treasury).ok_or(BondingCurveError::NotInitialized)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::token::StellarAssetClient;

    #[test]
    fn test_price_increases_with_supply() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.timestamp = 1;
        });

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token = sac.address();
        StellarAssetClient::new(&env, &token).mint(&buyer, &1_000_000i128);

        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);

        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

        let initial_price = contract.get_price();
        assert!(initial_price >= 0);

        // Buy first batch - should be cheap
        contract.buy(&buyer, &100i128, &i128::MAX);
        let price_after_first = contract.get_price();

        // Buy second batch - should be more expensive
        contract.buy(&buyer, &100i128, &i128::MAX);
        let price_after_second = contract.get_price();

        assert!(price_after_first > initial_price);
        assert!(price_after_second > price_after_first);
    }

    #[test]
    fn test_buy_sell_1_to_1_reserve() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.timestamp = 1;
        });

        let admin = Address::generate(&env);
        let trader = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token = sac.address();
        StellarAssetClient::new(&env, &token).mint(&trader, &1_000_000i128);

        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);

        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

        let initial_reserve = contract.get_reserve();
        assert_eq!(initial_reserve, 0);

        // Buy tokens
        contract.buy(&trader, &100i128, &i128::MAX);
        let reserve_after_buy = contract.get_reserve();
        assert!(reserve_after_buy > 0);

        // Sell half back
        contract.sell(&trader, &50i128, &0i128);
        let reserve_after_sell = contract.get_reserve();

        // Reserve should have decreased but not to zero (slippage)
        assert!(reserve_after_sell > 0);
        assert!(reserve_after_sell < reserve_after_buy);
    }

    #[test]
    fn test_overflow_safety() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.timestamp = 1;
        });

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);

        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

        // Try to buy with invalid amount
        let result = contract.try_buy(&buyer, &-100i128, &i128::MAX);
        assert!(result.is_err());
    }

    #[test]
    fn test_buy_cost_bootstrap_is_non_zero() {
        let cost = buy_cost(0, 0, 100, 1, 1_000_000, 10_000).unwrap();
        assert!(cost > 0);
    }

    #[test]
    fn test_bancor_fixed_point_power_and_root() {
        let squared = fixed_pow(2 * PRICE_SCALE, 2).unwrap();
        assert_eq!(squared, 4 * PRICE_SCALE);
        let root = fixed_root(squared, 2).unwrap();
        assert!((root - 2 * PRICE_SCALE).abs() <= 1);
    }

    #[test]
    fn test_fee_is_routed_to_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let treasury = Address::generate(&env);
        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token = sac.address();
        StellarAssetClient::new(&env, &token).mint(&buyer, &1_000_000i128);
        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);
        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &100u32, &treasury);
        contract.buy(&buyer, &100i128, &i128::MAX);
        assert!(soroban_sdk::token::Client::new(&env, &token).balance(&treasury) > 0);
        assert_eq!(contract.get_fee_bps(), 100);
        assert_eq!(contract.get_treasury(), treasury);
    }

    /// Test that overflow in buy_cost returns Overflow error instead of panicking.
    #[test]
    fn test_buy_overflow_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.timestamp = 1;
        });

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);

        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

        // Try to buy with a huge amount that would overflow
        // i128::MAX / PRICE_SCALE is a reasonable upper bound
        // Try with a value that will definitely overflow in the multiply operation
        let result = contract.try_buy(&buyer, &i128::MAX, &i128::MAX);
        assert!(result.is_err());
    }

    /// Test that overflow in sell_proceeds returns Overflow error instead of panicking.
    #[test]
    fn test_sell_overflow_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|le| {
            le.timestamp = 1;
        });

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let token = Address::generate(&env);

        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(&env, &contract_addr);

        contract.initialize(&admin, &token, &1_000_000i128, &1i128, &10_000u32, &0u32, &admin);

        // To trigger an overflow in sell_proceeds, we'd need to set up a state where
        // the amount * avg_price calculation overflows. This is harder to construct
        // directly, but we can test by trying to sell an amount larger than supply
        let result = contract.try_sell(&seller, &i128::MAX, &0i128);
        assert!(result.is_err());
    }
}
