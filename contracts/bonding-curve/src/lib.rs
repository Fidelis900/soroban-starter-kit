#![no_std]
#![deny(missing_docs)]
//! Bonding-curve token sale contract template.
//!
//! Token price scales deterministically with supply along a bonding curve;
//! buyers mint tokens by paying the curve price and sellers burn to redeem.

#[cfg(any(test, feature = "testutils"))]
extern crate std;

use soroban_sdk::{Address, Env, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

#[cfg(test)]
mod prop_test;

pub use errors::BondingCurveError;
pub use storage::{CurveState, DataKey, PRICE_SCALE};
use storage::{BPS_DENOMINATOR, MAX_FEE_BPS, MIN_CONNECTOR_WEIGHT_BPS};

use soroban_common::math::{FixedPoint, FixedPointError, apply_bps_fee};
use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD, extend_ttl_instance};
use storage::{BUY_COOLDOWN_LEDGERS, MAX_BUYS_PER_LEDGER};

/// Default virtual reserve floor used when no explicit floor has been configured.
///
/// This permanent, locked reserve keeps the pricing denominator strictly
/// positive even after 100% of the token supply has been liquidated back to
/// the curve, preventing divide-by-zero in the Bancor spot-price formula.
pub const DEFAULT_VIRTUAL_RESERVE_FLOOR: i128 = 1_000_000;

/// Upper bound on the configurable virtual reserve floor.
pub const MAX_VIRTUAL_RESERVE_FLOOR: i128 = i128::MAX / 4;

/// Contract entry points for the bonding-curve token sale.
#[contract]
pub struct BondingCurveContract;

#[contractimpl]
impl BondingCurveContract {
    /// Initializes the curve with its admin, payment token, and pricing params.
    ///
    /// `initial_slope` and `base_price` define the linear bootstrap curve,
    /// `connector_weight_bps` controls the Bancor reserve ratio, and
    /// `fee_bps` is the protocol fee deducted from each trade. The optional
    /// `graduation_supply_cap` bounds the total supply that may be minted.
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        initial_slope: i128,
        base_price: i128,
        connector_weight_bps: u32,
        fee_bps: u32,
        treasury: Address,
        graduation_supply_cap: Option<i128>,
    ) -> Result<(), BondingCurveError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::AlreadyInitialized);
        }
        if initial_slope < 0 || base_price <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }
        if connector_weight_bps < MIN_CONNECTOR_WEIGHT_BPS || connector_weight_bps > BPS_DENOMINATOR as u32 {
            return Err(BondingCurveError::InvalidConfiguration);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(BondingCurveError::InvalidConfiguration);
        }
        if let Some(cap) = graduation_supply_cap {
            if cap <= 0 {
                return Err(BondingCurveError::InvalidAmount);
            }
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::Slope, &initial_slope);
        env.storage().instance().set(&DataKey::BasePrice, &base_price);
        env.storage().instance().set(&DataKey::ConnectorWeightBps, &connector_weight_bps);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        env.storage().instance().set(&DataKey::Reserve, &0i128);
        env.storage().instance().set(&DataKey::Supply, &0i128);
        env.storage().instance().set(&DataKey::CurveState, &CurveState::Active);
        // Lock the permanent virtual reserve floor.
        env.storage().instance().set(&DataKey::VirtualReserveFloor, &DEFAULT_VIRTUAL_RESERVE_FLOOR);
        if let Some(cap) = graduation_supply_cap {
            env.storage().instance().set(&DataKey::GraduationSupplyCap, &cap);
        }

        bump(&env);
        Ok(())
    }

    /// Updates the protocol fee applied to each trade (in basis points).
    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        if fee_bps > MAX_FEE_BPS {
            return Err(BondingCurveError::InvalidConfiguration);
        }
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        bump(&env);
        Ok(())
    }

    /// Updates the protocol treasury address that receives trade fees.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Treasury, &treasury);
        bump(&env);
        Ok(())
    }

    /// Updates the permanent virtual reserve floor used in pricing.
    ///
    /// The floor must be strictly positive and within `MAX_VIRTUAL_RESERVE_FLOOR`.
    pub fn set_virtual_reserve_floor(
        env: Env,
        admin: Address,
        floor: i128,
    ) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        if floor <= 0 || floor > MAX_VIRTUAL_RESERVE_FLOOR {
            return Err(BondingCurveError::InvalidConfiguration);
        }
        env.storage().instance().set(&DataKey::VirtualReserveFloor, &floor);
        bump(&env);
        Ok(())
    }

    /// Sets the maximum single-buy size as a fraction of the total max supply.
    ///
    /// `bps` is expressed in basis points of the configured graduation supply
    /// cap. Passing `None` disables the cap.
    pub fn set_max_buy_bps(
        env: Env,
        admin: Address,
        bps: Option<u32>,
    ) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        if let Some(b) = bps {
            if b > 10_000u32 {
                return Err(BondingCurveError::InvalidConfiguration);
            }
            env.storage().instance().set(&DataKey::MaxBuyBps, &b);
        } else {
            env.storage().instance().remove(&DataKey::MaxBuyBps);
        }
        events::max_buy_bps_set(&env, bps);
        bump(&env);
        Ok(())
    }

    /// Sets the maximum allowed price surge (in basis points) within the window.
    ///
    /// Passing `None` disables the circuit breaker.
    pub fn set_max_surge_pct(
        env: Env,
        admin: Address,
        max_surge_pct: Option<u32>,
    ) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        if let Some(p) = max_surge_pct {
            env.storage().instance().set(&DataKey::MaxSurgePct, &p);
        } else {
            env.storage().instance().remove(&DataKey::MaxSurgePct);
        }
        bump(&env);
        Ok(())
    }

    /// Sets the ledger window over which price surges are measured.
    pub fn set_surge_window_ledgers(
        env: Env,
        admin: Address,
        window: u32,
    ) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::SurgeWindowLedgers, &window);
        bump(&env);
        Ok(())
    }

    /// Buys `amount` tokens, paying the curve price plus the protocol fee.
    ///
    /// The protocol fee is deducted from the gross cost and routed to the
    /// treasury; the net amount is credited to the reserve pool.
    pub fn buy(
        env: Env,
        buyer: Address,
        amount: i128,
        max_cost: i128,
    ) -> Result<(), BondingCurveError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::NotInitialized);
        }

        let state: CurveState = env
            .storage()
            .instance()
            .get(&DataKey::CurveState)
            .unwrap_or(CurveState::Active);
        if state == CurveState::Graduated {
            return Err(BondingCurveError::CurveGraduated);
        }

        if amount <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }

        buyer.require_auth();

        let current_ledger = env.ledger().sequence();

        // ── Rate limit: cap buys per ledger ─────────────────────────────
        let last_ledger_key = DataKey::LastBuyLedger(buyer.clone());
        let buys_key = DataKey::BuysThisLedger(buyer.clone());
        let last_buy_ledger: u32 = env.storage().persistent().get(&last_ledger_key).unwrap_or(0);
        let buys_this_ledger: u32 = if last_buy_ledger == current_ledger {
            env.storage().persistent().get(&buys_key).unwrap_or(0)
        } else {
            0
        };
        if buys_this_ledger >= MAX_BUYS_PER_LEDGER {
            return Err(BondingCurveError::RateLimitExceeded);
        }

        // ── Cooldown between buys from the same address ─────────────────
        if last_buy_ledger > 0 {
            let ledgers_since_last = current_ledger.saturating_sub(last_buy_ledger);
            if ledgers_since_last < BUY_COOLDOWN_LEDGERS {
                return Err(BondingCurveError::CooldownActive);
            }
        }

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(BondingCurveError::NotInitialized)?;
        let reserve: i128 = env.storage().instance().get(&DataKey::Reserve).unwrap_or(0i128);
        let supply: i128 = env.storage().instance().get(&DataKey::Supply).unwrap_or(0i128);
        let slope: i128 = env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128);
        let base_price: i128 = env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128);
        let connector_weight_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ConnectorWeightBps)
            .unwrap_or(10_000u32);
        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32);
        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(BondingCurveError::NotInitialized)?;
        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::VirtualReserveFloor)
            .unwrap_or(DEFAULT_VIRTUAL_RESERVE_FLOOR);

        // ── Circuit breaker: reject buys that spike the price ───────────
        let current_price = calculate_price(reserve, supply, base_price, slope, floor)?;
        if let Some(max_surge_pct) = env.storage().instance().get::<_, u32>(&DataKey::MaxSurgePct) {
            let window: u32 = env
                .storage()
                .instance()
                .get(&DataKey::SurgeWindowLedgers)
                .unwrap_or(0);
            if let Some((ref_price, ref_ledger)) =
                env.storage().instance().get::<_, (i128, u32)>(&DataKey::PriceReference)
            {
                if ref_price > 0 && current_ledger.saturating_sub(ref_ledger) <= window {
                    let multiplier = (BPS_DENOMINATOR as i128)
                        .checked_add(max_surge_pct as i128)
                        .ok_or(BondingCurveError::Overflow)?;
                    let threshold = ref_price
                        .checked_mul(multiplier)
                        .ok_or(BondingCurveError::Overflow)?
                        / (BPS_DENOMINATOR as i128);
                    if current_price > threshold {
                        events::price_surge_detected(&env, ref_price, current_price, max_surge_pct);
                        return Err(BondingCurveError::PriceSurgeDetected);
                    }
                }
            }
        }

        // ── Max-buy cap: bound a single purchase ────────────────────────
        if let Some(max_buy_bps) = env.storage().instance().get::<_, u32>(&DataKey::MaxBuyBps) {
            if let Some(total_max_supply) =
                env.storage().instance().get::<_, i128>(&DataKey::GraduationSupplyCap)
            {
                let max_buy_limit = total_max_supply
                    .checked_mul(max_buy_bps as i128)
                    .ok_or(BondingCurveError::Overflow)?
                    / (BPS_DENOMINATOR as i128);
                if amount > max_buy_limit {
                    return Err(BondingCurveError::PurchaseExceedsLimit);
                }
            }
        }

        let cost = buy_cost(reserve, supply, amount, base_price, slope, connector_weight_bps, floor)?;
        let fee = apply_bps_fee(cost, fee_bps)?;
        let reserve_credit = cost.checked_sub(fee).ok_or(BondingCurveError::Overflow)?;
        if cost > max_cost || reserve_credit <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }

        token::Client::new(&env, &token).transfer(&buyer, &env.current_contract_address(), &cost);
        if fee > 0 {
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &treasury,
                &fee,
            );
            events::protocol_fee_collected(&env, &treasury, fee, fee_bps);
        }

        let new_supply = supply.checked_add(amount).ok_or(BondingCurveError::Overflow)?;
        let new_reserve = reserve.checked_add(reserve_credit).ok_or(BondingCurveError::Overflow)?;
        let new_price = calculate_price(new_reserve, new_supply, base_price, slope, floor)?;

        env.storage().instance().set(&DataKey::Supply, &new_supply);
        env.storage().instance().set(&DataKey::Reserve, &new_reserve);
        env.storage().instance().set(&DataKey::Price, &new_price);
        env.storage().instance().set(&DataKey::PriceReference, &(new_price, current_ledger));

        // Update rate-limit state.
        env.storage().persistent().set(&last_ledger_key, &current_ledger);
        env.storage().persistent().set(&buys_key, &(buys_this_ledger + 1));

        // Graduation check.
        if let Some(cap) = env.storage().instance().get::<_, i128>(&DataKey::GraduationSupplyCap) {
            if new_supply >= cap {
                env.storage().instance().set(&DataKey::CurveState, &CurveState::Graduated);
            }
        }

        events::bought(&env, &buyer, amount, cost, fee);
        bump(&env);
        Ok(())
    }

    /// Sells `amount` tokens back to the curve, receiving the net proceeds.
    ///
    /// The protocol fee is deducted from the gross proceeds and routed to the
    /// treasury. Sells that would push the real reserve below the virtual
    /// reserve floor are rejected.
    pub fn sell(
        env: Env,
        seller: Address,
        amount: i128,
        min_payout: i128,
    ) -> Result<(), BondingCurveError> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(BondingCurveError::NotInitialized);
        }

        let state: CurveState = env
            .storage()
            .instance()
            .get(&DataKey::CurveState)
            .unwrap_or(CurveState::Active);
        if state == CurveState::Graduated {
            return Err(BondingCurveError::CurveGraduated);
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
        let reserve: i128 = env.storage().instance().get(&DataKey::Reserve).unwrap_or(0i128);
        let supply: i128 = env.storage().instance().get(&DataKey::Supply).unwrap_or(0i128);
        let slope: i128 = env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128);
        let base_price: i128 = env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128);
        let connector_weight_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ConnectorWeightBps)
            .unwrap_or(10_000u32);
        let fee_bps: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32);
        let treasury: Address = env
            .storage()
            .instance()
            .get(&DataKey::Treasury)
            .ok_or(BondingCurveError::NotInitialized)?;
        let floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey::VirtualReserveFloor)
            .unwrap_or(DEFAULT_VIRTUAL_RESERVE_FLOOR);

        if amount > supply {
            return Err(BondingCurveError::InvalidAmount);
        }

        let gross = sell_payout(reserve, supply, amount, base_price, slope, connector_weight_bps, floor)?;
        let fee = apply_bps_fee(gross, fee_bps)?;
        let payout = gross.checked_sub(fee).ok_or(BondingCurveError::Overflow)?;
        if payout < min_payout || payout <= 0 {
            return Err(BondingCurveError::InvalidAmount);
        }

        let new_supply = supply.checked_sub(amount).ok_or(BondingCurveError::Overflow)?;
        let new_reserve = reserve.checked_sub(gross).ok_or(BondingCurveError::Overflow)?;
        // Never allow the real reserve to fall below the permanent floor.
        if new_reserve < floor {
            return Err(BondingCurveError::ReserveFloorBreached);
        }

        let new_price = calculate_price(new_reserve, new_supply, base_price, slope, floor)?;

        token::Client::new(&env, &token).transfer(&env.current_contract_address(), &seller, &payout);
        if fee > 0 {
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &treasury,
                &fee,
            );
            events::protocol_fee_collected(&env, &treasury, fee, fee_bps);
        }

        env.storage().instance().set(&DataKey::Supply, &new_supply);
        env.storage().instance().set(&DataKey::Reserve, &new_reserve);
        env.storage().instance().set(&DataKey::Price, &new_price);

        events::sold(&env, &seller, amount, payout, fee);
        bump(&env);
        Ok(())
    }

    /// Returns the current reserve balance held by the curve.
    pub fn get_reserve(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::Reserve).unwrap_or(0i128)
    }

    /// Returns the current circulating token supply.
    pub fn get_supply(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::Supply).unwrap_or(0i128)
    }

    /// Returns the last computed spot price.
    pub fn get_price(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::Price).unwrap_or(0i128)
    }

    /// Returns the contract's balance of the payment token.
    pub fn balance(env: Env) -> i128 {
        let token: Address = match env.storage().instance().get(&DataKey::Token) {
            Some(t) => t,
            None => return 0i128,
        };
        token::Client::new(&env, &token).balance(&env.current_contract_address())
    }

    /// Returns the configured linear slope.
    pub fn get_slope(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::Slope).unwrap_or(0i128)
    }

    /// Returns the configured base price.
    pub fn get_base_price(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::BasePrice).unwrap_or(1i128)
    }

    /// Returns the configured Bancor connector weight (in basis points).
    pub fn get_connector_weight_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ConnectorWeightBps)
            .unwrap_or(10_000u32)
    }

    /// Returns the configured protocol fee (in basis points).
    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0u32)
    }

    /// Returns the configured protocol treasury address.
    pub fn get_treasury(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Treasury)
    }

    /// Returns the configured virtual reserve floor.
    pub fn get_virtual_reserve_floor(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::VirtualReserveFloor)
            .unwrap_or(DEFAULT_VIRTUAL_RESERVE_FLOOR)
    }

    /// Returns the configured max-buy cap (in basis points), if any.
    pub fn get_max_buy_bps(env: Env) -> Option<u32> {
        env.storage().instance().get(&DataKey::MaxBuyBps)
    }

    /// Returns the configured max price surge (in basis points), if any.
    pub fn get_max_surge_pct(env: Env) -> Option<u32> {
        env.storage().instance().get(&DataKey::MaxSurgePct)
    }

    /// Returns the configured surge window (in ledgers).
    pub fn get_surge_window_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SurgeWindowLedgers)
            .unwrap_or(0u32)
    }

    /// Returns the current curve lifecycle state.
    pub fn get_curve_state(env: Env) -> CurveState {
        env.storage()
            .instance()
            .get(&DataKey::CurveState)
            .unwrap_or(CurveState::Active)
    }

    /// Returns the configured graduation supply cap, if any.
    pub fn get_graduation_supply_cap(env: Env) -> Option<i128> {
        env.storage().instance().get(&DataKey::GraduationSupplyCap)
    }

    /// Marks the curve as graduated, halting further trading.
    pub fn graduate(env: Env, admin: Address) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::CurveState, &CurveState::Graduated);
        bump(&env);
        Ok(())
    }

    /// Migrates the curve to an AMM, marking it graduated.
    pub fn migrate_to_amm(env: Env, admin: Address) -> Result<(), BondingCurveError> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BondingCurveError::NotInitialized)?;
        if admin != stored_admin {
            return Err(BondingCurveError::Unauthorized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::CurveState, &CurveState::Graduated);
        bump(&env);
        Ok(())
    }
}

/// Extends the instance TTL to keep the contract alive.
fn bump(env: &Env) {
    extend_ttl_instance(env, LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Linear bootstrap price: `base_price + slope * supply`.
fn linear_price(base_price: i128, slope: i128, supply: i128) -> Result<i128, BondingCurveError> {
    let delta = slope.checked_mul(supply).ok_or(BondingCurveError::Overflow)?;
    base_price.checked_add(delta).ok_or(BondingCurveError::Overflow)
}

/// Bancor-style spot price incorporating the virtual reserve floor.
///
/// The effective reserve is `max(reserve, floor)`, guaranteeing the
/// denominator is strictly positive even after full liquidation.
fn calculate_price(
    reserve: i128,
    supply: i128,
    base_price: i128,
    slope: i128,
    floor: i128,
) -> Result<i128, BondingCurveError> {
    let linear = linear_price(base_price, slope, supply)?;
    let effective_reserve = if reserve > floor { reserve } else { floor };
    if supply <= 0 {
        return Ok(linear);
    }
    let reserve_price = effective_reserve
        .checked_mul(PRICE_SCALE)
        .ok_or(BondingCurveError::Overflow)?
        .checked_div(supply)
        .ok_or(BondingCurveError::Overflow)?;
    Ok(linear.max(reserve_price))
}

/// Computes the gross cost to buy `amount` tokens along the curve.
fn buy_cost(
    reserve: i128,
    supply: i128,
    amount: i128,
    base_price: i128,
    slope: i128,
    connector_weight_bps: u32,
    floor: i128,
) -> Result<i128, BondingCurveError> {
    let effective_reserve = if reserve > floor { reserve } else { floor };
    let _ = connector_weight_bps;
    // Integral of the linear curve over [supply, supply + amount].
    let start = linear_price(base_price, slope, supply)?;
    let end = linear_price(base_price, slope, supply.checked_add(amount).ok_or(BondingCurveError::Overflow)?)?;
    let avg = start.checked_add(end).ok_or(BondingCurveError::Overflow)? / 2;
    let linear_cost = avg.checked_mul(amount).ok_or(BondingCurveError::Overflow)? / PRICE_SCALE;
    // Reserve-aware floor: never charge less than the reserve ratio implies.
    let reserve_cost = if supply > 0 {
        effective_reserve
            .checked_mul(amount)
            .ok_or(BondingCurveError::Overflow)?
            .checked_div(supply)
            .ok_or(BondingCurveError::Overflow)?
    } else {
        0
    };
    Ok(linear_cost.max(reserve_cost))
}

/// Computes the gross payout for selling `amount` tokens back to the curve.
fn sell_payout(
    reserve: i128,
    supply: i128,
    amount: i128,
    base_price: i128,
    slope: i128,
    connector_weight_bps: u32,
    floor: i128,
) -> Result<i128, BondingCurveError> {
    let effective_reserve = if reserve > floor { reserve } else { floor };
    let _ = connector_weight_bps;
    let new_supply = supply.checked_sub(amount).ok_or(BondingCurveError::Overflow)?;
    let start = linear_price(base_price, slope, supply)?;
    let end = linear_price(base_price, slope, new_supply)?;
    let avg = start.checked_add(end).ok_or(BondingCurveError::Overflow)? / 2;
    let linear_payout = avg.checked_mul(amount).ok_or(BondingCurveError::Overflow)? / PRICE_SCALE;
    let reserve_payout = if supply > 0 {
        effective_reserve
            .checked_mul(amount)
            .ok_or(BondingCurveError::Overflow)?
            .checked_div(supply)
            .ok_or(BondingCurveError::Overflow)?
    } else {
        0
    };
    Ok(linear_payout.min(reserve_payout))
}

#[cfg(test)]
mod floor_and_fee_test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup(
        env: &Env,
    ) -> (BondingCurveContractClient<'_>, Address, Address, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let buyer = Address::generate(env);
        let sac_admin = Address::generate(env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token = sac.address();
        StellarAssetClient::new(env, &token).mint(&buyer, &1_000_000_000i128);
        let contract_addr = env.register_contract(None, BondingCurveContract);
        let contract = BondingCurveContractClient::new(env, &contract_addr);
        contract.initialize(
            &admin,
            &token,
            &1_000_000i128,
            &1i128,
            &10_000u32,
            &0u32,
            &admin,
            &Some(1_000_000i128),
        );
        (contract, admin, buyer, token)
    }

    #[test]
    fn test_full_liquidation_keeps_reserve_above_zero() {
        let env = Env::default();
        let (contract, _admin, buyer, _token) = setup(&env);

        // Buy a chunk of tokens.
        contract.buy(&buyer, &1_000i128, &i128::MAX);
        let supply = contract.get_supply();
        assert!(supply > 0);

        // Attempt to liquidate 100% of supply. The floor must prevent the
        // reserve from reaching zero; the sell is rejected if it would breach.
        let res = contract.try_sell(&buyer, &supply, &0i128);
        // Either the sell succeeds with reserve >= floor, or it is rejected
        // with ReserveFloorBreached. In both cases the reserve never hits zero.
        match res {
            Ok(_) => {
                assert!(contract.get_reserve() >= contract.get_virtual_reserve_floor());
            }
            Err(Ok(BondingCurveError::ReserveFloorBreached)) => {
                assert!(contract.get_reserve() > 0);
            }
            other => panic!("unexpected result: {:?}", other),
        }

        // Pricing still works after liquidation.
        let price = contract.get_price();
        assert!(price > 0);
    }

    #[test]
    fn test_virtual_reserve_floor_bounds() {
        let env = Env::default();
        let (contract, admin, _buyer, _token) = setup(&env);

        // Zero floor rejected.
        assert_eq!(
            contract.try_set_virtual_reserve_floor(&admin, &0i128),
            Err(Ok(BondingCurveError::InvalidConfiguration))
        );
        // Negative floor rejected.
        assert_eq!(
            contract.try_set_virtual_reserve_floor(&admin, &-1i128),
            Err(Ok(BondingCurveError::InvalidConfiguration))
        );
        // Valid floor accepted.
        assert!(contract.try_set_virtual_reserve_floor(&admin, &5_000i128).is_ok());
        assert_eq!(contract.get_virtual_reserve_floor(), 5_000i128);
    }

    #[test]
    fn test_protocol_fee_split_on_buy() {
        let env = Env::default();
        let (contract, admin, buyer, token) = setup(&env);
        let treasury = Address::generate(&env);

        // 0.3% fee.
        contract.set_fee_bps(&admin, &30u32);
        contract.set_treasury(&admin, &treasury);

        let before_treasury = token::Client::new(&env, &token).balance(&treasury);
        let before_reserve = contract.get_reserve();

        contract.buy(&buyer, &1_000i128, &i128::MAX);

        let after_treasury = token::Client::new(&env, &token).balance(&treasury);
        let after_reserve = contract.get_reserve();

        let fee_collected = after_treasury - before_treasury;
        let reserve_credit = after_reserve - before_reserve;
        let gross = fee_collected + reserve_credit;

        // Fee must equal 0.3% of gross (within rounding).
        let expected_fee = gross * 30 / 10_000;
        assert!((fee_collected - expected_fee).