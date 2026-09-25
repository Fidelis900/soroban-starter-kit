#![no_std]
#![deny(missing_docs)]
//! Recurring subscription payment contract template.
//!
//! A provider charges a subscriber a fixed amount per interval by pulling from
//! a pre-approved token allowance until the subscriber cancels.

use soroban_sdk::{Address, Env, Symbol, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::SubscriptionError;
pub use storage::{DataKey, Plan, SubscriptionInfo};

use soroban_common::{LEDGER_BUMP_AMOUNT, LEDGER_LIFETIME_THRESHOLD};

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LEDGER_LIFETIME_THRESHOLD, LEDGER_BUMP_AMOUNT);
}

/// Basis-point denominator for prepay discounts (`10_000` bps = 100%).
const BPS_DENOMINATOR: u32 = 10_000;

/// Upfront cost of prepaying `intervals` of `amount`, after applying `discount_bps`.
fn prepay_cost(amount: i128, intervals: u32, discount_bps: u32) -> Result<i128, SubscriptionError> {
    let keep_bps = BPS_DENOMINATOR.saturating_sub(discount_bps);
    amount
        .checked_mul(i128::from(intervals))
        .and_then(|gross| gross.checked_mul(i128::from(keep_bps)))
        .and_then(|scaled| scaled.checked_div(i128::from(BPS_DENOMINATOR)))
        .ok_or(SubscriptionError::ArithmeticOverflow)
}

fn bump_subscription(env: &Env, subscriber: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Subscription(subscriber.clone()),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

/// Recurring payment subscription contract.
///
/// Lifecycle:
/// 1. Deployer calls `initialize` to set the provider address and payment token.
/// 2. Subscribers call `subscribe` to register a recurring payment plan.
///    The subscriber must grant a token allowance to this contract so the provider
///    can pull payments: `token.approve(subscriber, this_contract, amount * N, expiry)`.
///    Subscribers may instead prepay several intervals upfront (at the plan's
///    discount), which is held in escrow and released to the provider per interval.
/// 3. For metered plans, the provider calls `report_usage` as usage accrues.
/// 4. Provider calls `charge(subscriber)` once per interval to collect
///    `base fee + usage_units * unit_price`; the usage meter then resets.
/// 5. Subscriber calls `cancel` to stop future charges and recover unconsumed prepaid funds.
pub use contract::*;

// The `#[contract]` / `#[contractimpl]` macros generate an undocumented public
// client type. Confine the missing_docs allowance to this module and re-export
// the public contract API above, keeping the rest of the crate enforced.
mod contract {
    #![allow(missing_docs)]
    use super::*;

    #[contract]
    pub struct SubscriptionContract;

    #[contractimpl]
    impl SubscriptionContract {
        /// Initialize the contract with a provider and payment token. Must be called exactly once.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::AlreadyInitialized`] if already initialized.
        pub fn initialize(
            env: Env,
            provider: Address,
            token: Address,
        ) -> Result<(), SubscriptionError> {
            if env.storage().instance().has(&DataKey::Provider) {
                return Err(SubscriptionError::AlreadyInitialized);
            }

            provider.require_auth();

            // Validate that token implements the token interface.
            token::Client::new(&env, &token).decimals();

            env.storage().instance().set(&DataKey::Provider, &provider);
            env.storage().instance().set(&DataKey::Token, &token);
            bump_instance(&env);

            events::initialized(&env, &provider, &token);
            Ok(())
        }

        /// Register a new subscription plan. Only the provider (admin) can call this.
        ///
        /// `amount` is the fixed base fee per interval. `unit_price` is charged per
        /// usage unit reported via `report_usage` (pass 0 for a flat-rate plan).
        /// `prepay_discount_bps` is the discount, in basis points, applied to the base
        /// fee when a subscriber prepays multiple intervals upfront (pass 0 for none).
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        /// Returns [`SubscriptionError::InvalidAmount`] if `amount` <= 0 or `unit_price` < 0.
        /// Returns [`SubscriptionError::InvalidInterval`] if `interval_ledgers` == 0.
        /// Returns [`SubscriptionError::InvalidDiscount`] if `prepay_discount_bps` > 10_000.
        /// Returns [`SubscriptionError::PlanAlreadyExists`] if a plan with this ID already exists.
        pub fn register_plan(
            env: Env,
            plan_id: Symbol,
            amount: i128,
            interval_ledgers: u32,
            unit_price: i128,
            prepay_discount_bps: u32,
        ) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;

            provider.require_auth();

            if amount <= 0 || unit_price < 0 {
                return Err(SubscriptionError::InvalidAmount);
            }
            if interval_ledgers == 0 {
                return Err(SubscriptionError::InvalidInterval);
            }
            if prepay_discount_bps > BPS_DENOMINATOR {
                return Err(SubscriptionError::InvalidDiscount);
            }

            let key = DataKey::Plan(plan_id.clone());
            if env.storage().persistent().has(&key) {
                return Err(SubscriptionError::PlanAlreadyExists);
            }

            let plan = Plan {
                plan_id: plan_id.clone(),
                amount,
                interval_ledgers,
                unit_price,
                prepay_discount_bps,
                active: true,
            };

            env.storage().persistent().set(&key, &plan);
            bump_instance(&env);

            events::plan_registered(&env, &plan_id, amount, interval_ledgers);
            Ok(())
        }

        /// Update an existing plan's status. Only the provider (admin) can call this.
        ///
        /// Deactivating a plan (`active = false`) only prevents **new subscriptions** from
        /// being created for this plan. Existing subscribers who are already subscribed to
        /// the plan will continue to be charged on their normal billing cycle. To stop
        /// billing for existing subscribers, they must call `cancel` individually.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        /// Returns [`SubscriptionError::PlanNotFound`] if no plan exists with this ID.
        pub fn set_plan_active(
            env: Env,
            plan_id: Symbol,
            active: bool,
        ) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;

            provider.require_auth();

            let key = DataKey::Plan(plan_id.clone());
            let mut plan: Plan = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::PlanNotFound)?;

            plan.active = active;
            env.storage().persistent().set(&key, &plan);
            bump_instance(&env);

            events::plan_updated(&env, &plan_id, active);
            Ok(())
        }

        /// Register a recurring payment subscription by selecting an existing plan.
        ///
        /// The subscriber must pre-approve this contract as a spender on the payment token
        /// before the provider can call `charge`. Concretely, the subscriber should call
        /// `token.approve(subscriber, subscription_contract, amount * periods, expiry_ledger)`
        /// with enough allowance to cover the desired number of billing periods.
        ///
        /// If `prepay_intervals` is `Some(n)` with `n > 0`, the base fee for `n` intervals
        /// (less the plan's `prepay_discount_bps`) is transferred upfront into contract
        /// escrow and released to the provider one interval at a time on each `charge`.
        /// Metered usage fees are still pulled from the subscriber's allowance. Any
        /// unconsumed prepaid balance is refunded on `cancel`.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::PlanNotFound`] if no plan exists with the provided ID.
        /// Returns [`SubscriptionError::PlanInactive`] if the selected plan is not active.
        /// Returns [`SubscriptionError::AlreadySubscribed`] if the subscriber already has an active plan.
        /// Returns [`SubscriptionError::ArithmeticOverflow`] if the prepay cost overflows.
        pub fn subscribe(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
            trial_ledgers: Option<u32>,
            prepay_intervals: Option<u32>,
        ) -> Result<(), SubscriptionError> {
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(SubscriptionError::NotInitialized)?;

            // Get the plan details
            let plan_key = DataKey::Plan(plan_id.clone());
            let plan: Plan = env
                .storage()
                .persistent()
                .get(&plan_key)
                .ok_or(SubscriptionError::PlanNotFound)?;

            if !plan.active {
                return Err(SubscriptionError::PlanInactive);
            }

            subscriber.require_auth();

            let sub_key = DataKey::Subscription(subscriber.clone());
            if let Some(existing) = env
                .storage()
                .persistent()
                .get::<_, SubscriptionInfo>(&sub_key)
            {
                if existing.active {
                    return Err(SubscriptionError::AlreadySubscribed);
                }
            }

            let trial_ledgers = trial_ledgers.unwrap_or(0);
            let prepay_intervals = prepay_intervals.unwrap_or(0);
            let prepaid_balance =
                prepay_cost(plan.amount, prepay_intervals, plan.prepay_discount_bps)?;

            let info = SubscriptionInfo {
                plan_id,
                amount: plan.amount,
                interval_ledgers: plan.interval_ledgers,
                unit_price: plan.unit_price,
                usage_units: 0,
                prepaid_intervals_remaining: prepay_intervals,
                prepaid_balance,
                trial_ledgers,
                trial_completed: trial_ledgers == 0,
                last_charged_ledger: env.ledger().sequence(),
                active: true,
            };

            env.storage().persistent().set(&sub_key, &info);
            bump_subscription(&env, &subscriber);
            bump_instance(&env);

            if prepaid_balance > 0 {
                token::Client::new(&env, &token_addr).transfer(
                    &subscriber,
                    &env.current_contract_address(),
                    &prepaid_balance,
                );
            }
            if prepay_intervals > 0 {
                events::prepaid(&env, &subscriber, prepay_intervals, prepaid_balance);
            }

            events::subscribed(
                &env,
                &subscriber,
                &info.plan_id,
                plan.amount,
                plan.interval_ledgers,
            );
            Ok(())
        }

        /// Provider pulls a recurring payment from a subscriber.
        ///
        /// Requires the subscriber to have an active subscription and to have granted
        /// sufficient allowance to this contract. The interval since the last charge
        /// must have fully elapsed.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for `subscriber`.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the subscription was cancelled.
        /// Returns [`SubscriptionError::IntervalNotElapsed`] if the charge interval has not passed.
        /// Returns [`SubscriptionError::InsufficientAllowance`] if the subscriber's token allowance is too low.
        pub fn charge(env: Env, subscriber: Address) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(SubscriptionError::NotInitialized)?;

            provider.require_auth();

            let key = DataKey::Subscription(subscriber.clone());
            let mut info: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::NotSubscribed)?;

            if !info.active {
                return Err(SubscriptionError::SubscriptionInactive);
            }

            let current_ledger = env.ledger().sequence();

            // Check if trial period is still active
            if !info.trial_completed {
                if current_ledger < info.last_charged_ledger + info.trial_ledgers {
                    return Err(SubscriptionError::IntervalNotElapsed);
                }
                // Trial period completed - mark as completed and update last charged ledger
                info.trial_completed = true;
                info.last_charged_ledger = current_ledger;
                env.storage().persistent().set(&key, &info);
                bump_subscription(&env, &subscriber);
                bump_instance(&env);
                events::trial_completed(&env, &subscriber);
                return Ok(());
            }

            // Normal billing period check
            if current_ledger < info.last_charged_ledger + info.interval_ledgers {
                return Err(SubscriptionError::IntervalNotElapsed);
            }

            let usage_fee = i128::from(info.usage_units)
                .checked_mul(info.unit_price)
                .ok_or(SubscriptionError::ArithmeticOverflow)?;

            // Base fee comes from prepaid escrow while intervals remain, otherwise
            // from the subscriber's allowance. Usage is always pulled from allowance.
            let (escrow_release, pull_amount) = if info.prepaid_intervals_remaining > 0 {
                // Spread rounding dust evenly; the final interval releases the remainder.
                let release = info
                    .prepaid_balance
                    .checked_div(i128::from(info.prepaid_intervals_remaining))
                    .ok_or(SubscriptionError::ArithmeticOverflow)?;
                (release, usage_fee)
            } else {
                let pull = info
                    .amount
                    .checked_add(usage_fee)
                    .ok_or(SubscriptionError::ArithmeticOverflow)?;
                (0, pull)
            };

            let total_charged = escrow_release
                .checked_add(pull_amount)
                .ok_or(SubscriptionError::ArithmeticOverflow)?;

            let token_client = token::Client::new(&env, &token_addr);

            if pull_amount > 0 {
                let allowance =
                    token_client.allowance(&subscriber, &env.current_contract_address());
                if allowance < pull_amount {
                    return Err(SubscriptionError::InsufficientAllowance);
                }
            }

            // checks-effects-interactions: update state before external call
            info.last_charged_ledger = current_ledger;
            info.usage_units = 0;
            if info.prepaid_intervals_remaining > 0 {
                info.prepaid_intervals_remaining -= 1;
                info.prepaid_balance -= escrow_release;
            }
            env.storage().persistent().set(&key, &info);
            bump_subscription(&env, &subscriber);
            bump_instance(&env);

            if escrow_release > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &provider,
                    &escrow_release,
                );
            }
            if pull_amount > 0 {
                token_client.transfer_from(
                    &env.current_contract_address(),
                    &subscriber,
                    &provider,
                    &pull_amount,
                );
            }

            events::charged(&env, &subscriber, &provider, total_charged);
            Ok(())
        }

        /// Provider reports metered usage for a subscriber.
        ///
        /// `units` are added to the subscriber's meter, which accumulates until the
        /// next successful `charge`. At that point `usage_units * unit_price` is billed
        /// on top of the base fee and the meter is reset to zero.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        /// Returns [`SubscriptionError::InvalidAmount`] if `units` == 0.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for `subscriber`.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the subscription was cancelled.
        /// Returns [`SubscriptionError::ArithmeticOverflow`] if the meter would overflow.
        pub fn report_usage(
            env: Env,
            subscriber: Address,
            units: u64,
        ) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;

            provider.require_auth();

            if units == 0 {
                return Err(SubscriptionError::InvalidAmount);
            }

            let key = DataKey::Subscription(subscriber.clone());
            let mut info: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::NotSubscribed)?;

            if !info.active {
                return Err(SubscriptionError::SubscriptionInactive);
            }

            info.usage_units = info
                .usage_units
                .checked_add(units)
                .ok_or(SubscriptionError::ArithmeticOverflow)?;
            env.storage().persistent().set(&key, &info);
            bump_subscription(&env, &subscriber);
            bump_instance(&env);

            events::usage_reported(&env, &subscriber, units, info.usage_units);
            Ok(())
        }

        /// Subscriber cancels their subscription. No further charges can be made.
        ///
        /// Any prepaid balance still held in escrow for unconsumed intervals is
        /// refunded to the subscriber.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for `subscriber`.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if already cancelled.
        pub fn cancel(env: Env, subscriber: Address) -> Result<(), SubscriptionError> {
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(SubscriptionError::NotInitialized)?;

            subscriber.require_auth();

            let key = DataKey::Subscription(subscriber.clone());
            let mut info: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::NotSubscribed)?;

            if !info.active {
                return Err(SubscriptionError::SubscriptionInactive);
            }

            let refund = info.prepaid_balance;
            let unconsumed = info.prepaid_intervals_remaining;

            info.active = false;
            info.usage_units = 0;
            info.prepaid_balance = 0;
            info.prepaid_intervals_remaining = 0;
            env.storage().persistent().set(&key, &info);
            bump_instance(&env);

            if refund > 0 {
                token::Client::new(&env, &token_addr).transfer(
                    &env.current_contract_address(),
                    &subscriber,
                    &refund,
                );
                events::prepaid_refunded(&env, &subscriber, unconsumed, refund);
            }

            events::cancelled(&env, &subscriber);
            Ok(())
        }

        /// Return the subscription details for `subscriber`, or `None` if not subscribed.
        pub fn get_subscription(env: Env, subscriber: Address) -> Option<SubscriptionInfo> {
            env.storage()
                .persistent()
                .get(&DataKey::Subscription(subscriber))
        }

        /// Return the provider address, or `None` if not initialized.
        pub fn get_provider(env: Env) -> Option<Address> {
            env.storage().instance().get(&DataKey::Provider)
        }

        /// Return the payment token address, or `None` if not initialized.
        pub fn get_token(env: Env) -> Option<Address> {
            env.storage().instance().get(&DataKey::Token)
        }

        /// Return the plan details for the given ID, or `None` if the plan doesn't exist.
        pub fn get_plan(env: Env, plan_id: Symbol) -> Option<Plan> {
            env.storage().persistent().get(&DataKey::Plan(plan_id))
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;
