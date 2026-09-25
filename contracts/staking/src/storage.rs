// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, contracttype, Vec};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Admin address.
    Admin,
    /// Token that users stake.
    StakeToken,
    /// Token distributed as rewards (may be the same as StakeToken).
    /// Retained for backwards compatibility; the full set of reward
    /// tokens is tracked under `RewardTokens`.
    RewardToken,
    /// Registered reward tokens (multi-reward support).
    RewardTokens,
    /// Total tokens currently staked across all stakers.
    TotalStaked,
    /// Total reward tokens deposited and not yet claimed.
    TotalRewards,
    /// Reward-per-token accumulator (scaled by REWARD_SCALE).
    /// Retained for backwards compatibility; per-token accumulators are
    /// tracked under `RewardPerTokenStoredFor`.
    RewardPerTokenStored,
    /// Per-reward-token accumulator (scaled by REWARD_SCALE).
    RewardPerTokenStoredFor(Address),
    /// Per-staker: amount staked.
    Stake(Address),
    /// Per-staker: reward-per-token snapshot at last update.
    /// Retained for backwards compatibility; per-token snapshots are
    /// tracked under `RewardPerTokenPaidFor`.
    RewardPerTokenPaid(Address),
    /// Per-staker, per-reward-token: reward-per-token snapshot at last update.
    RewardPerTokenPaidFor(Address, Address),
    /// Per-staker: accrued but unclaimed rewards.
    Rewards(Address),
    /// Per-staker, per-reward-token: accrued but unclaimed rewards.
    RewardsFor(Address, Address),
    /// Contract version number (`u32`).
    Version,
    /// Per-staker: whether auto-compounding is enabled (`bool`).
    Compounding(Address),
    /// Unbonding delay in ledgers; 0 means immediate withdrawal is allowed.
    UnbondingPeriod,
    /// Per-staker: pending unbond request.
    UnbondRequest(Address),
    /// Address that receives slashed tokens (treasury / burn).
    SlashDestination,
    /// Reward tokens deposited while no stake was active, held until
    /// stakers join (or reclaimed by the admin if the pool stays empty).
    UndistributedRewards,
    /// Continuous emission rate in reward tokens per ledger (`i128`).
    RewardRate,
    /// Ledger sequence at which the current emission period ends (`u32`).
    PeriodEnd,
    /// Ledger sequence at which the current emission period started (`u32`).
    LastUpdateLedger,
    /// Per-reward-token: reward tokens deposited while no stake was active.
    UndistributedRewardsFor(Address),
    /// Penalty applied to emergency unstakes, in basis points (1 bps = 0.01%).
    EmergencyPenaltyBps,
}

/// Scaling factor for reward-per-token fixed-point arithmetic.
/// Using 1e12 gives enough precision for typical token amounts.
pub const REWARD_SCALE: i128 = 1_000_000_000_000;

/// Basis-point denominator used to convert `EmergencyPenaltyBps` into a fee.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Holds the state of an unbonding request for a staker.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UnbondRequest {
    /// Amount of stake tokens queued for withdrawal.
    pub amount: i128,
    /// Ledger sequence after which `withdraw` becomes valid.
    pub available_at: u32,
}

/// Returns the registered reward tokens, defaulting to the legacy single
/// `RewardToken` entry when the multi-reward list has not been populated.
pub fn reward_tokens(env: &soroban_sdk::Env) -> Vec<Address> {
    if let Some(tokens) = env
        .storage()
        .instance()
        .get::<DataKey, Vec<Address>>(&DataKey::RewardTokens)
    {
        return tokens;
    }
    let mut tokens = Vec::new(env);
    if let Some(token) = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::RewardToken)
    {
        tokens.push_back(token);
    }
    tokens
}
