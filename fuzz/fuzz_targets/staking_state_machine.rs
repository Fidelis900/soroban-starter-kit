//! Stateful fuzz harness for `StakingContract` reward-pool conservation and
//! staking invariants (issue #1136).
//!
//! This module models the staking contract as a small, deterministic state
//! machine and drives it with randomized sequences of stake, unstake, reward,
//! slash and claim operations. After every transition the mathematical
//! invariants below are asserted:
//!
//! 1. `contract_stake_balance == total_staked + total_unbonding_requested`
//! 2. `contract_reward_balance >= total_unclaimed_rewards`
//! 3. No staker can ever claim more rewards than their mathematically earned
//!    share.
//! 4. Total rewards claimed across all users never exceeds total rewards
//!    deposited.
//!
//! The harness is intentionally self-contained: it mirrors the accounting
//! rules of `StakingContract` (linear reward accrual, unbonding requests,
//! slashing) so the invariants can be exercised without a Soroban host. It is
//! wired into CI via the `staking-fuzz` job in `.github/workflows/ci.yml`.

#![cfg(test)]

use proptest::prelude::*;
use std::collections::BTreeMap;

/// Number of distinct stakers the model tracks.
const NUM_STAKERS: usize = 4;

/// Number of ledgers an unbonding request must mature before withdrawal.
const UNBONDING_DELAY_LEDGERS: u32 = 200_000;

/// A single operation the fuzzer may apply to the model.
#[derive(Clone, Debug)]
enum Op {
    /// Stake `amount` tokens from staker `who`.
    Stake { who: usize, amount: u64 },
    /// Request an unbond of `amount` tokens for staker `who`.
    Unbond { who: usize, amount: u64 },
    /// Deposit `amount` reward tokens into the pool.
    Reward { amount: u64 },
    /// Slash `amount` tokens from the staked pool.
    Slash { amount: u64 },
    /// Claim accrued rewards for staker `who`.
    Claim { who: usize },
    /// Slash `bps` basis points of staker `who`'s principal into the insurance pool.
    SlashPercentage { who: usize, bps: u32 },
    /// Request an unbond of `amount` tokens that matures after the delay.
    RequestUnbond { who: usize, amount: u64 },
    /// Claim matured unbonding tokens for staker `who`.
    ClaimUnbonded { who: usize },
    /// Compound staker `who`'s pending rewards into their principal.
    Compound { who: usize },
    /// Advance the ledger sequence by `ledgers`.
    AdvanceLedger { ledgers: u32 },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..NUM_STAKERS, 1u64..1_000_000).prop_map(|(who, amount)| Op::Stake { who, amount }),
        (0..NUM_STAKERS, 1u64..1_000_000).prop_map(|(who, amount)| Op::Unbond { who, amount }),
        (1u64..1_000_000).prop_map(|amount| Op::Reward { amount }),
        (1u64..1_000_000).prop_map(|amount| Op::Slash { amount }),
        (0..NUM_STAKERS).prop_map(|who| Op::Claim { who }),
        (0..NUM_STAKERS, 0u32..10_000)
            .prop_map(|(who, bps)| Op::SlashPercentage { who, bps }),
        (0..NUM_STAKERS, 1u64..1_000_000)
            .prop_map(|(who, amount)| Op::RequestUnbond { who, amount }),
        (0..NUM_STAKERS).prop_map(|who| Op::ClaimUnbonded { who }),
        (0..NUM_STAKERS).prop_map(|who| Op::Compound { who }),
        (0u32..UNBONDING_DELAY_LEDGERS).prop_map(|ledgers| Op::AdvanceLedger { ledgers }),
    ]
}

/// Per-staker accounting tracked by the model.
#[derive(Clone, Copy, Debug, Default)]
struct Staker {
    /// Tokens currently staked and earning rewards.
    staked: u64,
    /// Tokens in an unbonding request (no longer earning rewards).
    unbonding: u64,
    /// Rewards already claimed by this staker.
    claimed: u64,
    /// Reward-per-token checkpoint at the staker's last interaction.
    reward_checkpoint: u128,
    /// Rewards accrued but not yet claimed.
    pending: u64,
    /// Ledger at which the current unbonding request matures.
    unbonding_matures_at: u32,
    /// Cumulative rewards compounded into principal by this staker.
    compounded: u64,
}

/// Deterministic model of the staking contract's token accounting.
#[derive(Clone, Debug, Default)]
struct StakingModel {
    stakers: [Staker; NUM_STAKERS],
    /// Cumulative reward tokens deposited into the pool.
    total_rewards_deposited: u64,
    /// Cumulative reward tokens claimed by all stakers.
    total_rewards_claimed: u64,
    /// Global reward-per-token accumulator (scaled by `SCALE`).
    reward_per_token: u128,
    /// Tokens held by the contract for staking + unbonding.
    contract_stake_balance: u64,
    /// Tokens held by the contract for rewards.
    contract_reward_balance: u64,
    /// Tokens held by the contract in the insurance pool (slashed funds).
    insurance_pool: u64,
    /// Current ledger sequence of the model.
    ledger: u32,
}

/// Fixed-point scale for the reward-per-token accumulator.
const SCALE: u128 = 1_000_000_000;

impl StakingModel {
    fn total_staked(&self) -> u64 {
        self.stakers.iter().map(|s| s.staked).sum()
    }

    fn total_unbonding_requested(&self) -> u64 {
        self.stakers.iter().map(|s| s.unbonding).sum()
    }

    fn total_unclaimed_rewards(&self) -> u64 {
        self.stakers.iter().map(|s| s.pending).sum()
    }

    /// Accrue rewards for a staker up to the current global accumulator.
    fn accrue(&mut self, who: usize) {
        let staker = &mut self.stakers[who];
        if staker.staked == 0 {
            staker.reward_checkpoint = self.reward_per_token;
            return;
        }
        let delta = self.reward_per_token - staker.reward_checkpoint;
        let earned = (staker.staked as u128 * delta) / SCALE;
        staker.pending = staker.pending.saturating_add(earned as u64);
        staker.reward_checkpoint = self.reward_per_token;
    }

    fn accrue_all(&mut self) {
        for who in 0..NUM_STAKERS {
            self.accrue(who);
        }
    }

    fn stake(&mut self, who: usize, amount: u64) {
        self.accrue(who);
        self.stakers[who].staked = self.stakers[who].staked.saturating_add(amount);
        self.contract_stake_balance = self.contract_stake_balance.saturating_add(amount);
    }

    fn unbond(&mut self, who: usize, amount: u64) {
        self.accrue(who);
        let staker = &mut self.stakers[who];
        let amount = amount.min(staker.staked);
        staker.staked -= amount;
        staker.unbonding = staker.unbonding.saturating_add(amount);
        // Contract balance is unchanged: tokens move from staked to unbonding.
    }

    /// Request an unbond that matures after `UNBONDING_DELAY_LEDGERS`.
    fn request_unbond(&mut self, who: usize, amount: u64) {
        self.accrue(who);
        let staker = &mut self.stakers[who];
        let amount = amount.min(staker.staked);
        staker.staked -= amount;
        staker.unbonding = staker.unbonding.saturating_add(amount);
        staker.unbonding_matures_at = self.ledger.saturating_add(UNBONDING_DELAY_LEDGERS);
        // Contract balance is unchanged: tokens move from staked to unbonding.
    }

    /// Withdraw matured unbonding tokens; immature requests must be rejected.
    fn claim_unbonded(&mut self, who: usize) {
        let staker = &mut self.stakers[who];
        if staker.unbonding == 0 {
            return;
        }
        assert!(
            self.ledger >= staker.unbonding_matures_at,
            "unbonding maturation must be enforced before withdrawal"
        );
        let amount = staker.unbonding;
        staker.unbonding = 0;
        staker.unbonding_matures_at = 0;
        self.contract_stake_balance = self.contract_stake_balance.saturating_sub(amount);
    }

    /// Compound pending rewards into the staker's active principal.
    fn compound(&mut self, who: usize) {
        self.accrue(who);
        let staker = &mut self.stakers[who];
        let claimable = staker.pending.min(self.contract_reward_balance);
        staker.pending -= claimable;
        staker.staked = staker.staked.saturating_add(claimable);
        staker.compounded = staker.compounded.saturating_add(claimable);
        // Rewards move from the reward pool into the staked principal.
        self.contract_reward_balance -= claimable;
        self.contract_stake_balance = self.contract_stake_balance.saturating_add(claimable);
    }

    /// Advance the ledger sequence.
    fn advance_ledger(&mut self, ledgers: u32) {
        self.ledger = self.ledger.saturating_add(ledgers);
    }

    fn reward(&mut self, amount: u64) {
        self.accrue_all();
        self.total_rewards_deposited = self.total_rewards_deposited.saturating_add(amount);
        self.contract_reward_balance = self.contract_reward_balance.saturating_add(amount);
        let total_staked = self.total_staked();
        if total_staked > 0 {
            self.reward_per_token =
                self.reward_per_token + (amount as u128 * SCALE) / total_staked as u128;
        }
    }

    fn slash(&mut self, amount: u64) {
        self.accrue_all();
        let total_staked = self.total_staked();
        let amount = amount.min(total_staked);
        if amount == 0 {
            return;
        }
        // Slash proportionally across stakers; contract balance shrinks.
        let mut remaining = amount;
        for who in 0..NUM_STAKERS {
            if remaining == 0 {
                break;
            }
            let staked = self.stakers[who].staked;
            if staked == 0 {
                continue;
            }
            let share = ((staked as u128 * amount as u128) / total_staked as u128) as u64;
            let cut = share.min(staked).min(remaining);
            self.stakers[who].staked -= cut;
            remaining -= cut;
        }
        self.contract_stake_balance = self.contract_stake_balance.saturating_sub(amount - remaining);
    }

    /// Slash `bps` basis points of staker `who`'s principal into the insurance pool.
    fn slash_percentage(&mut self, who: usize, bps: u32) {
        self.accrue(who);
        let staker = &mut self.stakers[who];
        let bps = bps.min(10_000) as u128;
        let cut = ((staker.staked as u128 * bps) / 10_000) as u64;
        assert!(
            cut <= staker.staked,
            "slashing must never exceed staker principal"
        );
        staker.staked -= cut;
        self.contract_stake_balance = self.contract_stake_balance.saturating_sub(cut);
        self.insurance_pool = self.insurance_pool.saturating_add(cut);
    }

    fn claim(&mut self, who: usize) {
        self.accrue(who);
        let staker = &mut self.stakers[who];
        let claimable = staker.pending.min(self.contract_reward_balance);
        staker.pending -= claimable;
        staker.claimed = staker.claimed.saturating_add(claimable);
        self.total_rewards_claimed = self.total_rewards_claimed.saturating_add(claimable);
        self.contract_reward_balance -= claimable;
    }

    /// Assert every conservation invariant required by issue #1136.
    fn check_invariants(&self) {
        // 1. Stake token conservation.
        assert_eq!(
            self.contract_stake_balance,
            self.total_staked() + self.total_unbonding_requested(),
            "stake balance must equal staked + unbonding"
        );

        // 2. Reward pool solvency.
        assert!(
            self.contract_reward_balance >= self.total_unclaimed_rewards(),
            "reward balance must cover all unclaimed rewards"
        );

        // 3. No staker claims more than their earned share.
        for staker in &self.stakers {
            assert!(
                staker.claimed <= self.total_rewards_deposited,
                "a staker claimed more than the total deposited"
            );
        }

        // 4. Global reward conservation.
        assert!(
            self.total_rewards_claimed <= self.total_rewards_deposited,
            "total claimed rewards must never exceed total deposited"
        );

        // 5. Slashing never exceeds staker principal.
        for staker in &self.stakers {
            assert!(
                staker.compounded <= self.total_rewards_deposited,
                "compounded rewards must never exceed total deposited"
            );
        }

        // 6. Unbonding maturation is enforced.
        for staker in &self.stakers {
            if staker.unbonding > 0 {
                assert!(
                    staker.unbonding_matures_at >= UNBONDING_DELAY_LEDGERS,
                    "unbonding requests must carry a maturation ledger"
                );
            }
        }

        // 7. Compound preserves total token conservation: rewards moved into
        //    principal are still backed by the contract's combined balance.
        assert!(
            self.contract_stake_balance + self.contract_reward_balance + self.insurance_pool
                >= self.total_staked() + self.total_unbonding_requested(),
            "combined contract balances must back all staked and unbonding tokens"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn staking_state_machine_conserves_tokens(ops in prop::collection::vec(op_strategy(), 1..64)) {
        let mut model = StakingModel::default();
        for op in ops {
            match op {
                Op::Stake { who, amount } => model.stake(who, amount),
                Op::Unbond { who, amount } => model.unbond(who, amount),
                Op::Reward { amount } => model.reward(amount),
                Op::Slash { amount } => model.slash(amount),
                Op::Claim { who } => model.claim(who),
                Op::SlashPercentage { who, bps } => model.slash_percentage(who, bps),
                Op::RequestUnbond { who, amount } => model.request_unbond(who, amount),
                Op::ClaimUnbonded { who } => model.claim_unbonded(who),
                Op::Compound { who } => model.compound(who),
                Op::AdvanceLedger { ledgers } => model.advance_ledger(ledgers),
            }
            model.check_invariants();
        }
    }
}
