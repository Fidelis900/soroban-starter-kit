// `#[contracttype]` generates undocumented public associated items.
#![allow(missing_docs)]

use soroban_sdk::{Address, String, Vec, contracttype};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Creator,
    Token,
    Goal,
    Deadline,
    TotalPledged,
    Claimed,
    Pledge(Address),
    Tiers,
    MaxPledgePerAddress,
    DeadlineExtended,
    ClaimWindow,
    Milestones,
    MilestoneVotes(u32, Address),
    MilestoneReleased(u32),
    WhitelistedTokens,
    TokenPledge(Address, Address), // (pledger, token)
    TokenTotalPledged(Address),
    OracleAddress,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CrowdfundInfo {
    pub creator: Address,
    pub token: Address,
    pub goal: i128,
    pub deadline: u32,
    pub total_pledged: i128,
    pub claimed: bool,
    pub tiers: Vec<TierStatus>,
    pub max_pledge_per_address: Option<i128>,
}

/// A funding tier (stretch goal) settable at initialize: crossing `threshold`
/// unlocks the reward described by `description`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FundingTier {
    pub threshold: i128,
    pub description: String,
}

/// A funding tier along with whether `total_pledged` has met its threshold.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TierStatus {
    pub threshold: i128,
    pub description: String,
    pub met: bool,
}

/// Milestone for tranche-based fund release
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub milestone_id: u32,
    pub amount: i128,
    pub description: String,
    pub votes_required: u32,
}

/// Token contribution with price conversion
#[contracttype]
#[derive(Clone, Debug)]
pub struct TokenContribution {
    pub token: Address,
    pub amount: i128,
    pub value_in_common_unit: i128,
}
