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
}

/// Fixed-point scale used for prices, slopes, and ratios.
pub const PRICE_SCALE: i128 = 1_000_000;
/// Basis-point denominator for connector weights and trading fees.
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Maximum fee accepted by the contract (100%).
pub const MAX_FEE_BPS: u32 = 10_000;
/// Minimum connector weight accepted by the contract.
pub const MIN_CONNECTOR_WEIGHT_BPS: u32 = 1;

/// Keep Address in this module's public type surface for generated clients.
pub type TreasuryAddress = Address;
