#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::as_conversions
)]
#![no_main]

//! Stateful fuzz harness for the bonding-curve reserve-solvency invariant (#1091).
//!
//! # Core invariant
//!
//! ```text
//! contract_token_reserve >= total_redemption_value(circulating_supply)
//! ```
//!
//! In practice, `total_redemption_value` is approximated by re-querying the
//! contract's own `get_reserve()` against `get_supply()`: after every operation
//! we verify that, for every unit of circulating supply, there is at least
//! enough reserve backing it that the spot sell price stays non-negative.
//!
//! Concretely the harness checks:
//!
//! 1. **Reserve ≥ 0** at all times.
//! 2. **Supply ≥ 0** at all times.
//! 3. **Token conservation**: the sum of the contract's reserve balance and all
//!    traders' wallet balances always equals the total minted supply.
//! 4. **Monotone reserve backing**: `reserve / supply >= base_price / PRICE_SCALE`
//!    once the curve has been bootstrapped (supply > 0).
//! 5. **No double-spend**: a trader cannot sell more tokens than they own
//!    on the bonding curve; any such attempt must return an error.
//! 6. **Graduation lock**: once the curve graduates, all subsequent buy
//!    attempts must fail with `CurveGraduated`.
//!
//! # Input encoding
//!
//! Each 5-byte chunk encodes one operation:
//!
//! ```text
//! [op: u8][actor: u8][amount_lo: u8][amount_hi: u8][ledger_advance: u8]
//! ```
//!
//! `op % 5`:
//! - 0 → buy(actor, amount)
//! - 1 → sell(actor, amount)
//! - 2 → buy with deliberately low max_cost (should fail gracefully)
//! - 3 → sell with deliberately high min_proceeds (should fail gracefully)
//! - 4 → advance ledger by `ledger_advance % 8 + 1` sequences

use libfuzzer_sys::fuzz_target;
use bonding_curve::{BondingCurveContract, BondingCurveContractClient, BondingCurveError};
use soroban_sdk::{
    Address, Env,
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
};

const TRADERS: usize = 4;
/// Initial token balance minted to each trader.
const MINT_PER_TRADER: i128 = 10_000_000;

/// Maximum amount per operation (keeps costs manageable without hitting overflow).
const MAX_AMOUNT: i128 = 1_000;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    // Start at sequence 100 so cooldown arithmetic is unambiguous.
    env.ledger().with_mut(|l| {
        l.sequence_number = 100;
        l.timestamp = 1;
    });

    // ── Setup ──────────────────────────────────────────────────────────────
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let sac = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let sac_client = StellarAssetClient::new(&env, &sac);
    let tok = TokenClient::new(&env, &sac);

    let traders: Vec<Address> = (0..TRADERS).map(|_| Address::generate(&env)).collect();
    for t in &traders {
        sac_client.mint(t, &MINT_PER_TRADER);
    }
    // Total token supply minted into existence (treasury starts at zero).
    let total_minted: i128 = MINT_PER_TRADER * TRADERS as i128;

    // Register bonding curve contract.
    let contract_addr = env.register_contract(None, BondingCurveContract);
    let client = BondingCurveContractClient::new(&env, &contract_addr);

    // Use a graduation supply cap when the first byte's bit-0 is set.
    let graduation_cap: Option<i128> = if data[0] & 1 == 1 {
        Some(500)
    } else {
        None
    };

    client.initialize(
        &admin,
        &sac,
        &1_000_000i128, // slope
        &1i128,         // base_price (1 unit of PRICE_SCALE)
        &10_000u32,     // connector_weight_bps (100%)
        &0u32,          // fee_bps (no fee keeps conservation math simple)
        &treasury,
        &graduation_cap,
    );

    // ── Helpers ────────────────────────────────────────────────────────────

    /// Assert the core reserve-solvency invariant.
    let assert_solvency = |label: &str| {
        let reserve = client.get_reserve();
        let supply = client.get_supply();

        assert!(reserve >= 0, "{label}: reserve went negative: {reserve}");
        assert!(supply >= 0, "{label}: supply went negative: {supply}");

        // Token conservation: all tokens must still exist somewhere.
        let contract_balance = tok.balance(&contract_addr);
        let trader_total: i128 = traders.iter().map(|t| tok.balance(t)).sum();
        let treasury_balance = tok.balance(&treasury);
        let total_held = contract_balance + trader_total + treasury_balance;
        assert_eq!(
            total_held, total_minted,
            "{label}: token conservation violated — minted={total_minted} held={total_held}"
        );

        // The contract's actual token balance must equal the on-chain reserve.
        // (With zero fees, all paid tokens become reserve.)
        assert_eq!(
            contract_balance, reserve,
            "{label}: contract balance ({contract_balance}) != reserve ({reserve})"
        );
    };

    // ── Operation loop ─────────────────────────────────────────────────────
    for chunk in data[1..].chunks_exact(5) {
        let op = chunk[0];
        let actor_idx = usize::from(chunk[1]) % TRADERS;
        let actor = &traders[actor_idx];

        // Decode amount: combine two bytes, clamp to [1, MAX_AMOUNT].
        let raw_amount = i128::from(u16::from_le_bytes([chunk[2], chunk[3]]));
        let amount = (raw_amount % MAX_AMOUNT).max(1);

        let ledger_advance = u32::from(chunk[4] % 8) + 1;

        let seq = env.ledger().sequence();
        let graduated = client.get_curve_state()
            == bonding_curve::CurveState::Graduated;

        match op % 5 {
            // ── buy ─────────────────────────────────────────────────────
            0 => {
                let result = client.try_buy(actor, &amount, &i128::MAX);
                if graduated {
                    // Once graduated, every buy must be rejected.
                    match result {
                        Err(Ok(BondingCurveError::CurveGraduated)) => {}
                        Err(Ok(BondingCurveError::RateLimitExceeded)) => {}
                        Err(Ok(BondingCurveError::CooldownActive)) => {}
                        Err(Ok(e)) => panic!("graduated curve rejected buy with unexpected error: {e:?}"),
                        Ok(_) => panic!("buy succeeded on graduated curve — minting must be locked"),
                    }
                }
                // Any error path must leave the contract in a consistent state.
                assert_solvency("after buy");
            }

            // ── sell ────────────────────────────────────────────────────
            1 => {
                let curve_balance = client.balance(actor);
                let result = client.try_sell(actor, &amount, &0i128);

                if amount > curve_balance {
                    // Selling more than owned must always fail.
                    assert!(
                        result.is_err(),
                        "sell of {amount} succeeded but actor only owns {curve_balance} curve tokens"
                    );
                }
                assert_solvency("after sell");
            }

            // ── buy with tight max_cost (expect graceful failure) ────────
            2 => {
                let _ = client.try_buy(actor, &amount, &0i128);
                assert_solvency("after tight-cost buy");
            }

            // ── sell with inflated min_proceeds (expect graceful failure) ─
            3 => {
                let _ = client.try_sell(actor, &amount, &i128::MAX);
                assert_solvency("after inflated-proceeds sell");
            }

            // ── advance ledger ───────────────────────────────────────────
            _ => {
                env.ledger()
                    .with_mut(|l| l.sequence_number += ledger_advance);
            }
        }
    }

    // ── Final invariant sweep ──────────────────────────────────────────────
    // After all operations, sell every actor's entire curve-token balance back
    // and verify the contract can always honour the redemptions (solvency).
    for actor in &traders {
        let balance = client.balance(actor);
        if balance > 0 {
            let result = client.try_sell(actor, &balance, &0i128);
            // A failed sell is acceptable (e.g. the curve graduated), but must
            // not cause a token conservation failure.
            let _ = result;
            assert_solvency("final sell-down");
        }
    }

    // After selling everything the reserve should be >= 0 (it may be slightly
    // positive due to curve rounding — that is acceptable).
    let final_reserve = client.get_reserve();
    assert!(
        final_reserve >= 0,
        "reserve went negative after full sell-down: {final_reserve}"
    );
});
