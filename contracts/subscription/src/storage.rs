// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Symbol, Vec, contracttype};

/// Default grace period applied when the provider has not configured one:
/// ~7 days at ~5 seconds per ledger.
pub const DEFAULT_GRACE_PERIOD_LEDGERS: u32 = 120_960;

/// Top-level storage keys used by [`SubscriptionContract`](crate::SubscriptionContract).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The service provider's [`Address`] (instance storage).
    Provider,
    /// The payment token contract [`Address`] (instance storage).
    Token,
    /// Per-(subscriber, plan) [`SubscriptionInfo`] (persistent storage).
    /// A subscriber may hold several distinct plans concurrently.
    Subscription(Address, Symbol),
    /// Plan details for a named plan (persistent storage).
    Plan(Symbol),
    /// Number of ledgers a delinquent subscription is tolerated before it is
    /// suspended (instance storage).
    GracePeriod,
}

/// Plan configuration that can be registered by the admin.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Unique identifier for the plan.
    pub plan_id: Symbol,
    /// Amount of tokens charged per interval.
    pub amount: i128,
    /// Number of ledgers between each charge.
    pub interval_ledgers: u32,
    /// Whether the plan is active and can be subscribed to.
    pub active: bool,
}

/// Subscription configuration and state for a single (subscriber, plan) pair.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionInfo {
    /// The plan ID that this subscription belongs to.
    pub plan_id: Symbol,
    /// Amount of tokens charged per interval (copied from plan at subscription time).
    pub amount: i128,
    /// Number of ledgers between each charge (copied from plan at subscription time).
    pub interval_ledgers: u32,
    /// Number of ledgers in the trial period (if any).
    pub trial_ledgers: u32,
    /// Whether the trial period has been completed (first charge processed).
    pub trial_completed: bool,
    /// Ledger sequence number the current paid period is measured from. The next
    /// charge is due at `last_charged_ledger + interval_ledgers`. After a
    /// pro-rated plan change this may lie in the future.
    /// Start ledger of the most recently paid billing period (or subscription start
    /// while in trial). The next period falls due `interval_ledgers` after this.
    pub last_charged_ledger: u32,
    /// Whether the subscription is currently active (false once cancelled,
    /// suspended or migrated to another plan).
    pub active: bool,
    /// Whether the subscription was suspended for non-payment after its grace
    /// period expired.
    pub suspended: bool,
    /// Number of consecutive failed charge attempts since the last successful charge.
    pub failed_charges_count: u32,
    /// Ledger of the first failed charge attempt in the current delinquency, if any.
    pub delinquent_since_ledger: Option<u32>,
}

/// Result of a single charge attempt that did not abort with an error.
///
/// Payment failures are reported as outcomes rather than errors so that the
/// delinquency tracking written during the attempt is persisted (a contract
/// error would roll back all state changes).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeOutcome {
    /// Tokens were transferred to the provider.
    Charged,
    /// The trial period ended; the paid billing cycle starts now.
    TrialCompleted,
    /// The transfer failed (insufficient allowance/balance); the subscription is
    /// delinquent but still within its grace period.
    PaymentFailed,
    /// The transfer failed and the grace period expired; the subscription is now suspended.
    Suspended,
    /// Batch only: the entry was not chargeable (not due, not found, inactive, ...).
    Skipped,
}

/// Aggregated result of [`charge_batch`](crate::SubscriptionContract::charge_batch).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchChargeResult {
    /// Number of subscriptions successfully charged.
    pub charged: u32,
    /// Number of subscriptions whose trial period completed.
    pub trials_completed: u32,
    /// Number of subscriptions whose payment failed but remain in their grace period.
    pub failed: u32,
    /// Number of subscriptions suspended by this batch.
    pub suspended: u32,
    /// Number of entries skipped (not due, not subscribed, inactive, suspended).
    pub skipped: u32,
    /// Total amount of tokens transferred to the provider.
    pub total_charged: i128,
    /// Per-entry outcome, in the same order as the input.
    pub outcomes: Vec<ChargeOutcome>,
}
