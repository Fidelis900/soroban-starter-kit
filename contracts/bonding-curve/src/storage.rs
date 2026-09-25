// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    Admin,
    Token,
    Reserve,
    Supply,
    Price,
    CurveBalance(Address),
    Slope,
    BasePrice,
    ConnectorWeightBps,
    FeeBps,
    Treasury,
    /// Graduation configuration: supply cap at which the curve graduates (instance).
    GraduationSupplyCap,
    /// Current curve state: Active or Graduated (instance).
    CurveState,
    /// Last ledger sequence at which `actor` executed a buy (persistent, per-address).
    /// Used to enforce the per-actor buy cooldown (anti-sandwich rate limit).
    LastBuyLedger(Address),
    /// Number of buys `actor` has made in the current ledger sequence (persistent).
    BuysThisLedger(Address),
}

/// Fixed-point scale used for prices, slopes, and ratios.
pub const PRICE_SCALE: i128 = 1_000_000;
/// Basis-point denominator for connector weights and trading fees.
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Maximum fee accepted by the contract (100%).
pub const MAX_FEE_BPS: u32 = 10_000;
/// Minimum connector weight accepted by the contract.
pub const MIN_CONNECTOR_WEIGHT_BPS: u32 = 1;

/// Maximum number of buy transactions a single address may submit in the same
/// ledger (sequence number). Enforcing this at the contract level prevents
/// atomic same-ledger sandwich attacks.
pub const MAX_BUYS_PER_LEDGER: u32 = 1;

/// Minimum number of ledgers that must pass between successive buys from the
/// same address. A value of 1 means an address cannot buy in two consecutive
/// ledgers (approximately 5 s on Stellar).
pub const BUY_COOLDOWN_LEDGERS: u32 = 1;

/// Represents the lifecycle state of the bonding curve.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CurveState {
    /// Normal operation: minting and burning are open.
    Active,
    /// The curve has graduated: minting is locked and an authorized migration
    /// entry point is available to move reserve + supply to an AMM.
    Graduated,
}

/// Keep Address in this module's public type surface for generated clients.
pub type TreasuryAddress = Address;
