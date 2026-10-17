// `#[contracttype]` generates undocumented public associated items; allow
// missing_docs for this module. The types and fields are still documented.
#![allow(missing_docs)]

use soroban_sdk::{Address, Symbol, contracttype};

/// Instance-storage keys for the oracle contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The admin address authorized to push price updates.
    Admin,
    /// The most recently published price (single-asset legacy feed).
    Price,
    /// The ledger sequence at which the price was last updated.
    UpdatedAt,
    /// The maximum age, in ledgers, before a price is considered stale.
    StalenessThreshold,
    /// The unix timestamp at which the price was last updated.
    UpdatedAtTimestamp,
    /// The admin-configured set of authorized publishers (persistent).
    Publishers,
    /// The latest submission from a given publisher (persistent).
    Submission(Address),
    /// Ring buffer of the last `N` price observations, oldest first (instance).
    History,
    /// The ledger sequence of a publisher's last `submit_price` call (persistent).
    LastSubmissionLedger(Address),
    /// Per-asset price stored in persistent storage, keyed by asset symbol.
    AssetPrice(Symbol),
    /// Per-asset decimal precision stored in persistent storage, keyed by asset symbol.
    AssetDecimals(Symbol),
    /// Minimum number of fresh publisher submissions required for
    /// `get_median_price` to return a result (issue #1195).
    MinQuorum,
}

/// Snapshot of the oracle state returned by read entry points.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceData {
    /// The published price.
    pub price: i128,
    /// The ledger sequence at which `price` was set.
    pub updated_at: u32,
    /// The admin address that published the price.
    pub admin: Address,
    /// The configured staleness threshold, in ledgers.
    pub staleness_threshold: u32,
}

/// A single publisher's latest price submission.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublisherSubmission {
    /// The submitted price.
    pub price: i128,
    /// The unix timestamp at which the price was submitted.
    pub timestamp: u64,
}

/// A single historical price observation, used to compute a TWAP.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceObservation {
    /// The recorded price.
    pub price: i128,
    /// The unix timestamp at which the price was recorded.
    pub timestamp: u64,
}

/// Per-asset price feed data returned by `get_asset_feed`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetFeedData {
    /// The asset symbol (e.g. `XLM`, `BTC`).
    pub asset: Symbol,
    /// The latest price for this asset.
    pub price: i128,
    /// Decimal precision of the price (e.g. 7 for XLM/USD at 7 decimals).
    pub decimals: u32,
    /// Ledger sequence at which this price was recorded.
    pub updated_at: u32,
}
