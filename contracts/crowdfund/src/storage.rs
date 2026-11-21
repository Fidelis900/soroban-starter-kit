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
    StretchGoals,
    MaxPledgePerAddress,
    DeadlineExtended,
    Cancelled,
    PledgeTier(Address),
    TierBackerCount(u32),
    ClaimWindow,
    Milestones,
    MilestoneVotes(u32, Address),
    /// Weighted approval power (sum of approving pledges) for a milestone.
    MilestoneApprovalPower(u32),
    /// Number of distinct backers who have approved a milestone.
    MilestoneApprovalCount(u32),
    /// Set when the creator submits a milestone release proposal.
    MilestoneRequested(u32),
    MilestoneReleased(u32),
    /// Total amount already disbursed across released tranches.
    TotalReleased,
    WhitelistedTokens,
    TokenPledge(Address, Address), // (pledger, token)
    TokenTotalPledged(Address),
    OracleAddress,
    /// Off-chain identity verification hash / credential from an approved KYC oracle.
    KycHash,
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
    pub cancelled: bool,
    pub tiers: Vec<TierStatus>,
    pub stretch_goals: Vec<StretchGoalStatus>,
    pub max_pledge_per_address: Option<i128>,
    /// Creator's KYC attestation hash, readable on-chain by contributors.
    pub kyc_hash: String,
}

/// A funding tier (stretch goal) settable at initialize: crossing `threshold`
/// unlocks the reward described by `description`. `max_backers` limits the
/// number of backers who can claim this tier (None = unlimited).
#[contracttype]
#[derive(Clone, Debug)]
pub struct FundingTier {
    pub threshold: i128,
    pub description: String,
    pub max_backers: Option<u32>,
}

/// A funding tier along with whether `total_pledged` has met its threshold.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TierStatus {
    pub threshold: i128,
    pub description: String,
    pub met: bool,
    pub max_backers: Option<u32>,
    pub current_backers: u32,
}

/// Secondary stretch goal: crossing `threshold` unlocks the described feature.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StretchGoal {
    pub threshold: i128,
    pub description: String,
}

/// A stretch goal plus its dynamically computed unlock status.
#[contracttype]
#[derive(Clone, Debug)]
pub struct StretchGoalStatus {
    pub threshold: i128,
    pub description: String,
    pub unlocked: bool,
}

/// Milestone for tranche-based fund release.
///
/// Funds are held in tranches; each tranche is released only after the
/// creator proposes it and backers approve with sufficient quorum.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub milestone_id: u32,
    pub amount: i128,
    pub description: String,
    /// Minimum number of distinct backer approvals required for release.
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
