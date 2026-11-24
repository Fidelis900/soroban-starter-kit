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

/// Emitted when a referral voucher is registered for a subscriber.
pub fn voucher_applied_to_subscriber(
    env: &Env,
    subscriber: &Address,
    code: &soroban_sdk::Bytes,
    discount_bps: u32,
) {
    env.events().publish(
        (Symbol::new(env, "voucher_applied"), subscriber.clone()),
        (code.clone(), discount_bps),
    );
}

/// Emitted when a voucher is consumed by a recurring charge.
pub fn voucher_applied(
    env: &Env,
    subscriber: &Address,
    code: &soroban_sdk::Bytes,
    discount_bps: u32,
    gross: i128,
    net: i128,
) {
    env.events().publish(
        (Symbol::new(env, "voucher_used"), subscriber.clone()),
        (code.clone(), discount_bps, gross, net),
    );
}

/// Emitted when a subscription is due (or about to be due) for renewal.
/// Topics: (Symbol, Address, Symbol) — event name, subscriber, plan
/// Data: (next_billing_ledger, current_ledger)
pub fn billing_due_soon(
    env: &Env,
    subscriber: &Address,
    plan_id: &Symbol,
    next_billing_ledger: u32,
    current_ledger: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "billing_due_soon"),
            subscriber.clone(),
            plan_id.clone(),
        ),
        (next_billing_ledger, current_ledger),
    );
}

/// Emitted when a fixed-term subscription reaches its billing cycle limit.
pub fn subscription_expired(env: &Env, subscriber: &Address, plan_id: &Symbol) {
    env.events().publish(
        (Symbol::new(env, "subscription_expired"), subscriber.clone()),
        plan_id.clone(),
    );
}

/// Emitted when a failed charge opens a subscription's grace period (issue #1485).
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (plan_id, grace_period_end_ledger)
pub fn grace_period_started(env: &Env, subscriber: &Address, plan_id: &Symbol, ends_at: u32) {
    env.events().publish(
        (Symbol::new(env, "grace_period_started"), subscriber.clone()),
        (plan_id.clone(), ends_at),
    );
}

/// Emitted when a retried charge succeeds during the grace period.
/// Topics: (Symbol, Address) — event name, subscriber
/// Data: (plan_id, failed attempts before recovery)
pub fn grace_period_recovered(
    env: &Env,
    subscriber: &Address,
    plan_id: &Symbol,
    failed_charges_count: u32,
) {
    env.events().publish(
        (
            Symbol::new(env, "grace_period_recovered"),
            subscriber.clone(),
        ),
        (plan_id.clone(), failed_charges_count),
    );
}

/// Emitted when the provider registers a keeper bot (issue #1484).
/// Topics: (Symbol, Address) — event name, keeper
pub fn keeper_registered(env: &Env, keeper: &Address) {
    env.events()
        .publish((Symbol::new(env, "keeper_registered"), keeper.clone()), ());
}

/// Emitted when the provider removes a keeper bot.
/// Topics: (Symbol, Address) — event name, keeper
pub fn keeper_removed(env: &Env, keeper: &Address) {
    env.events()
        .publish((Symbol::new(env, "keeper_removed"), keeper.clone()), ());
}

/// Emitted when the provider updates the keeper bounty (basis points).
/// Topics: (Symbol,) — event name
pub fn keeper_bounty_updated(env: &Env, bounty_bps: u32) {
    env.events()
        .publish((Symbol::new(env, "keeper_bounty_updated"),), bounty_bps);
}

/// Emitted after a keeper-initiated `charge_subscriber` call.
/// Topics: (Symbol, Address, Address) — event name, keeper, subscriber
/// Data: (plan_id, amount charged, bounty paid to the keeper)
pub fn keeper_charged(
    env: &Env,
    keeper: &Address,
    subscriber: &Address,
    plan_id: &Symbol,
    charged: i128,
    bounty: i128,
) {
    env.events().publish(
        (
            Symbol::new(env, "keeper_charged"),
            keeper.clone(),
            subscriber.clone(),
        ),
        (plan_id.clone(), charged, bounty),
    );
}

/// Emitted when a subscriber pauses their subscription's billing.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn paused(env: &Env, subscriber: &Address, plan_id: &Symbol) {
    env.events().publish(
        (Symbol::new(env, "paused"), subscriber.clone()),
        plan_id.clone(),
    );
}

/// Emitted when a subscriber resumes a paused subscription. The billing
/// interval is shifted forward by the paused duration so paid days are kept.
/// Topics: (Symbol, Address) — event name, subscriber
pub fn resumed(env: &Env, subscriber: &Address, plan_id: &Symbol, frozen_ledgers: u32) {
    env.events().publish(
        (Symbol::new(env, "resumed"), subscriber.clone()),
        (plan_id.clone(), frozen_ledgers),
    );
}

/// Emitted when the provider schedules a plan price increase. The new price
/// does not take effect until `effective_ledger`, giving subscribers a notice
/// window during which they may cancel without penalty.
/// Topics: (Symbol, Symbol) — event name, plan_id
pub fn price_increase_scheduled(
    env: &Env,
    plan_id: &Symbol,
    old_amount: i128,
    new_amount: i128,
    effective_ledger: u32,
) {
    env.events().publish(
        (Symbol::new(env, "price_increase_scheduled"), plan_id.clone()),
        (old_amount, new_amount, effective_ledger),
    );
}

/// Emitted when a scheduled price increase takes effect for a subscriber at
/// their first charge after the notice window elapsed.
/// Topics: (Symbol, Symbol) — event name, plan_id
pub fn price_increase_applied(env: &Env, plan_id: &Symbol, new_amount: i128) {
    env.events()
        .publish((Symbol::new(env, "price_increase_applied"), plan_id.clone()), new_amount);
}

/// Emitted when the provider lowers (or keeps) a plan price, applied
/// immediately to the plan for new subscribers.
/// Topics: (Symbol, Symbol) — event name, plan_id
pub fn plan_amount_updated(env: &Env, plan_id: &Symbol, new_amount: i128) {
    env.events()
        .publish((Symbol::new(env, "plan_amount_updated"), plan_id.clone()), new_amount);
}
