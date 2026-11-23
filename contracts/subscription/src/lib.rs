#![no_std]
#![deny(missing_docs)]
//! Recurring subscription payment contract template.
//!
//! A provider charges a subscriber a fixed amount per interval by pulling from
//! a pre-approved token allowance until the subscriber cancels. A subscriber may
//! hold several plans concurrently; each (subscriber, plan) pair is billed on its
//! own interval.

use soroban_sdk::{Address, Bytes, Env, Symbol, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::SubscriptionError;
pub use storage::{
    BatchChargeResult, ChargeOutcome, DEFAULT_GRACE_PERIOD_LEDGERS, DataKey, Plan,
    PRICE_NOTICE_LEDGERS, SubscriptionInfo, SubscriptionStatus,
};

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

/// Apply a pending referral voucher to the next recurring charge.
///
/// The active voucher is consumed once: after it has been applied the next
/// invoice is charged at the plan's full amount again.
fn discounted_charge(
    env: &Env,
    subscriber: &Address,
    amount: i128,
) -> Result<(i128, Option<(Bytes, u32)>), SubscriptionError> {
    let key = DataKey::ActiveVoucher(subscriber.clone());
    let Some(voucher) = env.storage().persistent().get::<_, (Bytes, u32)>(&key) else {
        return Ok((amount, None));
    };
    let keep_bps = BPS_DENOMINATOR.saturating_sub(voucher.1);
    let discounted = amount
        .checked_mul(i128::from(keep_bps))
        .and_then(|v| v.checked_div(i128::from(BPS_DENOMINATOR)))
        .ok_or(SubscriptionError::ArithmeticOverflow)?;
    Ok((discounted, Some(voucher)))
}

fn bump_subscription(env: &Env, subscriber: &Address, plan_id: &Symbol) {
    env.storage().persistent().extend_ttl(
        &DataKey::Subscription(subscriber.clone(), plan_id.clone()),
        LEDGER_LIFETIME_THRESHOLD,
        LEDGER_BUMP_AMOUNT,
    );
}

fn save_subscription(env: &Env, subscriber: &Address, info: &SubscriptionInfo) {
    env.storage().persistent().set(
        &DataKey::Subscription(subscriber.clone(), info.plan_id.clone()),
        info,
    );
    bump_subscription(env, subscriber, &info.plan_id);
}

fn grace_period(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::GracePeriod)
        .unwrap_or(DEFAULT_GRACE_PERIOD_LEDGERS)
}

/// Ledger at which the grace period of a delinquent subscription runs out, or
/// `None` when the subscription is not delinquent (issue #1485).
fn grace_period_end(info: &SubscriptionInfo, grace_period_ledgers: u32) -> Option<u32> {
    info.delinquent_since_ledger
        .map(|since| since.saturating_add(grace_period_ledgers))
}

/// Whether the subscriber may still use the service.
///
/// A failed charge does not revoke access: while the subscription is within
/// its grace period it stays usable and the charge can be retried. Access ends
/// only on cancellation, expiry, suspension, or once the grace period lapses.
fn access_active(info: &SubscriptionInfo, grace_period_ledgers: u32, current_ledger: u32) -> bool {
    if !info.active || info.suspended || info.paused || info.status == SubscriptionStatus::Expired {
        return false;
    }
    match grace_period_end(info, grace_period_ledgers) {
        Some(end) => current_ledger < end,
        None => true,
    }
}

fn is_registered_keeper(env: &Env, keeper: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Keeper(keeper.clone()))
        .unwrap_or(false)
}

fn keeper_bounty_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::KeeperBountyBps)
        .unwrap_or(0)
}

/// Share of `charged` paid to the keeper: `charged * bounty_bps / 10_000`.
fn keeper_bounty(charged: i128, bounty_bps: u32) -> Result<i128, SubscriptionError> {
    charged
        .checked_mul(i128::from(bounty_bps))
        .and_then(|v| v.checked_div(i128::from(BPS_DENOMINATOR)))
        .ok_or(SubscriptionError::ArithmeticOverflow)
}

/// Charge a single (subscriber, plan) subscription. Authorization is the
/// caller's responsibility. Returns the outcome and the amount transferred.
///
/// Payment failures are returned as `Ok(PaymentFailed | Suspended)` so that the
/// delinquency tracking is persisted; returning an error would roll it back.
fn charge_one(
    env: &Env,
    provider: &Address,
    token_client: &token::Client,
    grace_period_ledgers: u32,
    subscriber: &Address,
    plan_id: &Symbol,
) -> Result<(ChargeOutcome, i128), SubscriptionError> {
    let mut info: SubscriptionInfo = env
        .storage()
        .persistent()
        .get(&DataKey::Subscription(subscriber.clone(), plan_id.clone()))
        .ok_or(SubscriptionError::NotSubscribed)?;

    if info.suspended {
        return Err(SubscriptionError::SubscriptionSuspended);
    }
    if !info.active || info.status == SubscriptionStatus::Expired {
        return Err(SubscriptionError::SubscriptionInactive);
    }
    // A paused subscription is not billed: the caller is told so and no
    // deduction is made. The billing interval is frozen by `resume_subscription`.
    if info.paused {
        return Err(SubscriptionError::SubscriptionPaused);
    }

    // Fixed-term subscriptions stop billing once their cycle limit is reached.
    if let Some(limit) = info.max_billing_cycles {
        if info.billing_cycles >= limit {
            info.status = SubscriptionStatus::Expired;
            info.active = false;
            save_subscription(env, subscriber, &info);
            events::subscription_expired(env, subscriber, plan_id);
            return Ok((ChargeOutcome::Skipped, 0));
        }
    }

    let current_ledger = env.ledger().sequence();

    // Apply a scheduled price increase whose notice window has elapsed. The
    // increase takes effect for this subscriber at their first charge on or
    // after the effective ledger; the pending record is retained so every
    // existing subscriber migrates on their next charge.
    if let Some((new_amount, effective_ledger)) = env
        .storage()
        .persistent()
        .get::<_, (i128, u32)>(&DataKey::PendingIncrease(info.plan_id.clone()))
    {
        if current_ledger >= effective_ledger && new_amount > info.amount {
            info.amount = new_amount;
            let plan_key = DataKey::Plan(info.plan_id.clone());
            if let Some(mut plan) = env.storage().persistent().get::<_, Plan>(&plan_key) {
                if plan.amount < new_amount {
                    plan.amount = new_amount;
                    env.storage().persistent().set(&plan_key, &plan);
                }
            }
            events::price_increase_applied(env, &info.plan_id, new_amount);
        }
    }

    // Check if trial period is still active
    if !info.trial_completed {
        if current_ledger < info.last_charged_ledger.saturating_add(info.trial_ledgers) {
            return Err(SubscriptionError::IntervalNotElapsed);
        }
        // Trial period completed - mark as completed and start the first paid interval
        info.trial_completed = true;
        info.last_charged_ledger = current_ledger;
        save_subscription(env, subscriber, &info);
        events::trial_completed(env, subscriber, plan_id);
        return Ok((ChargeOutcome::TrialCompleted, 0));
    }

    // Normal billing period check. A delinquent subscription stays due, so the
    // provider can retry at any time during the grace period.
    if current_ledger
        < info
            .last_charged_ledger
            .saturating_add(info.interval_ledgers)
    {
        return Err(SubscriptionError::IntervalNotElapsed);
    }

    // `try_` isolates a failed transfer (insufficient allowance or balance) so
    // it can be recorded as a delinquency instead of aborting the transaction.
    // Soroban forbids contract re-entrancy, so writing state after the call is safe.
    let (charge_amount, applied_voucher) = discounted_charge(env, subscriber, info.amount)?;
    // Revenue is accumulated in contract escrow instead of being paid out on
    // every charge, avoiding a micro-transfer per billing cycle.
    let transfer = token_client.try_transfer_from(
        &env.current_contract_address(),
        subscriber,
        &env.current_contract_address(),
        &charge_amount,
    );

    if matches!(transfer, Ok(Ok(()))) {
        info.billing_cycles = info.billing_cycles.saturating_add(1);
        let revenue_key = DataKey::ProviderRevenue(provider.clone());
        let accumulated: i128 = env.storage().persistent().get(&revenue_key).unwrap_or(0);
        let new_revenue = accumulated
            .checked_add(charge_amount)
            .ok_or(SubscriptionError::ArithmeticOverflow)?;
        env.storage().persistent().set(&revenue_key, &new_revenue);
        if info.delinquent_since_ledger.is_some() {
            events::grace_period_recovered(env, subscriber, plan_id, info.failed_charges_count);
        }
        info.last_charged_ledger = current_ledger;
        info.failed_charges_count = 0;
        info.delinquent_since_ledger = None;
        save_subscription(env, subscriber, &info);
        if let Some((code, discount_bps)) = applied_voucher {
            env.storage()
                .persistent()
                .remove(&DataKey::ActiveVoucher(subscriber.clone()));
            events::voucher_applied(
                env,
                subscriber,
                &code,
                discount_bps,
                info.amount,
                charge_amount,
            );
        }
        events::charged(env, subscriber, provider, plan_id, charge_amount);
        return Ok((ChargeOutcome::Charged, charge_amount));
    }

    // A failed charge opens (or continues) the grace period: access stays
    // active and the charge may be retried until it lapses (issue #1485).
    info.failed_charges_count = info.failed_charges_count.saturating_add(1);
    let first_failure = info.delinquent_since_ledger.is_none();
    let delinquent_since = info.delinquent_since_ledger.unwrap_or(current_ledger);
    info.delinquent_since_ledger = Some(delinquent_since);
    if first_failure && grace_period_ledgers > 0 {
        events::grace_period_started(
            env,
            subscriber,
            plan_id,
            delinquent_since.saturating_add(grace_period_ledgers),
        );
    }

    if current_ledger >= delinquent_since.saturating_add(grace_period_ledgers) {
        info.active = false;
        info.suspended = true;
        save_subscription(env, subscriber, &info);
        events::subscription_suspended(env, subscriber, plan_id, info.failed_charges_count);
        return Ok((ChargeOutcome::Suspended, 0));
    }

    save_subscription(env, subscriber, &info);
    events::charge_failed(
        env,
        subscriber,
        plan_id,
        info.failed_charges_count,
        delinquent_since,
    );
    Ok((ChargeOutcome::PaymentFailed, 0))
}

/// Pro-rated terms for migrating a paid subscription to a new plan.
struct Proration {
    /// Value of the unused, already-paid part of the current interval.
    credit: i128,
    /// Amount to collect now (upgrade shortfall), or 0.
    charge: i128,
    /// `last_charged_ledger` for the new subscription.
    last_charged_ledger: u32,
}

/// Compute the pro-rated switch from `old` to `new_plan` at `current_ledger`.
///
/// `credit = old_amount * remaining / old_interval`. On an upgrade
/// (`credit < new_amount`) the shortfall is charged and a fresh interval starts
/// now; on a downgrade the credit buys `credit * new_interval / new_amount`
/// ledgers of the new plan before the next charge is due.
fn prorate(
    old: &SubscriptionInfo,
    new_plan: &Plan,
    current_ledger: u32,
) -> Result<Proration, SubscriptionError> {
    if old.delinquent_since_ledger.is_some() {
        return Err(SubscriptionError::PaymentOverdue);
    }
    let next_due = old
        .last_charged_ledger
        .checked_add(old.interval_ledgers)
        .ok_or(SubscriptionError::ArithmeticOverflow)?;
    let remaining = next_due.saturating_sub(current_ledger);
    if remaining == 0 {
        return Err(SubscriptionError::PaymentOverdue);
    }

    let credit = old
        .amount
        .checked_mul(i128::from(remaining))
        .and_then(|v| v.checked_div(i128::from(old.interval_ledgers)))
        .ok_or(SubscriptionError::ArithmeticOverflow)?;

    if credit < new_plan.amount {
        let charge = new_plan
            .amount
            .checked_sub(credit)
            .ok_or(SubscriptionError::ArithmeticOverflow)?;
        return Ok(Proration {
            credit,
            charge,
            last_charged_ledger: current_ledger,
        });
    }

    // The next charge falls due at `current_ledger + credit_ledgers`.
    let credit_ledgers = credit
        .checked_mul(i128::from(new_plan.interval_ledgers))
        .and_then(|v| v.checked_div(new_plan.amount))
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(SubscriptionError::ArithmeticOverflow)?;
    let last_charged_ledger = current_ledger
        .checked_add(credit_ledgers.saturating_sub(new_plan.interval_ledgers))
        .ok_or(SubscriptionError::ArithmeticOverflow)?;
    Ok(Proration {
        credit,
        charge: 0,
        last_charged_ledger,
    })
}

/// Recurring payment subscription contract.
///
/// Lifecycle:
/// 1. Deployer calls `initialize` to set the provider address and payment token.
/// 2. Subscribers call `subscribe` to register a recurring payment plan. A
///    subscriber may hold multiple distinct plans at the same time.
///    The subscriber must grant a token allowance to this contract so the provider
///    can pull payments: `token.approve(subscriber, this_contract, amount * N, expiry)`.
///    Subscribers may instead prepay several intervals upfront (at the plan's
///    discount), which is held in escrow and released to the provider per interval.
/// 3. For metered plans, the provider calls `report_usage` as usage accrues.
/// 4. Provider calls `charge(subscriber)` once per interval to collect
///    `base fee + usage_units * unit_price`; the usage meter then resets.
/// 5. Subscriber calls `cancel` to stop future charges and recover unconsumed prepaid funds.
/// 3. Provider calls `charge(subscriber, plan_id)` (or `charge_batch`) once per
///    interval to collect payment. Failed payments start a grace period; if still
///    unpaid when it expires, the subscription is suspended.
/// 4. Subscriber may call `change_plan` to migrate with pro-rated credit.
/// 5. Subscriber calls `cancel` to stop future charges.
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
        /// The grace period defaults to [`DEFAULT_GRACE_PERIOD_LEDGERS`] and can be
        /// changed with `set_grace_period`.
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
            env.storage()
                .instance()
                .set(&DataKey::GracePeriod, &DEFAULT_GRACE_PERIOD_LEDGERS);
            bump_instance(&env);

            events::initialized(&env, &provider, &token);
            Ok(())
        }

        /// Set the number of ledgers a delinquent subscription is tolerated before
        /// it is suspended. Only the provider can call this. A value of `0`
        /// suspends a subscription on its first failed charge.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        pub fn set_grace_period(
            env: Env,
            grace_period_ledgers: u32,
        ) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;

            provider.require_auth();

            env.storage()
                .instance()
                .set(&DataKey::GracePeriod, &grace_period_ledgers);
            bump_instance(&env);

            events::grace_period_updated(&env, grace_period_ledgers);
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
        /// A subscriber may hold several distinct plans concurrently; each is billed
        /// independently on its own interval.
        ///
        /// The subscriber must pre-approve this contract as a spender on the payment token
        /// before the provider can call `charge`. Concretely, the subscriber should call
        /// `token.approve(subscriber, subscription_contract, amount * periods, expiry_ledger)`
        /// with enough allowance to cover the desired number of billing periods across
        /// all plans they hold.
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
        /// Returns [`SubscriptionError::AlreadySubscribed`] if the subscriber already has an
        /// active subscription to this plan.
        pub fn subscribe(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
            trial_ledgers: Option<u32>,
            prepay_intervals: Option<u32>,
            max_billing_cycles: Option<u32>,
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

            let sub_key = DataKey::Subscription(subscriber.clone(), plan_id.clone());
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
            if max_billing_cycles == Some(0) {
                return Err(SubscriptionError::InvalidMaxBillingCycles);
            }
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
                suspended: false,
                failed_charges_count: 0,
                delinquent_since_ledger: None,
                billing_cycles: 0,
                max_billing_cycles,
                status: SubscriptionStatus::Active,
                paused: false,
                paused_at_ledger: None,
            };

            save_subscription(&env, &subscriber, &info);
            let index_key = DataKey::SubscriptionIndex;
            let mut index: Vec<(Address, Symbol)> = env
                .storage()
                .persistent()
                .get(&index_key)
                .unwrap_or(Vec::new(&env));
            index.push_back((subscriber.clone(), info.plan_id.clone()));
            env.storage().persistent().set(&index_key, &index);
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

        /// Register a referral discount voucher for `subscriber`.
        ///
        /// `discount_bps` is expressed in basis points (0..=10_000) and is
        /// deducted from the subscriber's next recurring invoice only. The
        /// voucher code is stored on-chain and may be queried later.
        pub fn apply_voucher(
            env: Env,
            subscriber: Address,
            voucher_code: Bytes,
            discount_bps: u32,
        ) -> Result<(), SubscriptionError> {
            subscriber.require_auth();
            if discount_bps > BPS_DENOMINATOR {
                return Err(SubscriptionError::InvalidVoucher);
            }

            env.storage().persistent().set(
                &DataKey::Voucher(subscriber.clone(), voucher_code.clone()),
                &discount_bps,
            );
            env.storage().persistent().set(
                &DataKey::ActiveVoucher(subscriber.clone()),
                &(voucher_code.clone(), discount_bps),
            );
            events::voucher_applied_to_subscriber(&env, &subscriber, &voucher_code, discount_bps);
            Ok(())
        }

        /// Returns the discount attached to `voucher_code`, if registered.
        pub fn get_voucher(env: Env, subscriber: Address, voucher_code: Bytes) -> Option<u32> {
            env.storage()
                .persistent()
                .get(&DataKey::Voucher(subscriber, voucher_code))
        }

        /// Returns the voucher pending on the subscriber's next invoice.
        pub fn get_active_voucher(env: Env, subscriber: Address) -> Option<(Bytes, u32)> {
            env.storage()
                .persistent()
                .get(&DataKey::ActiveVoucher(subscriber))
        }

        /// Provider pulls a recurring payment for one of a subscriber's plans.
        ///
        /// The interval since the last charge must have fully elapsed. If the token
        /// transfer fails (e.g. insufficient allowance or balance) the subscription
        /// becomes delinquent and `PaymentFailed` is returned; once the grace period
        /// since the first failed attempt has expired, a further failed attempt
        /// suspends the subscription and returns `Suspended`.
        /// Requires the subscriber to have an active subscription and to have granted
        /// sufficient allowance to this contract. The first period is charged as soon
        /// as the trial (if any) ends; each later period once its interval has elapsed.
        ///
        /// Missed periods are not forgiven: every period that has come due since the
        /// last paid one is collected in a single call, limited to what the current
        /// allowance covers. `last_charged_ledger` advances only by the periods
        /// actually paid, so any remainder can be collected later.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for the pair.
        /// Returns [`SubscriptionError::SubscriptionSuspended`] if the subscription was suspended.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the subscription was cancelled.
        /// Returns [`SubscriptionError::IntervalNotElapsed`] if the charge interval has not passed.
        pub fn charge(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<ChargeOutcome, SubscriptionError> {
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

            let token_client = token::Client::new(&env, &token_addr);
            let (outcome, _) = charge_one(
                &env,
                &provider,
                &token_client,
                grace_period(&env),
                &subscriber,
                &plan_id,
            )?;
            bump_instance(&env);
            Ok(outcome)
        }

        /// Provider charges many (subscriber, plan) subscriptions in one transaction.
        ///
        /// Entries that are not chargeable (not due, not subscribed, cancelled or
        /// suspended) are skipped, and failed payments are recorded as delinquencies,
        /// without reverting the successful charges in the same batch. The number of
        /// entries per call is bounded by the network's per-transaction resource limits.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotAuthorized`] if the caller is not the provider.
        pub fn charge_batch(
            env: Env,
            subscriptions: Vec<(Address, Symbol)>,
        ) -> Result<BatchChargeResult, SubscriptionError> {
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

            let token_client = token::Client::new(&env, &token_addr);
            let grace_period_ledgers = grace_period(&env);

            let mut result = BatchChargeResult {
                charged: 0,
                trials_completed: 0,
                failed: 0,
                suspended: 0,
                skipped: 0,
                total_charged: 0,
                outcomes: Vec::new(&env),
            };

            for (subscriber, plan_id) in subscriptions.iter() {
                let (outcome, amount) = charge_one(
                    &env,
                    &provider,
                    &token_client,
                    grace_period_ledgers,
                    &subscriber,
                    &plan_id,
                )
                .unwrap_or((ChargeOutcome::Skipped, 0));

                match outcome {
                    ChargeOutcome::Charged => {
                        result.charged = result.charged.saturating_add(1);
                        result.total_charged = result.total_charged.saturating_add(amount);
                    }
                    ChargeOutcome::TrialCompleted => {
                        result.trials_completed = result.trials_completed.saturating_add(1);
                    }
                    ChargeOutcome::PaymentFailed => {
                        result.failed = result.failed.saturating_add(1);
                    }
                    ChargeOutcome::Suspended => {
                        result.suspended = result.suspended.saturating_add(1);
                    }
                    ChargeOutcome::Skipped => {
                        result.skipped = result.skipped.saturating_add(1);
                    }
                }
                result.outcomes.push_back(outcome);
            }

            bump_instance(&env);
            events::batch_charged(&env, &provider, &result);
            Ok(result)
        }

        /// Migrate an active subscription from `old_plan_id` to `new_plan_id` with
        /// pro-rated billing.
        ///
        /// The unused, already-paid portion of the current interval is credited:
        /// `credit = old_amount * remaining_ledgers / old_interval`.
        /// - **Upgrade** (`credit < new_amount`): the difference `new_amount - credit`
        ///   is charged immediately and a fresh new-plan interval starts now.
        /// - **Downgrade** (`credit >= new_amount`): nothing is charged; the credit is
        ///   converted into new-plan time (`credit * new_interval / new_amount` ledgers)
        ///   before the next charge is due.
        ///
        /// A subscription still in its trial carries the remaining trial over to the
        /// new plan with no credit or charge.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for `old_plan_id`.
        /// Returns [`SubscriptionError::SubscriptionSuspended`] if the old subscription is suspended.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the old subscription was cancelled.
        /// Returns [`SubscriptionError::PlanNotFound`] if `new_plan_id` does not exist.
        /// Returns [`SubscriptionError::PlanInactive`] if `new_plan_id` is not active.
        /// Returns [`SubscriptionError::AlreadySubscribed`] if the subscriber already holds an
        /// active subscription to `new_plan_id` (including `new_plan_id == old_plan_id`).
        /// Returns [`SubscriptionError::PaymentOverdue`] if the current period is due or
        /// delinquent and must be charged before changing plans.
        /// Returns [`SubscriptionError::ArithmeticOverflow`] if the pro-ration overflows.
        pub fn change_plan(
            env: Env,
            subscriber: Address,
            old_plan_id: Symbol,
            new_plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
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

            subscriber.require_auth();

            let mut old: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&DataKey::Subscription(
                    subscriber.clone(),
                    old_plan_id.clone(),
                ))
                .ok_or(SubscriptionError::NotSubscribed)?;

            if old.suspended {
                return Err(SubscriptionError::SubscriptionSuspended);
            }
            if !old.active {
                return Err(SubscriptionError::SubscriptionInactive);
            }

            let new_plan: Plan = env
                .storage()
                .persistent()
                .get(&DataKey::Plan(new_plan_id.clone()))
                .ok_or(SubscriptionError::PlanNotFound)?;
            if !new_plan.active {
                return Err(SubscriptionError::PlanInactive);
            }

            if let Some(existing) =
                env.storage()
                    .persistent()
                    .get::<_, SubscriptionInfo>(&DataKey::Subscription(
                        subscriber.clone(),
                        new_plan_id.clone(),
                    ))
            {
                if existing.active {
                    return Err(SubscriptionError::AlreadySubscribed);
                }
            }

            let current_ledger = env.ledger().sequence();

            // A subscription still in trial has paid nothing: carry the trial over
            // with no credit or charge.
            let proration = if old.trial_completed {
                prorate(&old, &new_plan, current_ledger)?
            } else {
                Proration {
                    credit: 0,
                    charge: 0,
                    last_charged_ledger: old.last_charged_ledger,
                }
            };

            if proration.charge > 0 {
                token::Client::new(&env, &token_addr).transfer_from(
                    &env.current_contract_address(),
                    &subscriber,
                    &provider,
                    &proration.charge,
                );
            }

            let new_info = SubscriptionInfo {
                plan_id: new_plan_id.clone(),
                amount: new_plan.amount,
                interval_ledgers: new_plan.interval_ledgers,
                unit_price: new_plan.unit_price,
                usage_units: 0,
                prepaid_intervals_remaining: old.prepaid_intervals_remaining,
                prepaid_balance: old.prepaid_balance,
                trial_ledgers: old.trial_ledgers,
                trial_completed: old.trial_completed,
                last_charged_ledger: proration.last_charged_ledger,
                active: true,
                suspended: false,
                failed_charges_count: 0,
                delinquent_since_ledger: None,
                billing_cycles: old.billing_cycles,
                max_billing_cycles: old.max_billing_cycles,
                status: old.status,
                paused: false,
                paused_at_ledger: None,
            };

            old.active = false;
            save_subscription(&env, &subscriber, &old);
            save_subscription(&env, &subscriber, &new_info);
            bump_instance(&env);

            events::plan_changed(
                &env,
                &subscriber,
                &old_plan_id,
                &new_plan_id,
                proration.credit,
                proration.charge,
            );
            Ok(())
        }

        /// Provider reports metered usage for a subscriber plan.
        pub fn report_usage(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
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

            let key = DataKey::Subscription(subscriber.clone(), plan_id);
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
            bump_subscription(&env, &subscriber, &info.plan_id);
            bump_instance(&env);

            events::usage_reported(&env, &subscriber, units, info.usage_units);
            Ok(())
        }

        /// Subscriber cancels one of their subscriptions.
        pub fn cancel(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(SubscriptionError::NotInitialized)?;

            subscriber.require_auth();

            let key = DataKey::Subscription(subscriber.clone(), plan_id.clone());
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

            events::cancelled(&env, &subscriber, &plan_id);
            Ok(())
        }

        /// Pause a subscription's recurring billing without cancelling it.
        ///
        /// The subscriber keeps their plan configuration but is not charged while
        /// paused, and the billing interval is frozen: on
        /// [`Self::resume_subscription`] the paid period is shifted forward by the
        /// pause duration so the subscriber does not lose paid days. Service
        /// access ([`Self::has_access`]) is withdrawn while paused.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotSubscribed`] if the subscriber has no subscription.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the subscription is not active.
        /// Returns [`SubscriptionError::AlreadyPaused`] if it is already paused.
        pub fn pause_subscription(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
            subscriber.require_auth();

            let key = DataKey::Subscription(subscriber.clone(), plan_id.clone());
            let mut info: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::NotSubscribed)?;

            if !info.active {
                return Err(SubscriptionError::SubscriptionInactive);
            }
            if info.paused {
                return Err(SubscriptionError::AlreadyPaused);
            }

            info.paused = true;
            info.paused_at_ledger = Some(env.ledger().sequence());
            save_subscription(&env, &subscriber, &info);
            bump_instance(&env);

            events::paused(&env, &subscriber, &plan_id);
            Ok(())
        }

        /// Resume a paused subscription. The billing interval is shifted forward
        /// by the number of ledgers spent paused, so the paid period interrupted
        /// by the pause resumes where it left off and the subscriber is not
        /// charged for the paused period.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotSubscribed`] if the subscriber has no subscription.
        /// Returns [`SubscriptionError::NotPaused`] if the subscription is not paused.
        pub fn resume_subscription(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
            subscriber.require_auth();

            let key = DataKey::Subscription(subscriber.clone(), plan_id.clone());
            let mut info: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::NotSubscribed)?;

            if !info.paused {
                return Err(SubscriptionError::NotPaused);
            }

            let paused_for = info
                .paused_at_ledger
                .map(|at| env.ledger().sequence().saturating_sub(at))
                .unwrap_or(0);
            info.last_charged_ledger = info.last_charged_ledger.saturating_add(paused_for);
            info.paused = false;
            info.paused_at_ledger = None;
            save_subscription(&env, &subscriber, &info);
            bump_instance(&env);

            events::resumed(&env, &subscriber, &plan_id, paused_for);
            Ok(())
        }

        /// Returns the scheduled price increase pending for `plan_id`, if any:
        /// `(new_amount, effective_ledger)`. `None` once the increase has taken
        /// effect or no increase is scheduled.
        pub fn get_pending_price_increase(env: Env, plan_id: Symbol) -> Option<(i128, u32)> {
            env.storage()
                .persistent()
                .get(&DataKey::PendingIncrease(plan_id))
        }

        /// Return the subscription details for (`subscriber`, `plan_id`), or `None`
        /// if the subscriber never subscribed to that plan.
        pub fn get_subscription(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Option<SubscriptionInfo> {
            env.storage()
                .persistent()
                .get(&DataKey::Subscription(subscriber, plan_id))
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

        /// Return the grace period (in ledgers) applied to delinquent subscriptions.
        pub fn get_grace_period(env: Env) -> u32 {
            grace_period(&env)
        }

        /// Update a plan's base amount.
        ///
        /// A price **increase** does not take effect immediately: it is scheduled
        /// with a 30-day advance-notice window ([`PRICE_NOTICE_LEDGERS`]) before it
        /// takes effect, during which existing subscribers keep their current rate
        /// and may cancel without penalty. The new price applies to each subscriber
        /// at their first charge on or after the effective ledger, and to new
        /// subscribers once it has taken effect. A price **decrease** (or an
        /// unchanged amount) applies immediately for new subscribers and clears
        /// any pending increase.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::PlanNotFound`] if no plan exists with this ID.
        /// Returns [`SubscriptionError::InvalidAmount`] if `new_amount <= 0`.
        /// Returns [`SubscriptionError::ArithmeticOverflow`] if the effective ledger overflows.
        pub fn update_plan_amount(
            env: Env,
            plan_id: Symbol,
            new_amount: i128,
        ) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            provider.require_auth();

            if new_amount <= 0 {
                return Err(SubscriptionError::InvalidAmount);
            }

            let key = DataKey::Plan(plan_id.clone());
            let mut plan: Plan = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(SubscriptionError::PlanNotFound)?;

            if new_amount > plan.amount {
                // Increase: enforce the advance-notice window. The plan's amount
                // is left untouched until the window elapses (applied lazily on
                // the first eligible charge), so subscribers are never charged the
                // higher rate during the notice period.
                let effective_ledger = env
                    .ledger()
                    .sequence()
                    .checked_add(PRICE_NOTICE_LEDGERS)
                    .ok_or(SubscriptionError::ArithmeticOverflow)?;
                env.storage().persistent().set(
                    &DataKey::PendingIncrease(plan_id.clone()),
                    &(new_amount, effective_ledger),
                );
                events::price_increase_scheduled(
                    &env,
                    &plan_id,
                    plan.amount,
                    new_amount,
                    effective_ledger,
                );
            } else {
                // Decrease or unchanged: apply immediately for new subscribers.
                plan.amount = new_amount;
                env.storage().persistent().set(&key, &plan);
                env.storage()
                    .persistent()
                    .remove(&DataKey::PendingIncrease(plan_id.clone()));
                events::plan_amount_updated(&env, &plan_id, new_amount);
            }

            bump_instance(&env);
            Ok(())
        }

        /// Subscriber consents to a pending plan price increase, applying the new
        /// rate to their subscription immediately (ahead of the notice window that
        /// would otherwise auto-apply it on their next charge).
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotSubscribed`] if the subscriber has no subscription.
        /// Returns [`SubscriptionError::PlanNotFound`] if the plan does not exist.
        /// Returns [`SubscriptionError::InvalidAmount`] if no price increase is pending
        /// for this subscriber, or the new rate is not greater than their current rate.
        pub fn approve_price_increase(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
            subscriber.require_auth();

            let mut sub: SubscriptionInfo = env
                .storage()
                .persistent()
                .get(&DataKey::Subscription(subscriber.clone(), plan_id.clone()))
                .ok_or(SubscriptionError::NotSubscribed)?;

            let plan: Plan = env
                .storage()
                .persistent()
                .get(&DataKey::Plan(plan_id.clone()))
                .ok_or(SubscriptionError::PlanNotFound)?;

            // The rate a subscriber may approve is the pending scheduled increase
            // (if any), or the plan's current amount otherwise.
            let pending: Option<(i128, u32)> = env
                .storage()
                .persistent()
                .get(&DataKey::PendingIncrease(plan_id.clone()));
            let target = pending.map(|(amount, _)| amount).unwrap_or(plan.amount);

            if target <= sub.amount {
                return Err(SubscriptionError::InvalidAmount);
            }

            sub.amount = target;
            save_subscription(&env, &subscriber, &sub);
            env.storage().persistent().set(
                &DataKey::ApprovedAmount(subscriber, plan_id),
                &target,
            );
            bump_instance(&env);
            Ok(())
        }

        /// Returns true only if the subscription is due now and the subscriber has
        /// sufficient token balance and allowance for the expected charge.
        pub fn can_charge(env: Env, subscriber: Address, plan_id: Symbol) -> bool {
            let provider = match env.storage().instance().get::<_, Address>(&DataKey::Provider) {
                Some(p) => p,
                None => return false,
            };
            let token_addr = match env.storage().instance().get::<_, Address>(&DataKey::Token) {
                Some(t) => t,
                None => return false,
            };

            let info: SubscriptionInfo = match env
                .storage()
                .persistent()
                .get(&DataKey::Subscription(subscriber.clone(), plan_id))
            {
                Some(i) => i,
                None => return false,
            };

            if !info.active || info.suspended || info.paused {
                return false;
            }

            let current = env.ledger().sequence();
            if !info.trial_completed {
                if current < info.last_charged_ledger.saturating_add(info.trial_ledgers) {
                    return false;
                }
                // Trial completion path may not transfer tokens, but only if still in trial window we returned false above.
                // At/after trial end, charge can proceed.
            } else if current < info.last_charged_ledger.saturating_add(info.interval_ledgers) {
                return false;
            }

            // Estimate charge amount similar to charge_one base behavior.
            let mut amount = info.amount;
            if info.prepaid_intervals_remaining == 0 {
                let usage_fee = match i128::from(info.usage_units).checked_mul(info.unit_price) {
                    Some(v) => v,
                    None => return false,
                };
                amount = match amount.checked_add(usage_fee) {
                    Some(v) => v,
                    None => return false,
                };
            } else {
                // Prepaid base covered from escrow; usage fee (if any) pulled from allowance.
                amount = match i128::from(info.usage_units).checked_mul(info.unit_price) {
                    Some(v) => v,
                    None => return false,
                };
            }

            if amount <= 0 {
                // e.g. prepaid interval with no usage: transfer comes from contract escrow, not subscriber.
                return true;
            }

            let token_client = token::Client::new(&env, &token_addr);
            let balance = token_client.balance(&subscriber);
            let allowance = token_client.allowance(&subscriber, &env.current_contract_address());
            let _ = provider;
            balance >= amount && allowance >= amount
        }

        /// Set the minimum accumulated revenue required before the provider
        /// may withdraw. Provider-only.
        pub fn set_min_payout_threshold(
            env: Env,
            provider: Address,
            threshold: i128,
        ) -> Result<(), SubscriptionError> {
            provider.require_auth();
            if threshold <= 0 {
                return Err(SubscriptionError::InvalidPayoutThreshold);
            }
            env.storage()
                .persistent()
                .set(&DataKey::MinPayoutThreshold(provider), &threshold);
            Ok(())
        }

        /// Withdraw all accumulated provider revenue in a single payout once
        /// the configured threshold has been reached.
        pub fn withdraw_provider_revenue(
            env: Env,
            provider: Address,
        ) -> Result<i128, SubscriptionError> {
            provider.require_auth();
            let threshold: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::MinPayoutThreshold(provider.clone()))
                .ok_or(SubscriptionError::PayoutThresholdNotSet)?;
            let revenue_key = DataKey::ProviderRevenue(provider.clone());
            let revenue: i128 = env.storage().persistent().get(&revenue_key).unwrap_or(0);
            if revenue < threshold {
                return Err(SubscriptionError::BelowPayoutThreshold);
            }
            let token_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::Token)
                .ok_or(SubscriptionError::NotInitialized)?;
            if revenue > 0 {
                token::Client::new(&env, &token_addr).transfer(
                    &env.current_contract_address(),
                    &provider,
                    &revenue,
                );
            }
            env.storage().persistent().set(&revenue_key, &0i128);
            Ok(revenue)
        }

        /// Return the provider's currently accumulated (unpaid) revenue.
        pub fn get_provider_revenue(env: Env, provider: Address) -> i128 {
            env.storage()
                .persistent()
                .get(&DataKey::ProviderRevenue(provider))
                .unwrap_or(0)
        }

        /// Configure how many ledgers ahead of the due date subscriptions are
        /// reported to off-chain billing bots.
        pub fn set_notification_window(env: Env, window: u32) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            provider.require_auth();
            env.storage()
                .instance()
                .set(&DataKey::NotificationWindow, &window);
            Ok(())
        }

        /// Return active subscriptions whose next billing ledger falls inside
        /// the notification window, emitting `billing_due_soon` for each one.
        /// `max_results` bounds the work done per keeper call.
        pub fn get_due_subscriptions(
            env: Env,
            max_results: u32,
        ) -> Vec<(Address, Symbol, u32)> {
            let current = env.ledger().sequence();
            let window: u32 = env
                .storage()
                .instance()
                .get(&DataKey::NotificationWindow)
                .unwrap_or(0);
            let horizon = current.saturating_add(window);
            let index: Vec<(Address, Symbol)> = env
                .storage()
                .persistent()
                .get(&DataKey::SubscriptionIndex)
                .unwrap_or(Vec::new(&env));
            let mut due = Vec::new(&env);
            for (subscriber, plan_id) in index.iter() {
                if due.len() >= max_results {
                    break;
                }
                if let Some(info) = env.storage().persistent().get::<_, SubscriptionInfo>(
                    &DataKey::Subscription(subscriber.clone(), plan_id.clone()),
                ) {
                    let next_due = if info.trial_completed {
                        info.last_charged_ledger.saturating_add(info.interval_ledgers)
                    } else {
                        info.last_charged_ledger.saturating_add(info.trial_ledgers)
                    };
                    if info.active
                        && !info.suspended
                        && info.status == SubscriptionStatus::Active
                        && next_due <= horizon
                    {
                        events::billing_due_soon(
                            &env,
                            &subscriber,
                            &plan_id,
                            next_due,
                            current,
                        );
                        due.push_back((subscriber, plan_id, next_due));
                    }
                }
            }
            due
        }

        /// Return `true` while the subscriber may use the service for `plan_id`.
        ///
        /// Access stays active during the grace period that follows a failed
        /// charge, so a temporary payment problem does not lock the subscriber
        /// out while retries are pending (issue #1485). It ends once the
        /// subscription is cancelled, expired or suspended, or once the grace
        /// period has lapsed without a successful charge.
        pub fn has_access(env: Env, subscriber: Address, plan_id: Symbol) -> bool {
            env.storage()
                .persistent()
                .get::<_, SubscriptionInfo>(&DataKey::Subscription(subscriber, plan_id))
                .is_some_and(|info| {
                    access_active(&info, grace_period(&env), env.ledger().sequence())
                })
        }

        /// Return the ledger at which the current grace period ends, or `None`
        /// if the subscription does not exist or is not delinquent.
        pub fn get_grace_period_end(env: Env, subscriber: Address, plan_id: Symbol) -> Option<u32> {
            env.storage()
                .persistent()
                .get::<_, SubscriptionInfo>(&DataKey::Subscription(subscriber, plan_id))
                .and_then(|info| grace_period_end(&info, grace_period(&env)))
        }

        /// Register `keeper` as a bot allowed to call `charge_subscriber`.
        /// Only the provider can call this (issue #1484).
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        pub fn register_keeper(env: Env, keeper: Address) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            provider.require_auth();

            let key = DataKey::Keeper(keeper.clone());
            env.storage().persistent().set(&key, &true);
            env.storage().persistent().extend_ttl(
                &key,
                LEDGER_LIFETIME_THRESHOLD,
                LEDGER_BUMP_AMOUNT,
            );
            bump_instance(&env);

            events::keeper_registered(&env, &keeper);
            Ok(())
        }

        /// Revoke a keeper's permission to call `charge_subscriber`.
        /// Only the provider can call this.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::KeeperNotRegistered`] if `keeper` is not registered.
        pub fn remove_keeper(env: Env, keeper: Address) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            provider.require_auth();

            if !is_registered_keeper(&env, &keeper) {
                return Err(SubscriptionError::KeeperNotRegistered);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::Keeper(keeper.clone()));
            bump_instance(&env);

            events::keeper_removed(&env, &keeper);
            Ok(())
        }

        /// Set the share of each keeper-collected charge paid to the keeper as a
        /// gas bounty, in basis points of the charged amount (`0` disables it).
        /// The bounty is deducted from the provider's accumulated revenue.
        /// Only the provider can call this.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::InvalidBounty`] if `bounty_bps` > 10_000.
        pub fn set_keeper_bounty(env: Env, bounty_bps: u32) -> Result<(), SubscriptionError> {
            let provider: Address = env
                .storage()
                .instance()
                .get(&DataKey::Provider)
                .ok_or(SubscriptionError::NotInitialized)?;
            provider.require_auth();

            if bounty_bps > BPS_DENOMINATOR {
                return Err(SubscriptionError::InvalidBounty);
            }
            env.storage()
                .instance()
                .set(&DataKey::KeeperBountyBps, &bounty_bps);
            bump_instance(&env);

            events::keeper_bounty_updated(&env, bounty_bps);
            Ok(())
        }

        /// Return `true` if `keeper` is a registered keeper bot.
        pub fn is_keeper(env: Env, keeper: Address) -> bool {
            is_registered_keeper(&env, &keeper)
        }

        /// Return the keeper gas bounty in basis points of the charged amount.
        pub fn get_keeper_bounty(env: Env) -> u32 {
            keeper_bounty_bps(&env)
        }

        /// Registered keeper bot collects a due recurring charge for
        /// (`subscriber`, `plan_id`) on the provider's behalf (issue #1484).
        ///
        /// Billing rules are identical to [`SubscriptionContract::charge`]: the
        /// charge is only accepted once the billing interval has fully elapsed,
        /// so premature or duplicate calls are rejected with
        /// [`SubscriptionError::IntervalNotElapsed`]. Failed payments open the
        /// grace period instead of cancelling the subscription, and remain
        /// retryable until it lapses.
        ///
        /// On a successful charge the keeper receives `charged * bounty_bps /
        /// 10_000` from the billing fee (see `set_keeper_bounty`). If the bounty
        /// transfer fails it is skipped and the full fee stays with the provider,
        /// so a misbehaving keeper account can never block billing.
        ///
        /// # Errors
        ///
        /// Returns [`SubscriptionError::NotInitialized`] if the contract is not initialized.
        /// Returns [`SubscriptionError::KeeperNotRegistered`] if `keeper` is not registered.
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for the pair.
        /// Returns [`SubscriptionError::SubscriptionSuspended`] if the subscription was suspended.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if the subscription was cancelled.
        /// Returns [`SubscriptionError::IntervalNotElapsed`] if the charge interval has not passed.
        pub fn charge_subscriber(
            env: Env,
            keeper: Address,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<ChargeOutcome, SubscriptionError> {
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

            keeper.require_auth();
            if !is_registered_keeper(&env, &keeper) {
                return Err(SubscriptionError::KeeperNotRegistered);
            }

            let token_client = token::Client::new(&env, &token_addr);
            let (outcome, charged) = charge_one(
                &env,
                &provider,
                &token_client,
                grace_period(&env),
                &subscriber,
                &plan_id,
            )?;

            let mut bounty = 0i128;
            if outcome == ChargeOutcome::Charged && charged > 0 {
                let owed = keeper_bounty(charged, keeper_bounty_bps(&env))?;
                if owed > 0 {
                    let revenue_key = DataKey::ProviderRevenue(provider.clone());
                    let accumulated: i128 =
                        env.storage().persistent().get(&revenue_key).unwrap_or(0);
                    let remaining = accumulated
                        .checked_sub(owed)
                        .ok_or(SubscriptionError::ArithmeticOverflow)?;
                    let paid = token_client
                        .try_transfer(&env.current_contract_address(), &keeper, &owed)
                        .is_ok_and(|r| r.is_ok());
                    if paid {
                        env.storage().persistent().set(&revenue_key, &remaining);
                        bounty = owed;
                    }
                }
            }

            bump_instance(&env);
            events::keeper_charged(&env, &keeper, &subscriber, &plan_id, charged, bounty);
            Ok(outcome)
        }
    }
}

// #[cfg(test)]
// mod test;

// #[cfg(test)]
// mod prop_test;

#[cfg(test)]
mod issue_tests;

#[cfg(test)]
mod grace_keeper_tests;
