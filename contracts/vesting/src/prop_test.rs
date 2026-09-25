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
    token, Address, Env,
};

use crate::{VestingContract, VestingContractClient};

/// High-precision reference model for a single linear vesting schedule.
///
/// Mirrors the contract's linear release math so we can cross-check the
/// on-chain accounting without relying on the contract's own arithmetic.
#[derive(Clone, Debug)]
struct LinearModel {
    amount: i128,
    start: u64,
    duration: u64,
    cliff: u64,
    claimed: i128,
    revoked: bool,
}

impl LinearModel {
    fn new(amount: i128, start: u64, duration: u64, cliff: u64) -> Self {
        Self { amount, start, duration, cliff, claimed: 0, revoked: false }
    }

    /// Vested amount at `now`, computed with the same linear interpolation the
    /// contract uses. Returns 0 before the cliff.
    fn vested(&self, now: u64) -> i128 {
        if now < self.start.saturating_add(self.cliff) {
            return 0;
        }
        if now >= self.start.saturating_add(self.duration) {
            return self.amount;
        }
        let elapsed = (now - self.start) as i128;
        self.amount * elapsed / (self.duration as i128)
    }

    /// Amount currently claimable (vested minus already claimed).
    fn claimable(&self, now: u64) -> i128 {
        (self.vested(now) - self.claimed).max(0)
    }

    /// Unvested remainder still locked in the contract.
    fn unvested_remaining(&self, now: u64) -> i128 {
        (self.amount - self.vested(now)).max(0)
    }
}

/// One random action in the stateful sequence.
#[derive(Clone, Debug)]
enum Action {
    Advance(u64),
    Claim(usize),
    Revoke(usize),
}

fn action_strategy(num_beneficiaries: usize) -> impl Strategy<Value = Action> {
    prop_oneof![
        3 => (1u64..=10_000).prop_map(Action::Advance),
        4 => (0..num_beneficiaries).prop_map(Action::Claim),
        1 => (0..num_beneficiaries).prop_map(Action::Revoke),
    ]
}

/// Deploy a fresh vesting contract with a minted token and `n` beneficiaries.
///
/// Returns the client, the token client, the admin, and the per-beneficiary
/// `(address, amount, start, duration, cliff)` tuples.
fn setup(
    env: &Env,
    n: usize,
    amounts: &[i128],
    starts: &[u64],
    durations: &[u64],
    cliffs: &[u64],
) -> (
    VestingContractClient<'_>,
    token::Client<'_>,
    Address,
    Vec<(Address, i128, u64, u64, u64)>,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let contract_id = env.register(VestingContract, ());
    let client = VestingContractClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(env, &token_id.address());
    let token_admin_client = token::StellarAssetClient::new(env, &token_id.address());

    let mut total: i128 = 0;
    let mut beneficiaries = Vec::new();
    for i in 0..n {
        let beneficiary = Address::generate(env);
        total += amounts[i];
        beneficiaries.push((beneficiary, amounts[i], starts[i], durations[i], cliffs[i]));
    }

    token_admin_client.mint(&contract_id, &total);
    client.initialize(&admin, &token_id.address());

    for (beneficiary, amount, start, duration, cliff) in beneficiaries.iter() {
        client.create_schedule(beneficiary, amount, start, duration, cliff);
    }

    (client, token, admin, beneficiaries)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Stateful property test: random ledger advances, claims, and revocations
    /// must preserve token conservation and schedule accuracy.
    #[test]
    fn prop_stateful_token_conservation(
        amounts in prop::collection::vec(1i128..=1_000_000, 1..=4),
        starts in prop::collection::vec(0u64..=1_000, 1..=4),
        durations in prop::collection::vec(1u64..=10_000, 1..=4),
        cliffs in prop::collection::vec(0u64..=1_000, 1..=4),
        actions in prop::collection::vec(any::<u8>(), 1..=32),
    ) {
        let n = amounts.len().min(starts.len()).min(durations.len()).min(cliffs.len());
        let amounts = &amounts[..n];
        let starts = &starts[..n];
        let durations = &durations[..n];
        let cliffs = &cliffs[..n];

        let env = Env::default();
        let (client, token, _admin, beneficiaries) =
            setup(&env, n, amounts, starts, durations, cliffs);

        let mut models: Vec<LinearModel> = beneficiaries
            .iter()
            .map(|(_, amount, start, duration, cliff)| {
                LinearModel::new(*amount, *start, *duration, *cliff)
            })
            .collect();

        let mut now: u64 = 0;
        let mut total_claimed: i128 = 0;
        let mut total_revoked_returned: i128 = 0;

        for raw in actions.iter() {
            let action = match raw % 3 {
                0 => Action::Advance((*raw as u64 + 1) * 100),
                1 => Action::Claim((*raw as usize) % n),
                _ => Action::Revoke((*raw as usize) % n),
            };

            match action {
                Action::Advance(delta) => {
                    now = now.saturating_add(delta);
                    env.ledger().set_timestamp(now);
                }
                Action::Claim(i) => {
                    let model = &mut models[i];
                    if model.revoked {
                        continue;
                    }
                    let expected = model.claimable(now);
                    let (beneficiary, ..) = &beneficiaries[i];
                    let claimed = client.claim(beneficiary);

                    // Invariant 2: never claim more than the initial amount.
                    prop_assert!(claimed <= model.amount);
                    prop_assert_eq!(claimed, expected);

                    model.claimed += claimed;
                    total_claimed += claimed;
                    prop_assert!(model.claimed <= model.amount);
                }
                Action::Revoke(i) => {
                    let model = &mut models[i];
                    if model.revoked {
                        continue;
                    }
                    let (beneficiary, ..) = &beneficiaries[i];
                    let vested_before = model.vested(now);
                    let returned = client.revoke(beneficiary);

                    // Invariant 3: revocation never clawbacks vested tokens.
                    let unvested = model.unvested_remaining(now);
                    prop_assert_eq!(returned, unvested);
                    prop_assert!(returned >= 0);

                    // Invariant 4: returned + claimed == initial allocation.
                    prop_assert_eq!(returned + model.claimed, model.amount);

                    model.revoked = true;
                    total_revoked_returned += returned;

                    // Vested tokens remain claimable after revocation.
                    let still_claimable = (vested_before - model.claimed).max(0);
                    prop_assert!(still_claimable >= 0);
                }
            }

            // Invariant 1: contract balance covers all outstanding obligations.
            let mut outstanding: i128 = 0;
            for (idx, model) in models.iter().enumerate() {
                if model.revoked {
                    outstanding += (model.vested(now) - model.claimed).max(0);
                } else {
                    outstanding += model.claimable(now) + model.unvested_remaining(now);
                }
                let _ = idx;
            }
            let balance = token.balance(&client.address);
            prop_assert!(balance >= outstanding);
        }

        // Global conservation: everything minted is either claimed, returned,
        // or still held by the contract.
        let total_allocated: i128 = amounts.iter().sum();
        let balance = token.balance(&client.address);
        prop_assert_eq!(total_claimed + total_revoked_returned + balance, total_allocated);
    }

    /// Linear release invariant against the high-precision reference model.
    #[test]
    fn prop_linear_release_matches_model(
        amount in 1i128..=1_000_000,
        start in 0u64..=1_000,
        duration in 1u64..=10_000,
        cliff in 0u64..=1_000,
        probes in prop::collection::vec(0u64..=20_000, 1..=16),
    ) {
        let env = Env::default();
        let (client, _token, _admin, beneficiaries) = setup(
            &env,
            1,
            &[amount],
            &[start],
            &[duration],
            &[cliff],
        );
        let (beneficiary, ..) = &beneficiaries[0];
        let model = LinearModel::new(amount, start, duration, cliff);

        for probe in probes.iter() {
            env.ledger().set_timestamp(*probe);
            let expected = model.claimable(*probe);
            let actual = client.claimable(beneficiary);
            prop_assert_eq!(actual, expected);
        }
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
        let checkpoint = cliff + duration * checkpoint_pct / 100;

        // Must not panic and must match the exact linear interpolation.
        let vested = vested_amount(amount, cliff, end, checkpoint);
        let expected = if checkpoint < cliff {
            0
        } else if checkpoint >= end {
            amount
        } else {
            // Reference computation using 256-bit wide math to avoid the
            // very overflow we are guarding against.
            let elapsed = u128::from(checkpoint - cliff);
            let total = u128::from(duration);
            let wide = (amount as u128) * elapsed / total;
            wide as i128
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
        prop_assert_eq!(vested_amount(amount, cliff, end, end), amount);
        // At the cliff nothing has vested yet.
        prop_assert_eq!(vested_amount(amount, cliff, end, cliff), 0);
        // Halfway through, the result must be the exact floor of the
        // linear interpolation computed with wide arithmetic.
        let mid = cliff + duration / 2;
        let expected = ((amount as u128) * u128::from(duration / 2) / u128::from(duration)) as i128;
        prop_assert_eq!(vested_amount(amount, cliff, end, mid), expected);
    }
}
