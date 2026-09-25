// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype};

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
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapState {
    Pending = 0,
    Executed = 1,
    Accepted = 1,
    Cancelled = 2,
}

impl core::fmt::Display for SwapState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            SwapState::Pending => "pending",
            SwapState::Executed => "executed",
            SwapState::Accepted => "accepted",
            SwapState::Cancelled => "cancelled",
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
#[derive(Clone, Debug)]
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
}
}
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
