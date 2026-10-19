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
    /// Maximum single-transaction purchase size in basis points of the total
    /// max supply (instance). `None` disables the per-transaction cap.
    MaxBuyBps,
    /// Maximum allowed spot-price surge in percent within the surge window
    /// before buys are halted by the circuit breaker (instance). `None`
    /// disables the circuit breaker.
    MaxSurgePct,
    /// Number of ledgers forming the look-back window for the circuit breaker
    /// (instance). Defaults to `DEFAULT_SURGE_WINDOW_LEDGERS`.
    SurgeWindowLedgers,
    /// Spot price recorded at the start of the current surge window (instance).
    /// Used together with `RefLedger` to detect rapid price surges.
    RefPrice,
    /// Ledger sequence at which `RefPrice` was recorded (instance).
    RefLedger,
    /// Sigmoid curve configuration: midpoint (in supply units) and steepness
    /// (fixed-point scale multiplier). Stored in instance storage.
    SigmoidConfig,
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

/// Default look-back window (in ledgers) used by the price-surge circuit
/// breaker when `SurgeWindowLedgers` has not been configured.
pub const DEFAULT_SURGE_WINDOW_LEDGERS: u32 = 20;
/// Selects the pricing model used by the bonding curve.
///
/// * `Linear` — constant slope price curve.
/// * `Power` — power-law curve driven by the connector weight.
/// * `Sigmoid` — S-curve dynamics: cheap early tokens, rapid price growth
///   during adoption, and a plateau as supply matures.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CurveModel {
    /// Linear price curve.
    Linear,
    /// Power curve (connector-weight driven).
    Power,
    /// Sigmoid (S-curve) price model.
    ///
    /// * `midpoint` — supply value (in token base units) at which the curve's
    ///   inflection point sits; the price grows fastest around this supply.
    /// * `steepness` — fixed-point scale multiplier controlling how sharply
    ///   the curve transitions through the midpoint. Larger values yield a
    ///   steeper S-shape.
    Sigmoid { midpoint: i128, steepness: u32 },
}

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
