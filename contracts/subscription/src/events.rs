use soroban_sdk::{Address, Env, Symbol};

use crate::storage::BatchChargeResult;

/// Emitted when the contract is initialized.
/// Topics: (Symbol, Address) — event name, provider
pub fn initialized(env: &Env, provider: &Address, token: &Address) {
    env.events().publish(
        (Symbol::new(env, "initialized"), provider.clone()),
        token.clone(),
    );
}

/// Emitted when a new plan is registered by the provider.
/// Topics: (Symbol, Symbol) — event name, plan_id
pub fn plan_registered(env: &Env, plan_id: &Symbol, amount: i128, interval_ledgers: u32) {
    env.events().publish(
        (Symbol::new(env, "plan_registered"), plan_id.clone()),
        (amount, interval_ledgers),
    );
}

/// Emitted when a plan's status is updated by the provider.
/// Topics: (Symbol, Symbol) — event name, plan_id
pub fn plan_updated(env: &Env, plan_id: &Symbol, active: bool) {
    env.events()
        .publish((Symbol::new(env, "plan_updated"), plan_id.clone()), active);
}

/// Emitted when the provider updates the grace period.
/// Topics: (Symbol,) — event name
pub fn grace_period_updated(env: &Env, grace_period_ledgers: u32) {
    env.events().publish(
        (Symbol::new(env, "grace_period_updated"),),
        grace_period_ledgers,
    );
}

/// Emitted when a subscriber registers a new subscription.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn subscribed(
    env: &Env,
    subscriber: &Address,
    plan_id: &Symbol,
    amount: i128,
    interval_ledgers: u32,
) {
    env.events().publish(
        (Symbol::new(env, "subscribed"), subscriber.clone()),
        (plan_id.clone(), amount, interval_ledgers),
    );
}

/// Emitted when the provider successfully charges a subscriber.
/// Topics: (Symbol, Address, Address) — event name, subscriber, provider
pub fn charged(
    env: &Env,
    subscriber: &Address,
    provider: &Address,
    plan_id: &Symbol,
    amount: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "charged"),
            subscriber.clone(),
            provider.clone(),
        ),
        (plan_id.clone(), amount),
    );
}

/// Emitted when a charge attempt fails and the subscription enters/remains in its grace period.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn charge_failed(
    env: &Env,
    subscriber: &Address,
    plan_id: &Symbol,
    failed_charges_count: u32,
    delinquent_since_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "charge_failed"), subscriber.clone()),
        (
            plan_id.clone(),
            failed_charges_count,
            delinquent_since_ledger,
        ),
    );
}

/// Emitted when a subscription is suspended after its grace period expired.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn subscription_suspended(
    env: &Env,
    subscriber: &Address,
    plan_id: &Symbol,
    failed_charges_count: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "subscription_suspended"),
            subscriber.clone(),
        ),
        (plan_id.clone(), failed_charges_count),
    );
}

/// Emitted once per `charge_batch` call with the aggregated totals.
/// Topics: (Symbol, Address) — event name, provider
pub fn batch_charged(env: &Env, provider: &Address, result: &BatchChargeResult) {
    env.events().publish(
        (Symbol::new(env, "batch_charged"), provider.clone()),
        (
            result.charged,
            result.trials_completed,
            result.failed,
            result.suspended,
            result.skipped,
            result.total_charged,
        ),
    );
}

/// Emitted when a subscriber cancels their subscription.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn cancelled(env: &Env, subscriber: &Address, plan_id: &Symbol) {
    env.events().publish(
        (Symbol::new(env, "cancelled"), subscriber.clone()),
        plan_id.clone(),
    );
}

/// Emitted when the provider reports metered usage for a subscriber.
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (u64, u64) — units reported, cumulative units since the last charge
pub fn usage_reported(env: &Env, subscriber: &Address, units: u64, total_units: u64) {
    env.events().publish(
        (Symbol::new(env, "usage_reported"), subscriber.clone()),
        (units, total_units),
    );
}

/// Emitted when a subscriber prepays for multiple intervals into escrow.
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (u32, i128) — prepaid intervals, total amount escrowed
pub fn prepaid(env: &Env, subscriber: &Address, intervals: u32, total: i128) {
    env.events().publish(
        (Symbol::new(env, "prepaid"), subscriber.clone()),
        (intervals, total),
    );
}

/// Emitted when unconsumed prepaid funds are refunded to a subscriber on cancellation.
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (u32, i128) — unconsumed intervals, amount refunded
pub fn prepaid_refunded(env: &Env, subscriber: &Address, intervals: u32, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "prepaid_refunded"), subscriber.clone()),
        (intervals, amount),
    );
}

/// Emitted when a subscriber's trial period is completed.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn trial_completed(env: &Env, subscriber: &Address, plan_id: &Symbol) {
    env.events().publish(
        (Symbol::new(env, "trial_completed"), subscriber.clone()),
        plan_id.clone(),
    );
}

/// Emitted when a subscriber migrates from one plan to another.
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (`old_plan_id`, `new_plan_id`, credit applied, amount charged now)
pub fn plan_changed(
    env: &Env,
    subscriber: &Address,
    old_plan_id: &Symbol,
    new_plan_id: &Symbol,
    credit: i128,
    charged: i128,
) {
    env.events().publish(
        (Symbol::new(env, "plan_changed"), subscriber.clone()),
        (old_plan_id.clone(), new_plan_id.clone(), credit, charged),
    );
}
