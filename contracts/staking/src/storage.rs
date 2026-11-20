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
    /// Total boosted weight (reward-share denominator).
    TotalWeight,
    /// Per-staker: boosted weight (`stake * boost`).
    Weight(Address),
    /// Per-staker: boost multiplier in basis points.
    Boost(Address),
    /// Per-staker: ledger until which the stake is locked.
    LockedUntil(Address),
    /// Cumulative rewards emitted to stakers.
    TotalEmitted,
    /// Cumulative rewards claimed by stakers.
    TotalClaimed,
    /// Per-address: whether the address is an authorized consensus monitor
    /// permitted to call `slash` (`bool`).
    AuthorizedSlasher(Address),
    /// Total slashed tokens held in the insurance pool (`i128`).
    InsurancePool,
    /// Base reward rate in reward tokens per ledger before APY decay (`i128`).
    BaseRewardRate,
    /// Total-staked saturation threshold above which the reward rate decays (`i128`).
    SaturationThreshold,
    /// Decay factor applied per unit of saturation, in basis points (`i128`).
    DecayFactorBps,
    /// Per-staker: queue of pending unbond requests (`Vec<UnbondRequest>`).
    PendingUnbonds(Address),
    /// Contract paused state (`bool`).
    Paused,
    /// Emergency unstake penalty in basis points (`i128`).
    EmergencyPenaltyBps,
}

/// Boost denominator: 10_000 basis points = 1x.
pub const BOOST_DENOMINATOR: i128 = 10_000;

/// Lockup tiers as `(min_lockup_ledgers, boost_bps)` at ~5s ledgers:
/// 90 days → 1.5x, 365 days → 2x.
pub const LOCKUP_TIERS: [(u32, i128); 2] = [(1_555_200, 15_000), (6_307_200, 20_000)];

/// Scaling factor for reward-per-token fixed-point arithmetic.
/// Using 1e12 gives enough precision for typical token amounts.
pub const REWARD_SCALE: i128 = 1_000_000_000_000;

/// Basis-point denominator used to convert `EmergencyPenaltyBps` into a fee.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Basis-point denominator used to convert a slash `percentage` into a
/// fraction of the staker's principal (1 bps = 0.01%).
pub const SLASH_BPS_DENOMINATOR: i128 = 10_000;

/// Default unbonding delay in ledgers (~14 days at ~5s ledgers).
pub const DEFAULT_UNBONDING_PERIOD: u32 = 200_000;

/// Holds the state of an unbonding request for a staker.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UnbondRequest {
    /// Amount of stake tokens queued for withdrawal.
    pub amount: i128,
    /// Ledger sequence after which `withdraw` becomes valid.
    pub available_at: u32,
}

/// Computes the dynamic reward rate (reward tokens per ledger) given the
/// current total staked amount.
///
/// The rate stays at `base_rate` while `total_staked` is at or below
/// `saturation_threshold`. Above the threshold the rate decays linearly,
/// scaled by `decay_factor_bps`, and is floored at zero so emissions can
/// never exceed the configured base rate or go negative.
pub fn dynamic_reward_rate(
    base_rate: i128,
    total_staked: i128,
    saturation_threshold: i128,
    decay_factor_bps: i128,
) -> i128 {
    if base_rate <= 0 {
        return 0;
    }
    if saturation_threshold <= 0 || total_staked <= saturation_threshold {
        return base_rate;
    }
    // Excess saturation expressed in basis points of the threshold.
    let excess_bps = (total_staked - saturation_threshold)
        .saturating_mul(BPS_DENOMINATOR)
        / saturation_threshold;
    let decay_bps = excess_bps.saturating_mul(decay_factor_bps) / BPS_DENOMINATOR;
    if decay_bps >= BPS_DENOMINATOR {
        return 0;
    }
    base_rate.saturating_mul(BPS_DENOMINATOR - decay_bps) / BPS_DENOMINATOR
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
