// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, Bytes, Symbol, Vec, contracttype};

/// Default grace period applied when the provider has not configured one:
/// 3 days at ~5 seconds per ledger (`3 * 24 * 60 * 60 / 5`). During this window
/// a subscriber whose recurring charge failed keeps access and the charge can be
/// retried before the subscription is suspended (issue #1485).
pub const DEFAULT_GRACE_PERIOD_LEDGERS: u32 = 51_840;

/// Minimum advance-notice window a provider must give before a plan price
/// increase takes effect: 30 days at ~5 seconds per ledger
/// (`30 * 24 * 60 * 60 / 5`). Subscribers may cancel without penalty during
/// this window; the new price applies only once it has elapsed.
pub const PRICE_NOTICE_LEDGERS: u32 = 518_400;

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
    /// Discount in basis points attached to a referral voucher code (persistent).
    Voucher(Address, Bytes),
    /// The voucher earmarked for a subscriber's next recurring charge (persistent).
    ActiveVoucher(Address),
    /// Subscriber-approved amount for a (subscriber, plan) pair when a provider
    /// increases plan pricing and explicit consent is required.
    ApprovedAmount(Address, Symbol),
    /// Accumulated provider revenue held in contract escrow.
    ProviderRevenue(Address),
    /// Minimum accumulated revenue required before a payout is allowed.
    MinPayoutThreshold(Address),
    /// Number of ledgers before the due date that subscriptions are reported.
    NotificationWindow,
    /// Index of all (subscriber, plan) pairs for keeper queries.
    SubscriptionIndex,
    /// Registered keeper bot allowed to call `charge_subscriber` (persistent).
    Keeper(Address),
    /// Keeper gas bounty in basis points of each charged amount (instance).
    KeeperBountyBps,
    /// Scheduled plan price increase awaiting its notice window to elapse
    /// (persistent): `(new_amount, effective_ledger)`.
    PendingIncrease(Symbol),
}

/// Plan configuration that can be registered by the admin.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Unique identifier for the plan.
    pub plan_id: Symbol,
    /// Fixed base fee charged per interval.
    pub amount: i128,
    /// Number of ledgers between each charge.
    pub interval_ledgers: u32,
    /// Price per reported usage unit, added on top of `amount` at each charge (0 = no metering).
    pub unit_price: i128,
    /// Discount in basis points (0..=10_000) applied to the base fee when prepaying intervals.
    pub prepay_discount_bps: u32,
    /// Whether the plan is active and can be subscribed to.
    pub active: bool,
}

/// Lifecycle status of a fixed-term subscription.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Active,
    Expired,
}

/// Subscription configuration and state for a single (subscriber, plan) pair.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionInfo {
    /// The plan ID that this subscription belongs to.
    pub plan_id: Symbol,
    /// Base fee charged per interval (copied from plan at subscription time).
    pub amount: i128,
    /// Number of ledgers between each charge (copied from plan at subscription time).
    pub interval_ledgers: u32,
    /// Price per usage unit (copied from plan at subscription time).
    pub unit_price: i128,
    /// Usage units reported since the last charge; reset to 0 on each charge.
    pub usage_units: u64,
    /// Number of prepaid intervals not yet released to the provider.
    pub prepaid_intervals_remaining: u32,
    /// Prepaid tokens still held in contract escrow for this subscriber.
    pub prepaid_balance: i128,
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
    /// Number of successfully completed billing cycles.
    pub billing_cycles: u32,
    /// Terminal cycle limit for fixed-term subscriptions (`None` = open-ended).
    pub max_billing_cycles: Option<u32>,
    /// Whether the subscription is still active or has reached its term end.
    pub status: SubscriptionStatus,
    /// Whether the subscriber has paused billing without cancelling. While
    /// paused no recurring charge is collected and the billing interval is
    /// frozen, so the paid period is not consumed by the pause.
    pub paused: bool,
    /// Ledger at which the current pause started, if any.
    pub paused_at_ledger: Option<u32>,
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
