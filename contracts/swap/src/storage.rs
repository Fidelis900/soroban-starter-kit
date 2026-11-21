// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, BytesN, contracttype};

/// Minimum LP shares permanently locked to the dead address on the first
/// liquidity provision, mirroring Uniswap V2's `MINIMUM_LIQUIDITY`.
pub const MINIMUM_LIQUIDITY: i128 = 1_000;

/// Returns the dead address used to permanently lock minimum liquidity.
///
/// The address is derived from a fixed byte pattern so every deployment burns
/// to the same unrecoverable account.
pub fn dead_address(env: &soroban_sdk::Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

/// Configuration and persistent swap keys.
///
/// Configuration and the monotonic counter are kept in instance storage. Each
/// individual swap is stored under a composite persistent key so transaction
/// data cannot grow the instance entry.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    SwapCount,
    BasketSwapCount,
    Initialized,
    Admin,
    Treasury,
    FeeBps,
    Swap(u32),
    BasketSwap(u32),
    /// Cumulative price of token0 weighted by elapsed ledgers.
    Price0Cumulative,
    /// Cumulative price of token1 weighted by elapsed ledgers.
    Price1Cumulative,
    /// Ledger sequence at which the cumulative prices were last updated.
    LastUpdateLedger,
    /// Reserve of token0 in the pool.
    Reserve0,
    /// Reserve of token1 in the pool.
    Reserve1,
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapState {
    Pending = 0,
    Executed = 1,
    Accepted = 1,
    Cancelled = 2,
    Expired = 3,
}

impl core::fmt::Display for SwapState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            SwapState::Pending => "pending",
            SwapState::Executed => "executed",
            SwapState::Accepted => "accepted",
            SwapState::Cancelled => "cancelled",
            SwapState::Expired => "expired",
        })
    }
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasketLeg {
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapInfo {
    pub id: u32,
    pub party_a: Address,
    pub token_a: Address,
    pub amount_a: i128,
    pub token_b: Address,
    pub amount_b: i128,
    pub expires_at: u32,
    pub state: SwapState,
    pub filled_amount: i128,
    pub allow_partial: bool,
    pub escrowed: bool,
    /// Whether takers may fill less than the full offered amount.
    pub allow_partial_fills: bool,
    /// Optional SHA-256 hash lock; takers must reveal a matching preimage.
    pub hash_lock: Option<BytesN<32>>,
    /// Remaining amount of `token_a` still available to be filled.
    pub remaining_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct BasketSwapInfo {
    pub id: u32,
    pub party_a: Address,
    pub offers: soroban_sdk::Vec<BasketLeg>,
    pub demands: soroban_sdk::Vec<BasketLeg>,
    pub expires_at: u32,
    pub state: SwapState,
    pub allowed_counterparty: Option<Address>,
    pub max_execution_delay: Option<u32>,
    pub created_at: u32,
}

/// One page of results from [`super::SwapContract::get_active_swaps`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapPage {
    /// Active swaps found in this page, in ascending ID order.
    pub swaps: soroban_sdk::Vec<SwapInfo>,
    /// The cursor to pass to the next call to continue scanning, or `None`
    /// if the end of the swap range has been reached.
    pub next_cursor: Option<u32>,
}

/// Cumulative price accumulator state for TWAP oracle queries.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CumulativePrices {
    /// Cumulative price of token0 weighted by elapsed ledgers.
    pub price_0_cumulative: i128,
    /// Cumulative price of token1 weighted by elapsed ledgers.
    pub price_1_cumulative: i128,
    /// Ledger sequence at which the accumulators were last updated.
    pub last_update_ledger: u32,
}

/// Event payload emitted when minimum liquidity is locked on pool creation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidityLocked {
    /// Amount of LP shares permanently locked to the dead address.
    pub amount: i128,
    /// The dead address receiving the locked shares.
    pub to: Address,
}
