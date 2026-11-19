//! Stateful property test harness for `VestingContract`.
//!
//! Validates token conservation and release schedule accuracy across random
//! sequences of ledger advances, claims, and revocations.
//!
//! Invariants checked:
//! 1. `contract_token_balance >= sum(unclaimed_vested + unvested_remaining)`.
//! 2. No beneficiary can claim more than their initial `amount`.
//! 3. Revocation never clawbacks already-vested tokens.
//! 4. Tokens returned to admin on revocation + claimed by beneficiary exactly
//!    equals the initial allocation.
#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, U256,
};
use std::vec::Vec;

use crate::{vested_amount, VestingContract, VestingContractClient, VestingError};

const BASE_LEDGER: u32 = 100;

/// Independent 256-bit reference for `floor(amount * num / den)`.
fn wide_mul_div(amount: i128, num: u32, den: u32) -> i128 {
    let env = Env::default();
    let wide = U256::from_u128(&env, amount as u128)
        .mul(&U256::from_u32(&env, num))
        .div(&U256::from_u32(&env, den));
    wide.to_u128().unwrap() as i128
}

/// Reference model for a single linear (cliff → end) vesting schedule.
#[derive(Clone, Debug)]
struct LinearModel {
    amount: i128,
    cliff: u32,
    end: u32,
    claimed: i128,
    /// Ledger at which vesting was capped by a revocation.
    cap: Option<u32>,
}

impl LinearModel {
    fn new(amount: i128, cliff: u32, end: u32) -> Self {
        Self { amount, cliff, end, claimed: 0, cap: None }
    }

    /// Vested amount at `now`, honouring a revocation cap.
    fn vested(&self, now: u32) -> i128 {
        let at = self.cap.map_or(now, |c| c.min(now));
        if at < self.cliff {
            return 0;
        }
        if at >= self.end {
            return self.amount;
        }
        self.amount * i128::from(at - self.cliff) / i128::from(self.end - self.cliff)
    }

    fn claimable(&self, now: u32) -> i128 {
        (self.vested(now) - self.claimed).max(0)
    }
}

/// Deploy a vesting contract and create one linear schedule per
/// `(amount, cliff_offset, duration)` triple.
fn setup<'a>(
    env: &'a Env,
    params: &[(i128, u32, u32)],
) -> (VestingContractClient<'a>, token::Client<'a>, Vec<(Address, LinearModel)>) {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = BASE_LEDGER);

    let admin = Address::generate(env);
    let contract_id = env.register(VestingContract, ());
    let client = VestingContractClient::new(env, &contract_id);

    let token_id = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = token::Client::new(env, &token_id.address());
    let total: i128 = params.iter().map(|p| p.0).sum();
    token::StellarAssetClient::new(env, &token_id.address()).mint(&admin, &total);
    client.initialize(&admin, &token_id.address());

    let mut out = Vec::new();
    for (amount, cliff_offset, duration) in params.iter().copied() {
        let beneficiary = Address::generate(env);
        let cliff = BASE_LEDGER + 1 + cliff_offset;
        let end = cliff + duration;
        client.create_schedule(&beneficiary, &cliff, &end, &amount, &true);
        out.push((beneficiary, LinearModel::new(amount, cliff, end)));
    }
    (client, token, out)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Stateful property test: random ledger advances, claims, and revocations
    /// must preserve token conservation and schedule accuracy.
    #[test]
    fn prop_stateful_token_conservation(
        params in prop::collection::vec((1i128..=1_000_000, 0u32..=1_000, 1u32..=10_000), 1..=4),
        actions in prop::collection::vec(any::<u8>(), 1..=32),
    ) {
        let env = Env::default();
        let (client, token, mut schedules) = setup(&env, &params);
        let n = schedules.len();

        let mut now = BASE_LEDGER;
        let mut total_claimed: i128 = 0;
        let mut total_returned: i128 = 0;

        for raw in actions.iter() {
            let i = (*raw as usize) % n;
            match raw % 3 {
                0 => {
                    now += (u32::from(*raw) + 1) * 100;
                    env.ledger().with_mut(|l| l.sequence_number = now);
                }
                1 => {
                    let (beneficiary, model) = &mut schedules[i];
                    let expected = model.claimable(now);
                    if expected == 0 {
                        prop_assert_eq!(
                            client.try_claim(beneficiary),
                            Err(Ok(VestingError::NothingToClaim))
                        );
                    } else {
                        let claimed = client.claim(beneficiary);
                        prop_assert_eq!(claimed, expected);
                        model.claimed += claimed;
                        total_claimed += claimed;
                        // Invariant 2: never claim more than the initial amount.
                        prop_assert!(model.claimed <= model.amount);
                    }
                }
                _ => {
                    let (beneficiary, model) = &mut schedules[i];
                    if model.cap.is_some() {
                        continue;
                    }
                    let returned = client.revoke(beneficiary);
                    // Invariant 3: revocation never claws back vested tokens.
                    prop_assert_eq!(returned, model.amount - model.vested(now));
                    model.cap = Some(now);
                    total_returned += returned;
                }
            }

            // Invariant 1: contract balance covers all outstanding obligations.
            let outstanding: i128 = schedules
                .iter()
                .map(|(_, m)| m.vested(u32::MAX) - m.claimed)
                .sum();
            prop_assert!(token.balance(&client.address) >= outstanding);
        }

        // Global conservation: everything allocated is claimed, returned, or held.
        let total_allocated: i128 = params.iter().map(|p| p.0).sum();
        prop_assert_eq!(
            total_claimed + total_returned + token.balance(&client.address),
            total_allocated
        );
    }

    /// Linear release invariant against the reference model.
    #[test]
    fn prop_linear_release_matches_model(
        amount in 1i128..=1_000_000,
        cliff_offset in 0u32..=1_000,
        duration in 1u32..=10_000,
        probes in prop::collection::vec(0u32..=20_000, 1..=16),
    ) {
        let env = Env::default();
        let (client, _token, schedules) = setup(&env, &[(amount, cliff_offset, duration)]);
        let (beneficiary, model) = &schedules[0];

        for probe in probes.iter() {
            let ledger = BASE_LEDGER + probe;
            env.ledger().with_mut(|l| l.sequence_number = ledger);
            prop_assert_eq!(client.claimable(beneficiary), model.claimable(ledger));
        }
    }

    // Regression tests for issue #1138: large token supplies with long
    // durations must not overflow `amount * elapsed` in `vested_amount`.
    // 10 billion tokens at 18 decimals = 1e28 units; 10 years of ledgers
    // (~6.3e7) would push the unchecked product past i128::MAX.
    #[test]
    fn prop_vested_amount_large_supply_no_overflow(
        // 1e28 units (10 billion tokens @ 18 decimals) up to i128::MAX.
        amount in 10_000_000_000_000_000_000_000_000_000i128..=i128::MAX,
        // Long durations: up to ~10 years of ledgers.
        duration in 1u32..=63_000_000u32,
        checkpoint_pct in 0u32..=100u32,
    ) {
        let cliff = 1u32;
        let end = cliff + duration;
        let checkpoint = cliff + (u64::from(duration) * u64::from(checkpoint_pct) / 100) as u32;

        // Must not panic and must match the exact linear interpolation.
        let vested = vested_amount(amount, cliff, end, checkpoint).unwrap();
        let expected = if checkpoint < cliff {
            0
        } else if checkpoint >= end {
            amount
        } else {
            // Reference computation using 256-bit wide math to avoid the
            // very overflow we are guarding against.
            wide_mul_div(amount, checkpoint - cliff, duration)
        };
        prop_assert_eq!(vested, expected);
    }

    #[test]
    fn prop_vested_amount_max_supply_exact_linear(
        // Maximum supply allocations with long durations.
        amount in 10_000_000_000_000_000_000_000_000_000i128..=i128::MAX,
        duration in 1u32..=63_000_000u32,
    ) {
        let cliff = 1u32;
        let end = cliff + duration;

        // At the end ledger the full amount must vest exactly.
        prop_assert_eq!(vested_amount(amount, cliff, end, end).unwrap(), amount);
        // At the cliff nothing has vested yet.
        prop_assert_eq!(vested_amount(amount, cliff, end, cliff).unwrap(), 0);
        // Halfway through, the result must be the exact floor of the
        // linear interpolation computed with wide arithmetic.
        let mid = cliff + duration / 2;
        let expected = wide_mul_div(amount, duration / 2, duration);
        prop_assert_eq!(vested_amount(amount, cliff, end, mid).unwrap(), expected);
    }
}
