#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient, TokenInterface},
};

// ---------------------------------------------------------------------------
// MockToken — no-op token for unit tests.
// ---------------------------------------------------------------------------

#[contract]
pub struct MockToken;

#[contractimpl]
impl TokenInterface for MockToken {
    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn balance(_env: Env, _id: Address) -> i128 {
        i128::MAX
    }
    fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}
    fn burn(_env: Env, _from: Address, _amount: i128) {}
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {}
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> String {
        String::from_str(&env, "Mock")
    }
    fn symbol(env: Env) -> String {
        String::from_str(&env, "MCK")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (TimelockContractClient, Address, Address, Address, u32, i128) {
    let admin = Address::generate(env);
    let beneficiary = Address::generate(env);
    let token = env.register_contract(None, MockToken);
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 100;
    let amount = 1_000i128;

    let contract_addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(env, &contract_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);

    (client, admin, beneficiary, token, release_ledger, amount)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, beneficiary, token, release_ledger, amount) = setup(&env);

    let info = client.get_info();
    assert_eq!(info.admin, admin);
    assert_eq!(info.beneficiary, beneficiary);
    assert_eq!(info.token, token);
    assert_eq!(info.release_ledger, release_ledger);
    assert_eq!(info.amount, amount);
    assert_eq!(info.state, TimelockState::Active);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);
    let release_ledger = env.ledger().sequence() + 100;

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_initialize_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);
    let release_ledger = env.ledger().sequence() + 100;

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_initialize_past_release_ledger_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 200);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);
    client.initialize(&admin, &token, &beneficiary, &50, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_initialize_within_min_deadline_buffer_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    // One ledger short of the minimum buffer must be rejected.
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER - 1;
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);
}

#[test]
fn test_initialize_at_min_deadline_buffer_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    // Exactly the minimum buffer must be accepted.
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER;
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);

    let info = client.get_info();
    assert_eq!(info.release_ledger, release_ledger);
    assert_eq!(info.state, TimelockState::Active);
}

#[test]
fn test_release_after_ledger() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();

    assert_eq!(client.get_info().state, TimelockState::Released);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_release_too_early_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup(&env);
    // release_ledger is current + 100; we're still before it.
    client.release();
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_release_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);
    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();
    client.release();
}

#[test]
fn test_cancel_by_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup(&env);
    client.cancel();

    assert_eq!(client.get_info().state, TimelockState::Cancelled);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_cancel_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup(&env);
    client.cancel();
    client.cancel();
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_cancel_after_release_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);
    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();
    client.cancel();
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_release_after_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);
    client.cancel();
    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();
}

#[test]
fn test_is_releasable() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);
    assert!(!client.is_releasable());

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    assert!(client.is_releasable());
}

#[test]
fn test_reassign_beneficiary() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, old_beneficiary, _, _, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    let info = client.get_info();
    assert_eq!(info.beneficiary, old_beneficiary);

    // Reassign to a new beneficiary
    client.reassign_beneficiary(&new_beneficiary);

    let updated_info = client.get_info();
    assert_eq!(updated_info.beneficiary, new_beneficiary);
    assert_eq!(updated_info.state, TimelockState::Active);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_reassign_beneficiary_after_release_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, release_ledger, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();

    // Should fail because already released
    client.reassign_beneficiary(&new_beneficiary);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_reassign_beneficiary_after_cancel_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, _, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    client.cancel();

    // Should fail because already cancelled
    client.reassign_beneficiary(&new_beneficiary);
}

// ── Two-step beneficiary reassignment tests ──────────────────────────────

#[test]
fn test_propose_and_accept_beneficiary() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, old_beneficiary, _, _, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    // Proposing must not change the stored beneficiary yet.
    client.propose_beneficiary(&new_beneficiary);
    assert_eq!(client.get_info().beneficiary, old_beneficiary);

    // Acceptance updates the stored beneficiary.
    client.accept_beneficiary();
    assert_eq!(client.get_info().beneficiary, new_beneficiary);
    assert_eq!(client.get_info().state, TimelockState::Active);
}

#[test]
#[should_panic]
fn test_propose_beneficiary_requires_current_beneficiary_auth() {
    let env = Env::default();

    let (client, _, old_beneficiary, _, _, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    // Only the new beneficiary authorizes; the current beneficiary does not.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &new_beneficiary,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "propose_beneficiary",
            args: (new_beneficiary.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.propose_beneficiary(&new_beneficiary);
    let _ = old_beneficiary;
}

#[test]
#[should_panic]
fn test_accept_beneficiary_requires_new_beneficiary_auth() {
    let env = Env::default();

    let (client, _, old_beneficiary, _, _, _) = setup(&env);
    let new_beneficiary = Address::generate(&env);

    client.propose_beneficiary(&new_beneficiary);

    // Only the current beneficiary authorizes; the proposed beneficiary does not.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &old_beneficiary,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "accept_beneficiary",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.accept_beneficiary();
}

#[test]
fn test_release_pays_new_beneficiary_after_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let new_beneficiary = Address::generate(&env);
    let amount = 1_000i128;
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 100;

    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);

    client.propose_beneficiary(&new_beneficiary);
    client.accept_beneficiary();
    assert_eq!(client.get_info().beneficiary, new_beneficiary);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();

    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&new_beneficiary), amount);
    assert_eq!(token_client.balance(&beneficiary), 0);
}

// ── Early emergency release with penalty tests ───────────────────────────

#[test]
fn test_early_release_with_penalty_distribution() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let penalty_recipient = Address::generate(&env);
    let amount = 10_000i128;
    let bps = 500u32; // 5%
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 100;

    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);
    client.set_early_release_config(&Some(bps), &Some(penalty_recipient.clone()));

    // Still before the release ledger.
    client.early_release_with_penalty(&beneficiary);

    let expected_penalty = amount * bps as i128 / 10_000;
    let expected_remainder = amount - expected_penalty;

    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&penalty_recipient), expected_penalty);
    assert_eq!(token_client.balance(&beneficiary), expected_remainder);
    assert_eq!(client.get_info().state, TimelockState::Released);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_early_release_without_config_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, beneficiary, _, _, _) = setup(&env);

    // No early release config set — must fail.
    client.early_release_with_penalty(&beneficiary);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_set_early_release_config_invalid_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, _, _) = setup(&env);
    let penalty_recipient = Address::generate(&env);

    // bps greater than 10_000 must be rejected.
    client.set_early_release_config(&Some(10_001u32), &Some(penalty_recipient));
}

// ── Multi-tranche release tests ──────────────────────────────────────────

#[test]
fn test_initialize_with_tranches() {
    use soroban_sdk::Vec as SdkVec;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let mut tranches = SdkVec::new(&env);
    let ledger1 = env.ledger().sequence() + 50;
    let ledger2 = env.ledger().sequence() + 100;
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger1,
        amount: 500,
    });
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger2,
        amount: 500,
    });

    client.initialize_with_tranches(&admin, &token, &beneficiary, &tranches);

    let info = client.get_info();
    assert_eq!(info.tranches.len(), 2);
    assert_eq!(info.amount, 1_000);
    assert_eq!(info.state, TimelockState::Active);
}

#[test]
fn test_multi_tranche_partial_release() {
    use soroban_sdk::Vec as SdkVec;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let mut tranches = SdkVec::new(&env);
    let ledger1 = env.ledger().sequence() + 50;
    let ledger2 = env.ledger().sequence() + 100;
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger1,
        amount: 400,
    });
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger2,
        amount: 600,
    });

    client.initialize_with_tranches(&admin, &token, &beneficiary, &tranches);

    // Advance to ledger1 - should release first tranche
    env.ledger().with_mut(|l| l.sequence_number = ledger1);
    client.release();

    let info = client.get_info();
    assert_eq!(info.state, TimelockState::Active); // Not yet fully released

    // Advance to ledger2 - should release second tranche
    env.ledger().with_mut(|l| l.sequence_number = ledger2);
    client.release();

    let info = client.get_info();
    assert_eq!(info.state, TimelockState::Released); // Now fully released
}

#[test]
fn test_multi_tranche_is_releasable() {
    use soroban_sdk::Vec as SdkVec;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let mut tranches = SdkVec::new(&env);
    let ledger1 = env.ledger().sequence() + 50;
    let ledger2 = env.ledger().sequence() + 100;
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger1,
        amount: 500,
    });
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger2,
        amount: 500,
    });

    client.initialize_with_tranches(&admin, &token, &beneficiary, &tranches);

    assert!(!client.is_releasable());

    env.ledger().with_mut(|l| l.sequence_number = ledger1);
    assert!(client.is_releasable());

    env.ledger().with_mut(|l| l.sequence_number = ledger2);
    assert!(client.is_releasable());
}

// ── Yield-generating timelock integration tests ──────────────────────────

/// Deploy a real Stellar asset contract and mint `amount` to `to`.
fn deploy_token(env: &Env, admin: &Address, to: &Address, amount: i128) -> Address {
    let token = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token.address();
    StellarAssetClient::new(env, &token_addr).mint(to, &amount);
    token_addr
}

/// Deploy a staking contract with `unbonding_period` and return its address.
fn deploy_staking(env: &Env, unbonding_period: u32) -> Address {
    let staking_addr = env.register_contract(None, staking::StakingContract);
    let staking_client = staking::StakingContractClient::new(env, &staking_addr);
    staking_client.initialize(&unbonding_period);
    staking_addr
}

#[test]
fn test_yield_timelock_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let yield_recipient = Address::generate(&env);

    let amount = 10_000i128;
    let unbonding_period = 10u32;
    let release_ledger = env.ledger().sequence() + 200;

    // Real test token funded to the timelock contract.
    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);

    // Staking contract with an unbonding period.
    let staking_addr = deploy_staking(&env, unbonding_period);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);
    client.set_yield_config(&staking_addr, &yield_recipient);

    // Deposit locked balance into the staking pool.
    client.deposit_locked_balance();

    let staking_client = staking::StakingContractClient::new(&env, &staking_addr);
    assert_eq!(staking_client.total_staked(), amount);

    // Advance and withdraw/unbond prior to release.
    env.ledger().with_mut(|l| l.sequence_number += 1);
    client.withdraw_and_unbond();

    // Tokens returned to the timelock after unbonding.
    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&timelock_addr), amount);

    // Advance past the release ledger and release.
    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release();

    assert_eq!(client.get_info().state, TimelockState::Released);
    // Principal to beneficiary; accrued yield to the designated recipient.
    assert_eq!(token_client.balance(&beneficiary), amount);
    assert!(token_client.balance(&yield_recipient) >= 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_deposit_without_staking_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let amount = 1_000i128;
    let release_ledger = env.ledger().sequence() + 100;

    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);

    // No yield config set — deposit must fail.
    client.deposit_locked_balance();
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_withdraw_without_staking_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let amount = 1_000i128;
    let release_ledger = env.ledger().sequence() + 100;

    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);

    // No yield config set — withdraw must fail.
    client.withdraw_and_unbond();
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_double_deposit_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let yield_recipient = Address::generate(&env);
    let amount = 1_000i128;
    let release_ledger = env.ledger().sequence() + 100;

    let timelock_addr = env.register_contract(None, TimelockContract);
    let token = deploy_token(&env, &admin, &timelock_addr, amount);
    let staking_addr = deploy_staking(&env, 5);

    let client = TimelockContractClient::new(&env, &timelock_addr);
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &amount);
    client.set_yield_config(&staking_addr, &yield_recipient);

    client.deposit_locked_balance();
    // Second deposit must fail.
    client.deposit_locked_balance();
// ── Persistent-storage multi-tranche tests ───────────────────────────────

/// Builds a client with `n` tranches of `amount_each`, spaced `spacing`
/// ledgers apart starting at `first_ledger`.
fn setup_multi_tranche(
    env: &Env,
    n: u32,
    amount_each: i128,
    first_ledger: u32,
    spacing: u32,
) -> (TimelockContractClient, Address, Address, Address, SdkVec<ReleaseTranche>) {
// ── Mutual tranche rescheduling tests ────────────────────────────────────

fn setup_with_tranches(
    env: &Env,
) -> (
    TimelockContractClient,
    Address,
    Address,
    Address,
    u32,
    u32,
) {
    use soroban_sdk::Vec as SdkVec;

    let admin = Address::generate(env);
    let beneficiary = Address::generate(env);
    let token = env.register_contract(None, MockToken);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(env, &addr);

    let mut tranches = SdkVec::new(env);
    for i in 0..n {
        tranches.push_back(ReleaseTranche {
            release_ledger: first_ledger + i * spacing,
            amount: amount_each,
        });
    }

    client.initialize_with_tranches(&admin, &token, &beneficiary, &tranches);

    (client, admin, beneficiary, token, tranches)
}

#[test]
fn test_persistent_tranches_read_back_via_get_info() {
    let env = Env::default();
    env.mock_all_auths();

    let first_ledger = env.ledger().sequence() + 10;
    let (client, _, _, _, tranches) = setup_multi_tranche(&env, 48, 100, first_ledger, 30);

    let info = client.get_info();
    assert_eq!(info.tranches.len(), 48);
    assert_eq!(info.amount, 48 * 100);
    assert_eq!(info.release_ledger, first_ledger + 47 * 30);
    assert_eq!(info.state, TimelockState::Active);

    // Every tranche must round-trip through persistent storage unchanged.
    for i in 0..48u32 {
        let expected = tranches.get(i).unwrap();
        let actual = info.tranches.get(i).unwrap();
        assert_eq!(actual.release_ledger, expected.release_ledger);
        assert_eq!(actual.amount, expected.amount);
    }
}

#[test]
fn test_persistent_tranche_partial_then_full_release() {
    let env = Env::default();
    env.mock_all_auths();

    let first_ledger = env.ledger().sequence() + 10;
    let (client, _, _, _, tranches) = setup_multi_tranche(&env, 3, 250, first_ledger, 20);

    // Before any ledger: nothing releasable, still active.
    assert!(!client.is_releasable());
    assert_eq!(client.get_info().state, TimelockState::Active);

    // Release tranche 0.
    let l0 = tranches.get(0).unwrap().release_ledger;
    env.ledger().with_mut(|l| l.sequence_number = l0);
    assert!(client.is_releasable());
    client.release();
    assert_eq!(client.get_info().state, TimelockState::Active);

    // Release tranche 1.
    let l1 = tranches.get(1).unwrap().release_ledger;
    env.ledger().with_mut(|l| l.sequence_number = l1);
    assert!(client.is_releasable());
    client.release();
    assert_eq!(client.get_info().state, TimelockState::Active);

    // Release tranche 2 — now fully released.
    let l2 = tranches.get(2).unwrap().release_ledger;
    env.ledger().with_mut(|l| l.sequence_number = l2);
    assert!(client.is_releasable());
    client.release();
    assert_eq!(client.get_info().state, TimelockState::Released);

    // Once fully released, no further tranche is releasable.
    assert!(!client.is_releasable());
}

#[test]
fn test_persistent_tranche_is_releasable_per_tranche() {
    let env = Env::default();
    env.mock_all_auths();

    let first_ledger = env.ledger().sequence() + 10;
    let (client, _, _, _, tranches) = setup_multi_tranche(&env, 4, 100, first_ledger, 25);

    // Before the first tranche ledger, nothing is releasable.
    assert!(!client.is_releasable());

    // At each tranche ledger, is_releasable must be true until all are released.
    for i in 0..4u32 {
        let ledger = tranches.get(i).unwrap().release_ledger;
        env.ledger().with_mut(|l| l.sequence_number = ledger);
        assert!(client.is_releasable(), "tranche {i} should be releasable");
        client.release();
    }

    // All tranches released: no longer releasable.
    assert!(!client.is_releasable());
    assert_eq!(client.get_info().state, TimelockState::Released);
}

#[test]
fn test_persistent_tranche_lookup_extends_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let first_ledger = env.ledger().sequence() + 10;
    let (client, _, _, _, tranches) = setup_multi_tranche(&env, 5, 200, first_ledger, 40);

    // Advance the ledger well past the initial TTL window but before the
    // final tranche's release ledger, then read the tranches back. The
    // lookup must extend the TTL so entries remain accessible.
    let mid_ledger = first_ledger + 2 * 40;
    env.ledger().with_mut(|l| l.sequence_number = mid_ledger);

    let info = client.get_info();
    assert_eq!(info.tranches.len(), 5);
    for i in 0..5u32 {
        let expected = tranches.get(i).unwrap();
        let actual = info.tranches.get(i).unwrap();
        assert_eq!(actual.release_ledger, expected.release_ledger);
        assert_eq!(actual.amount, expected.amount);
    }

    // Advance further and confirm entries are still accessible after the
    // TTL extension performed by the previous lookup.
    let later_ledger = first_ledger + 4 * 40;
    env.ledger().with_mut(|l| l.sequence_number = later_ledger);

    let info2 = client.get_info();
    assert_eq!(info2.tranches.len(), 5);
    for i in 0..5u32 {
        let expected = tranches.get(i).unwrap();
        let actual = info2.tranches.get(i).unwrap();
        assert_eq!(actual.release_ledger, expected.release_ledger);
        assert_eq!(actual.amount, expected.amount);
    }

    // Release the final tranche and confirm the release status persisted.
    env.ledger().with_mut(|l| l.sequence_number = later_ledger);
    client.release();
    assert_eq!(client.get_info().state, TimelockState::Released);
    let ledger1 = env.ledger().sequence() + 50;
    let ledger2 = env.ledger().sequence() + 100;

    let mut tranches = SdkVec::new(env);
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger1,
        amount: 500,
    });
    tranches.push_back(ReleaseTranche {
        release_ledger: ledger2,
        amount: 500,
    });

    client.initialize_with_tranches(&admin, &token, &beneficiary, &tranches);

    (client, admin, beneficiary, token, ledger1, ledger2)
}

#[test]
fn test_reschedule_tranche_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, ledger1, _) = setup_with_tranches(&env);

    let new_release_ledger = ledger1 + 25;
    client.reschedule_tranche(&0, &new_release_ledger);

    let info = client.get_info();
    assert_eq!(info.tranches.get(0).unwrap().release_ledger, new_release_ledger);
    assert_eq!(info.state, TimelockState::Active);

    // A TrancheRescheduled event must have been emitted.
    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        topics
            .iter()
            .any(|t| t == &TrancheRescheduled { tranche_id: 0, new_release_ledger }.into())
    });
    assert!(found, "TrancheRescheduled event not emitted");
}

#[test]
#[should_panic]
fn test_reschedule_tranche_requires_both_parties() {
    let env = Env::default();

    let (client, admin, _, _, ledger1, _) = setup_with_tranches(&env);

    // Only the admin authorizes; the beneficiary does not.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "reschedule_tranche",
            args: (0u32, ledger1 + 25u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.reschedule_tranche(&0, &(ledger1 + 25));
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_reschedule_released_tranche_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, ledger1, _) = setup_with_tranches(&env);

    // Release the first tranche.
    env.ledger().with_mut(|l| l.sequence_number = ledger1);
    client.release();

    // Attempting to reschedule an already-released tranche must fail.
    client.reschedule_tranche(&0, &(ledger1 + 50));
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_reschedule_tranche_invalid_ledger_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, ledger1, _) = setup_with_tranches(&env);

    // new_release_ledger must be strictly greater than the current ledger.
    client.reschedule_tranche(&0, &ledger1);
}

// ── Multi-token locking tests ────────────────────────────────────────────

/// Deploy a fresh MockToken and return its address.
fn mock_token(env: &Env) -> Address {
    env.register_contract(None, MockToken)
}

#[test]
fn test_multi_token_deposit_tracks_balances_separately() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let token_a = mock_token(&env);
    let token_b = mock_token(&env);
    let token_c = mock_token(&env);

    let ledger_a = env.ledger().sequence() + 50;
    let ledger_b = env.ledger().sequence() + 100;
    let ledger_c = env.ledger().sequence() + 150;

    client.initialize(&admin, &token_a, &beneficiary, &ledger_a, &1_000);
    client.deposit(&token_b, &2_000, &ledger_b);
    client.deposit(&token_c, &3_000, &ledger_c);

    // Each token's balance and schedule must be tracked independently.
    assert_eq!(client.get_locked_balance(&token_a), 1_000);
    assert_eq!(client.get_locked_balance(&token_b), 2_000);
    assert_eq!(client.get_locked_balance(&token_c), 3_000);

    assert_eq!(client.get_release_ledger(&token_a), ledger_a);
    assert_eq!(client.get_release_ledger(&token_b), ledger_b);
    assert_eq!(client.get_release_ledger(&token_c), ledger_c);
}

#[test]
fn test_multi_token_independent_release_schedules() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let token_a = mock_token(&env);
    let token_b = mock_token(&env);

    let ledger_a = env.ledger().sequence() + 50;
    let ledger_b = env.ledger().sequence() + 100;

    client.initialize(&admin, &token_a, &beneficiary, &ledger_a, &1_000);
    client.deposit(&token_b, &2_000, &ledger_b);

    // Before either schedule matures, nothing is claimable.
    assert_eq!(client.claimable_amount(&token_a), 0);
    assert_eq!(client.claimable_amount(&token_b), 0);

    // Advance to token A's ledger: only A matures.
    env.ledger().with_mut(|l| l.sequence_number = ledger_a);
    assert_eq!(client.claimable_amount(&token_a), 1_000);
    assert_eq!(client.claimable_amount(&token_b), 0);

    client.release(&token_a);
    assert_eq!(client.get_locked_balance(&token_a), 0);
    // Token B remains untouched by A's release.
    assert_eq!(client.get_locked_balance(&token_b), 2_000);

    // Advance to token B's ledger: B matures independently.
    env.ledger().with_mut(|l| l.sequence_number = ledger_b);
    assert_eq!(client.claimable_amount(&token_b), 2_000);
    client.release(&token_b);
    assert_eq!(client.get_locked_balance(&token_b), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_multi_token_release_too_early_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let token_a = mock_token(&env);
    let token_b = mock_token(&env);

    let ledger_a = env.ledger().sequence() + 50;
    let ledger_b = env.ledger().sequence() + 100;

    client.initialize(&admin, &token_a, &beneficiary, &ledger_a, &1_000);
    client.deposit(&token_b, &2_000, &ledger_b);

    // Token A is mature but token B is not; releasing B must fail.
    env.ledger().with_mut(|l| l.sequence_number = ledger_a);
    client.release(&token_b);
}

// ── Partial release tests ────────────────────────────────────────────────

#[test]
fn test_release_partial_multiple_claims_from_single_tranche() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = mock_token(&env);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let release_ledger = env.ledger().sequence() + 100;
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);

    // Mature the single tranche.
    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    assert_eq!(client.claimable_amount(&token), 1_000);

    // First partial claim.
    client.release_partial(&token, &300);
    assert_eq!(client.claimable_amount(&token), 700);
    assert_eq!(client.get_locked_balance(&token), 700);

    // Second partial claim from the same matured tranche.
    client.release_partial(&token, &200);
    assert_eq!(client.claimable_amount(&token), 500);
    assert_eq!(client.get_locked_balance(&token), 500);

    // Final partial claim drains the tranche.
    client.release_partial(&token, &500);
    assert_eq!(client.claimable_amount(&token), 0);
    assert_eq!(client.get_locked_balance(&token), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_release_partial_exceeds_claimable_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = mock_token(&env);

    let addr = env.register_contract(None, TimelockContract);
    let client = TimelockContractClient::new(&env, &addr);

    let release_ledger = env.ledger().sequence() + 100;
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);

    // Requesting more than the matured claimable balance must fail.
    client.release_partial(&token, &1_001);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_release_partial_before_maturity_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token, _, _) = setup(&env);

    // Nothing has matured yet, so any partial release must fail.
    client.release_partial(&token, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_release_partial_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token, release_ledger, _) = setup(&env);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release_partial(&token, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_release_partial_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token, release_ledger, _) = setup(&env);

    env.ledger()
        .with_mut(|l| l.sequence_number = release_ledger);
    client.release_partial(&token, &-100);
}
