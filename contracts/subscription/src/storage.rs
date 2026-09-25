// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype, Symbol};

/// Top-level storage keys used by [`SubscriptionContract`](crate::SubscriptionContract).
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The service provider's [`Address`] (instance storage).
    Provider,
    /// The payment token contract [`Address`] (instance storage).
    Token,
    /// Per-subscriber [`SubscriptionInfo`] (persistent storage).
    Subscription(Address),
    /// Plan details for a named plan (persistent storage).
    Plan(Symbol),
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

/// Subscription configuration and state for a single subscriber.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionInfo {
    /// The plan ID that this subscriber is subscribed to.
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
    /// Ledger sequence number of the last successful charge (or subscription start).
    pub last_charged_ledger: u32,
    /// Whether the subscription is currently active.
    pub active: bool,
}