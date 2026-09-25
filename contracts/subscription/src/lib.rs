#![no_std]
#![deny(missing_docs)]
//! Recurring subscription payment contract template.
//!
//! A provider charges a subscriber a fixed amount per interval by pulling from
//! a pre-approved token allowance until the subscriber cancels. A subscriber may
//! hold several plans concurrently; each (subscriber, plan) pair is billed on its
//! own interval.

use soroban_sdk::{Address, Env, Symbol, Vec, contract, contractimpl, token};

mod errors;
mod events;
mod storage;

pub use errors::SubscriptionError;
pub use storage::{
    BatchChargeResult, ChargeOutcome, DEFAULT_GRACE_PERIOD_LEDGERS, DataKey, Plan, SubscriptionInfo,
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

fn bump_subscription(env: &Env, subscriber: &Address) {
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
    if !info.active {
        return Err(SubscriptionError::SubscriptionInactive);
    }

    let current_ledger = env.ledger().sequence();

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
    let transfer = token_client.try_transfer_from(
        &env.current_contract_address(),
        subscriber,
        provider,
        &info.amount,
    );

    if matches!(transfer, Ok(Ok(()))) {
        info.last_charged_ledger = current_ledger;
        info.failed_charges_count = 0;
        info.delinquent_since_ledger = None;
        save_subscription(env, subscriber, &info);
        events::charged(env, subscriber, provider, plan_id, info.amount);
        return Ok((ChargeOutcome::Charged, info.amount));
    }

    info.failed_charges_count = info.failed_charges_count.saturating_add(1);
    let delinquent_since = info.delinquent_since_ledger.unwrap_or(current_ledger);
    info.delinquent_since_ledger = Some(delinquent_since);

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
            };

            save_subscription(&env, &subscriber, &info);
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
                trial_ledgers: old.trial_ledgers,
                trial_completed: old.trial_completed,
                last_charged_ledger: proration.last_charged_ledger,
                active: true,
                suspended: false,
                failed_charges_count: 0,
                delinquent_since_ledger: None,
            };

            old.active = false;
            save_subscription(&env, &subscriber, &old);
            save_subscription(&env, &subscriber, &new_info);
            // Ledger at which the next unpaid period starts. While in trial the
            // first period starts the moment the trial ends; afterwards each
            // period starts one interval after the last paid one.
            let next_due = if info.trial_completed {
                info.last_charged_ledger + info.interval_ledgers
            } else {
                info.last_charged_ledger + info.trial_ledgers
            };
            if current_ledger < next_due {
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
            // Every period that has started since `next_due` is owed, including
            // any missed while the subscriber lacked allowance.
            let due_intervals = 1 + (current_ledger - next_due) / info.interval_ledgers;

            let token_client = token::Client::new(&env, &token_addr);
            let allowance = token_client.allowance(&subscriber, &env.current_contract_address());

            // Collect as many owed periods as the allowance covers; the rest stay owed.
            let affordable = allowance / info.amount;
            let paid_intervals = u32::try_from(affordable)
                .unwrap_or(u32::MAX)
                .min(due_intervals);
            if paid_intervals == 0 {
                return Err(SubscriptionError::InsufficientAllowance);
            }
            let total = info
                .amount
                .checked_mul(i128::from(paid_intervals))
                .ok_or(SubscriptionError::InvalidAmount)?;

            // checks-effects-interactions: update state before external call.
            // Advance only by the periods actually paid so unpaid ones remain collectable.
            let trial_just_completed = !info.trial_completed;
            info.trial_completed = true;
            info.last_charged_ledger = next_due + (paid_intervals - 1) * info.interval_ledgers;
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
            events::plan_changed(
                &env,
                &subscriber,
                &old_plan_id,
                &new_plan_id,
                proration.credit,
                proration.charge,
            );
                &provider,
                &total,
            );

            if trial_just_completed {
                events::trial_completed(&env, &subscriber);
            }
            events::charged(&env, &subscriber, &provider, total);
            Ok(())
        }

        /// Subscriber cancels one of their subscriptions. No further charges can be
        /// made for that plan; other plans held by the subscriber are unaffected.
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
        /// Returns [`SubscriptionError::NotSubscribed`] if no subscription exists for the pair.
        /// Returns [`SubscriptionError::SubscriptionInactive`] if already cancelled or suspended.
        pub fn cancel(
            env: Env,
            subscriber: Address,
            plan_id: Symbol,
        ) -> Result<(), SubscriptionError> {
            if !env.storage().instance().has(&DataKey::Provider) {
                return Err(SubscriptionError::NotInitialized);
            }

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

            events::cancelled(&env, &subscriber);
            events::cancelled(&env, &subscriber, &plan_id);
            Ok(())
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
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod prop_test;
