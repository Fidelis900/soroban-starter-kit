#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use super::*;
use soroban_sdk::token::TokenInterface;
use soroban_sdk::{
    Address, Env, FromVal, IntoVal, String, Symbol, Vec,
    testutils::{Address as _, Events as _, Ledger as _},
};

// ---------------------------------------------------------------------------
// MockToken — a no-op token contract so cross-contract calls don't panic.
// Balance defaults to i128::MAX; set DataKey::Balance(addr) to override.
// ---------------------------------------------------------------------------

#[contract]
pub struct MockToken;

#[contractimpl]
impl TokenInterface for MockToken {
    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }

    fn approve(
        _env: Env,
        _from: Address,
        _spender: Address,
        _amount: i128,
        _expiration_ledger: u32,
    ) {
    }

    fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .get::<Address, i128>(&id)
            .unwrap_or(i128::MAX)
    }

    fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}

    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}

    fn burn(_env: Env, _from: Address, _amount: i128) {}

    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {}

    fn decimals(_env: Env) -> u32 {
        18
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

fn create_escrow_contract<'a>(env: &'a Env) -> (EscrowContractClient<'a>, Address) {
    let contract_address = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &contract_address);
    (client, contract_address)
}

fn create_mock_token(env: &Env) -> Address {
    env.register_contract(None, MockToken)
}

/// Returns (client, contract_address, buyer, seller, arbiter, token, amount).
fn setup_funded_escrow<'a>(
    env: &'a Env,
) -> (
    EscrowContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
    i128,
) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, contract_address) = create_escrow_contract(env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &0, &None, &None,
    );
    client.fund();

    (
        client,
        contract_address,
        buyer,
        seller,
        arbiter,
        token,
        amount,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, contract_address) = create_escrow_contract(&env);

    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    let info = client.get_escrow_info();
    assert_eq!(info.buyer, buyer);
    assert_eq!(info.seller, seller);
    assert_eq!(info.arbiter, arbiter);
    assert_eq!(info.token_contract, token_contract);
    assert_eq!(info.amount, amount);
    assert_eq!(info.deadline, deadline);
    assert_eq!(info.state, EscrowState::Created);

    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_address.clone(),
                (Symbol::new(&env, "created"), buyer.clone(), seller.clone()).into_val(&env),
                amount.into_val(&env),
            ),
            (
                contract_address.clone(),
                (
                    Symbol::new(&env, "initialized"),
                    buyer.clone(),
                    seller.clone(),
                    arbiter.clone()
                )
                    .into_val(&env),
                amount.into_val(&env),
            ),
        ]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );
    // Second call must fail with AlreadyInitialized (#5)
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_initialize_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 10);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = 5u32; // 5 < 10, already in the past

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_initialize_buyer_equals_seller_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let same = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &same, &same, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_initialize_buyer_equals_arbiter_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let same = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &same, &seller, &same, &token, &1_000, &deadline, &0, &None, &None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_initialize_seller_equals_arbiter_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let same = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &same, &same, &token, &1_000, &deadline, &0, &None, &None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_initialize_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &0, &deadline, &0, &None, &None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_initialize_negative_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &-1, &deadline, &0, &None, &None,
    );
}

#[test]
fn test_initialize_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, seller, _, _, amount) = setup_funded_escrow(&env);

    let info = client.get_escrow_info();
    assert_eq!(info.buyer, buyer);
    assert_eq!(info.seller, seller);
    assert_eq!(info.amount, amount);
    assert_eq!(info.state, EscrowState::Funded);
}

#[test]
fn test_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, contract_address) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );
    client.fund();

    assert_eq!(client.get_state(), Some(EscrowState::Funded));

    // Verify funded event was emitted
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_address.clone(),
                (Symbol::new(&env, "created"), buyer.clone(), seller.clone()).into_val(&env),
                amount.into_val(&env),
            ),
            (
                contract_address.clone(),
                (
                    Symbol::new(&env, "initialized"),
                    buyer.clone(),
                    seller.clone(),
                    arbiter.clone()
                )
                    .into_val(&env),
                amount.into_val(&env),
            ),
            (
                contract_address.clone(),
                (Symbol::new(&env, "funded"), buyer.clone()).into_val(&env),
                amount.into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_mark_delivered() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    client.mark_delivered();

    assert_eq!(client.get_state(), Some(EscrowState::Delivered));
}

#[test]
#[should_panic]
fn test_mark_delivered_by_buyer_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, _, _, _) = setup_funded_escrow(&env);
    // Clear auths so seller auth is not present — mark_delivered requires seller
    env.set_auths(&[]);
    client.mark_delivered();
}

#[test]
#[should_panic]
fn test_mark_delivered_by_arbiter_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, _, _, _) = setup_funded_escrow(&env);
    // Clear auths so seller auth is not present — mark_delivered requires seller
    env.set_auths(&[]);
    client.mark_delivered();
}

#[test]
fn test_approve_delivery() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    client.mark_delivered();
    client.approve_delivery();

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, ..) = setup_funded_escrow(&env);
    client.raise_dispute(&buyer);

    assert_eq!(client.get_state(), Some(EscrowState::Disputed));
}

#[test]
fn test_seller_raises_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, seller, ..) = setup_funded_escrow(&env);
    client.raise_dispute(&seller);

    assert_eq!(client.get_state(), Some(EscrowState::Disputed));
}

#[test]
fn test_resolve_dispute_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, _seller, arbiter, ..) = setup_funded_escrow(&env);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_resolve_dispute_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, _seller, arbiter, ..) = setup_funded_escrow(&env);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
}

#[test]
fn test_deadline_passed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    assert_eq!(client.is_deadline_passed(), false);

    env.ledger()
        .with_mut(|li| li.sequence_number = deadline + 1);

    assert_eq!(client.is_deadline_passed(), true);
}

#[test]
fn test_get_remaining_ledgers_positive() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let current_sequence = env.ledger().sequence();
    let deadline = current_sequence + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    let remaining = client.get_remaining_ledgers();
    assert_eq!(remaining, 100i64);
    assert!(remaining > 0);
}

#[test]
fn test_get_remaining_ledgers_negative() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = create_mock_token(&env);
    let amount = 1_000i128;
    let current_sequence = env.ledger().sequence();
    let deadline = current_sequence + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token_contract,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    env.ledger()
        .with_mut(|li| li.sequence_number = deadline + 50);

    let remaining = client.get_remaining_ledgers();
    assert_eq!(remaining, -50i64);
    assert!(remaining < 0);
}

#[test]
fn test_arbiter_resolve_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _contract_address, buyer, _seller, arbiter, _, _amount) =
        setup_funded_escrow(&env);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
    assert!(!env.events().all().is_empty());
}

#[test]
fn test_arbiter_resolve_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, _seller, arbiter, ..) = setup_funded_escrow(&env);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
    assert!(!env.events().all().is_empty());
}

#[test]
#[should_panic]
fn test_initialize_invalid_token_address() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    // Use a random address that has no contract — decimals() will panic.
    let invalid_token = Address::generate(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &invalid_token,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_cancel_by_seller_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, contract_address) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &0, &None, &None,
    );

    // Only authorize the seller — buyer.require_auth() inside cancel() will fail.
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    env.mock_auths(&[MockAuth {
        address: &seller,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "cancel",
            args: soroban_sdk::vec![&env].into(),
            sub_invokes: &[],
        },
    }]);
    client.cancel();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_cancel_after_funded_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    // State is now Funded; cancel() requires Created state → InvalidState (#2).
    client.cancel();
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_fund_insufficient_funds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    // Set buyer's balance to 0 in the mock token's storage
    env.as_contract(&token, || {
        env.storage().instance().set(&buyer, &0i128);
    });

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &0, &None, &None,
    );
    // buyer has balance 0 < amount 1000 → InsufficientFunds (#7)
    client.fund();
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_get_escrow_info_uninitialized_fails() {
    let env = Env::default();
    let (client, _) = create_escrow_contract(&env);
    // Calling get_escrow_info on uninitialized contract should panic with NotInitialized (#6)
    let _ = client.get_escrow_info();
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_bump_uninitialized_fails() {
    let env = Env::default();
    let (client, _) = create_escrow_contract(&env);
    // Calling bump on uninitialized contract should fail with NotInitialized (#6)
    let _ = client.bump();
}

#[test]
fn test_bump_initialized_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    // bump should succeed on initialized escrow
    client.bump();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_fund_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    // Already Funded; calling fund again must fail with InvalidState (#2)
    client.fund();
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_request_refund_before_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    // Deadline is sequence + 100; current sequence is 0 → deadline not reached → DeadlineNotReached (#4)
    client.request_refund();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_approve_delivery_without_mark_delivered_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    // Escrow is Funded; approve_delivery requires Delivered state → InvalidState (#2)
    client.approve_delivery();
}

#[test]
#[should_panic]
fn test_approve_delivery_by_seller_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, seller, ..) = setup_funded_escrow(&env);
    client.mark_delivered();

    // Clear auths and only authorize seller — approve_delivery requires buyer auth
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let contract_address = env.register_contract(None, EscrowContract);
    env.mock_auths(&[MockAuth {
        address: &seller,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "approve_delivery",
            args: soroban_sdk::vec![&env].into(),
            sub_invokes: &[],
        },
    }]);
    client.approve_delivery();
}

#[test]
#[should_panic]
fn test_approve_delivery_by_arbiter_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, arbiter, ..) = setup_funded_escrow(&env);
    client.mark_delivered();

    // Clear auths and only authorize arbiter — approve_delivery requires buyer auth
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let contract_address = env.register_contract(None, EscrowContract);
    env.mock_auths(&[MockAuth {
        address: &arbiter,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "approve_delivery",
            args: soroban_sdk::vec![&env].into(),
            sub_invokes: &[],
        },
    }]);
    client.approve_delivery();
}

#[test]
fn test_release_partial() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, _arbiter, _token, amount) = setup_funded_escrow(&env);
    let partial = amount / 2;

    client.release_partial(&partial);

    // Verify state is still Funded
    assert_eq!(client.get_state(), Some(EscrowState::Funded));

    // Verify amount was decremented
    let info = client.get_escrow_info();
    assert_eq!(info.amount, amount - partial);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_release_partial_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &0, &None, &None,
    );

    // Try to release_partial in Created state — should fail with InvalidState
    client.release_partial(&500i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_release_partial_exceeds_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, _arbiter, _token, amount) = setup_funded_escrow(&env);

    // Try to release more than available — should fail with InsufficientFunds
    client.release_partial(&(amount + 1));
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_release_partial_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, _arbiter, _token, _amount) = setup_funded_escrow(&env);

    // Try to release zero amount — should fail with InvalidAmount
    client.release_partial(&0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_resolve_dispute_wrong_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    // Escrow is Funded but never Disputed — resolve_dispute must reject with InvalidState (#2)
    let (client, _, _, _, arbiter, ..) = setup_funded_escrow(&env);
    client.resolve_dispute(&arbiter, &true);
}

// ---------------------------------------------------------------------------
// Feature-gated tests
// ---------------------------------------------------------------------------

#[cfg(feature = "pausable")]
mod pausable_tests {
    use super::*;

    fn setup_with_admin<'a>(env: &'a Env) -> (EscrowContractClient<'a>, Address, Address) {
        let admin = Address::generate(env);
        let buyer = Address::generate(env);
        let seller = Address::generate(env);
        let arbiter = Address::generate(env);
        let token = create_mock_token(env);
        let amount = 1_000i128;
        let deadline = env.ledger().sequence() + 100;

        let (client, _contract_address) = create_escrow_contract(env);
        client.initialize(
            &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &0, &None, &None,
        );
        (client, admin, buyer)
    }

    #[test]
    fn test_pause_blocks_fund() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _buyer) = setup_with_admin(&env);

        client.pause();
        assert!(client.try_fund().is_err());
    }

    #[test]
    fn test_unpause_restores_fund() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _buyer) = setup_with_admin(&env);

        client.pause();
        client.unpause();
        client.fund();
        assert_eq!(client.get_state(), Some(EscrowState::Funded));
    }

    #[test]
    fn test_pause_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _buyer) = setup_with_admin(&env);

        client.pause();

        use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
        let all = env.events().all();
        let last = all.last().unwrap();
        let (_, topics, _) = last;
        assert_eq!(topics, (Symbol::new(&env, "paused"), admin).into_val(&env));
    }

    #[test]
    fn test_unpause_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _buyer) = setup_with_admin(&env);

        client.pause();
        client.unpause();

        use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
        let all = env.events().all();
        let last = all.last().unwrap();
        let (_, topics, _) = last;
        assert_eq!(
            topics,
            (Symbol::new(&env, "unpaused"), admin).into_val(&env)
        );
    }
}

// `propose_upgrade`/`execute_upgrade` live under the `pausable` feature gate in
// lib.rs (not a separate `upgradeable` gate, which exists in Cargo.toml but
// currently guards nothing in production code) — this module follows that.
#[cfg(feature = "pausable")]
mod upgradeable_tests {
    use super::*;
    use soroban_common::AdminKey;

    #[test]
    fn test_upgrade_requires_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, contract_address, ..) = setup_funded_escrow(&env);
        let admin = Address::generate(&env);
        env.as_contract(&contract_address, || {
            env.storage().instance().set(&AdminKey::Admin, &admin);
        });
        let dummy_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        client.propose_upgrade(&dummy_hash);
        // Auth passes; execute_upgrade still fails because the delay hasn't elapsed
        // and the hash isn't a real uploaded WASM.
        let _ = client.try_execute_upgrade();
    }

    #[test]
    fn test_upgrade_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, contract_address, ..) = setup_funded_escrow(&env);
        let admin = Address::generate(&env);
        env.as_contract(&contract_address, || {
            env.storage().instance().set(&AdminKey::Admin, &admin);
        });
        let dummy_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&dummy_hash);
        // Advance past the upgrade delay so execute_upgrade proceeds far enough
        // to emit the `upgraded` event before failing on the fake WASM hash.
        env.ledger().with_mut(|l| l.sequence_number += 20_000);
        // The upgraded event is emitted before update_current_contract_wasm, so
        // even though the call fails (invalid hash), the event is still captured.
        let _ = client.try_execute_upgrade();

        use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
        let all = env.events().all();
        // Find the upgraded event
        let found = all.iter().any(|(_, topics, _)| {
            topics == (Symbol::new(&env, "upgraded"), admin.clone()).into_val(&env)
        });
        assert!(found, "upgraded event not emitted");
    }
}

// Tests for new features
#[test]
fn test_update_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );

    // Update amount before funding
    client.update_amount(&2_000);

    let info = client.get_escrow_info();
    assert_eq!(info.amount, 2_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_update_amount_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );
    client.update_amount(&0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_update_amount_after_funding_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, ..) = setup_funded_escrow(&env);

    // Try to update amount after funding
    client.update_amount(&2_000);
}

#[test]
fn test_initialize_with_arbiters() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter1 = Address::generate(&env);
    let arbiter2 = Address::generate(&env);
    let arbiter3 = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    let arbiters = soroban_sdk::vec![&env, arbiter1.clone(), arbiter2.clone(), arbiter3.clone()];
    client.initialize_with_arbiters(
        &admin, &buyer, &seller, &arbiters, &token, &1_000, &deadline, &2, &0, &None, &None,
    );

    let info = client.get_escrow_info();
    assert_eq!(info.amount, 1_000);
}

// ── #716 metadata_hash tests ─────────────────────────────────────────────────

#[test]
fn test_initialize_with_metadata_hash_stores_it() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    let hash = soroban_sdk::BytesN::from_array(&env, &[0xabu8; 32]);

    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &1_000,
        &deadline,
        &0,
        &Some(hash.clone()),
        &None,
    );

    let info = client.get_escrow_info();
    assert_eq!(info.metadata_hash, hash);
}

#[test]
fn test_initialize_without_metadata_hash_is_none() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );

    let info = client.get_escrow_info();
    assert_eq!(
        info.metadata_hash,
        soroban_sdk::BytesN::from_array(&env, &[0u8; 32])
    );
}

// ── #709 dispute timeout tests ────────────────────────────────────────────────

/// Helper: set up a funded escrow with a dispute timeout of `timeout` ledgers.
fn setup_funded_escrow_with_timeout<'a>(
    env: &'a Env,
    timeout: u32,
) -> (EscrowContractClient<'a>, Address, Address) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &amount, &deadline, &timeout, &None, &None,
    );
    client.fund();
    (client, buyer, seller)
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_claim_dispute_timeout_before_timeout_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _) = setup_funded_escrow_with_timeout(&env, 50);
    client.raise_dispute(&buyer);
    // Only 0 ledgers have elapsed; timeout is 50 → DisputeTimeoutNotReached (#10)
    client.claim_dispute_timeout();
}

#[test]
fn test_claim_dispute_timeout_after_timeout_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _) = setup_funded_escrow_with_timeout(&env, 50);
    client.raise_dispute(&buyer);

    // Advance ledger past the timeout
    env.ledger().with_mut(|l| l.sequence_number += 51);
    client.claim_dispute_timeout();

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_claim_dispute_timeout_no_timeout_configured_fails() {
    let env = Env::default();
    env.mock_all_auths();

    // timeout = 0 means no timeout configured → NoDisputeTimeout (#11)
    let (client, buyer, _) = setup_funded_escrow_with_timeout(&env, 0);
    client.raise_dispute(&buyer);
    env.ledger().with_mut(|l| l.sequence_number += 200);
    client.claim_dispute_timeout();
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_claim_dispute_timeout_not_in_disputed_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _buyer, _seller) = setup_funded_escrow_with_timeout(&env, 50);
    // No dispute raised → InvalidState (#2)
    client.claim_dispute_timeout();
}

// ── #713 partial refund tests ─────────────────────────────────────────────────

#[test]
fn test_request_partial_refund_after_partial_release() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, _arbiter, _token, amount) = setup_funded_escrow(&env);
    let partial = amount / 2; // release half to seller

    client.release_partial(&partial);

    // Buyer gets remaining half back via partial refund (no deadline needed)
    client.request_partial_refund();

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
}

#[test]
fn test_request_partial_refund_full_balance() {
    let env = Env::default();
    env.mock_all_auths();

    // No partial release — buyer refunds full remaining balance
    let (client, _, _buyer, _seller, _arbiter, _token, _amount) = setup_funded_escrow(&env);
    client.request_partial_refund();

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_request_partial_refund_wrong_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );

    // Still Created, not Funded → InvalidState (#2)
    client.request_partial_refund();
}

#[test]
fn test_request_refund_after_partial_release_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, _seller, _arbiter, _token, amount) = setup_funded_escrow(&env);
    client.release_partial(&(amount / 4)); // release 250 to seller, 750 remains

    // Advance past deadline
    env.ledger().with_mut(|l| l.sequence_number = 200);
    client.request_refund();

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
    // Remaining amount (750) was refunded; stored balance is cleared after payout.
    assert_eq!(amount - amount / 4, 750);
    let info = client.get_escrow_info();
    assert_eq!(info.amount, 0);
}

// ── Admin-configurable fee tests ───────────────────────────────────────────────

fn last_event_data<T: FromVal<Env, soroban_sdk::Val>>(env: &Env) -> T {
    let all = env.events().all();
    let (_, _, data) = all.last().unwrap();
    T::from_val(env, &data)
}

#[test]
fn test_get_fee_config_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    let (fee_bps, treasury) = client.get_fee_config();
    assert_eq!(fee_bps, 0);
    assert_eq!(treasury, None);
}

#[test]
fn test_set_fee_config_stores_bps_and_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let treasury = Address::generate(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );

    client.set_fee_config(&500u32, &treasury);

    let (fee_bps, stored_treasury) = client.get_fee_config();
    assert_eq!(fee_bps, 500);
    assert_eq!(stored_treasury, Some(treasury.clone()));

    assert_eq!(last_event_data::<(u32, Address)>(&env), (500u32, treasury));
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_set_fee_config_above_10000_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    let treasury = Address::generate(&env);

    // 10_001 bps > 100% — must reject with FeeTooHigh (#13)
    client.set_fee_config(&10_001u32, &treasury);
}

#[test]
fn test_set_fee_config_at_10000_bps_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let treasury = Address::generate(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );

    // 10_000 bps == 100% is the maximum allowed value, not an error.
    client.set_fee_config(&10_000u32, &treasury);
    let (fee_bps, _) = client.get_fee_config();
    assert_eq!(fee_bps, 10_000);
}

#[test]
fn test_release_to_seller_with_no_fee_configured_pays_full_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _buyer, seller, ..) = setup_funded_escrow(&env);
    client.mark_delivered();
    client.approve_delivery();

    // No fee configured: net == full amount, fee == 0.
    assert_eq!(last_event_data::<(i128, i128)>(&env), (1_000i128, 0i128));
    let _ = seller;
}

#[test]
fn test_release_to_seller_deducts_configured_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let treasury = Address::generate(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );
    // 5% fee.
    client.set_fee_config(&500u32, &treasury);
    client.fund();
    client.mark_delivered();
    client.approve_delivery();

    // 1_000 * 500 / 10_000 = 50 fee, 950 net to the seller.
    assert_eq!(last_event_data::<(i128, i128)>(&env), (950i128, 50i128));
}

#[test]
fn test_release_partial_deducts_configured_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let treasury = Address::generate(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None,
    );
    // 10% fee.
    client.set_fee_config(&1_000u32, &treasury);
    client.fund();

    client.release_partial(&200i128);

    // 200 * 1_000 / 10_000 = 20 fee, 180 net to the seller.
    assert_eq!(last_event_data::<(i128, i128)>(&env), (180i128, 20i128));

    // The full gross 200 (not just the net) is deducted from the remaining
    // escrowed amount, so the fee is paid out of the release, not on top of it.
    let info = client.get_escrow_info();
    assert_eq!(info.amount, 800);
}

// ── Milestone dispute remainder (issue #1189) ───────────────────────────────────

fn setup_funded_milestone_escrow<'a>(
    env: &'a Env,
) -> (
    EscrowContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    i128,
    i128,
) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let deadline = env.ledger().sequence() + 100;
    let m1 = 400i128;
    let m2 = 600i128;

    let mut milestones = Vec::new(env);
    milestones.push_back(Milestone {
        description: String::from_str(env, "phase-1"),
        amount: m1,
        released: false,
    });
    milestones.push_back(Milestone {
        description: String::from_str(env, "phase-2"),
        amount: m2,
        released: false,
    });

    let (client, _) = create_escrow_contract(env);
    client.initialize_with_milestones(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &milestones,
        &deadline,
        &0,
        &None,
        &None,
    );
    client.fund();
    (client, buyer, seller, arbiter, token, m1, m2)
}

#[test]
fn test_resolve_dispute_to_seller_after_partial_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, _m1, m2) = setup_funded_milestone_escrow(&env);

    // Release milestone 0; only m2 remains disputable.
    client.release_milestone(&buyer, &0u32);
    assert_eq!(client.get_escrow_info().amount, m2);

    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
    // Stored amount cleared after paying only the unreleased remainder.
    assert_eq!(client.get_escrow_info().amount, 0);

    let milestones = client.get_milestones();
    assert!(milestones.get(0).unwrap().released);
    assert!(milestones.get(1).unwrap().released);
}

#[test]
fn test_resolve_dispute_to_buyer_after_partial_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, _m1, m2) = setup_funded_milestone_escrow(&env);

    client.release_milestone(&buyer, &0u32);
    assert_eq!(client.get_escrow_info().amount, m2);

    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
    // Refund settles only the unreleased remainder — never re-disburse m1.
    assert_eq!(client.get_escrow_info().amount, 0);
    let milestones = client.get_milestones();
    assert!(milestones.get(0).unwrap().released);
    assert!(milestones.get(1).unwrap().released);
}

#[test]
fn test_dispute_resolution_ignores_stale_total_amount_for_milestones() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, m1, m2) = setup_funded_milestone_escrow(&env);
    let contract_id = client.address.clone();
    client.release_milestone(&buyer, &0u32);

    // Simulate a stale Amount still holding the original total (m1+m2).
    // Dispute resolution must still use only unreleased milestone funds.
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::Amount, &(m1 + m2));
    });
    assert_eq!(client.get_escrow_info().amount, m1 + m2);

    // Before resolve, unreleased remainder is m2 even though Amount is stale.
    let milestones_before = client.get_milestones();
    assert!(milestones_before.get(0).unwrap().released);
    assert!(!milestones_before.get(1).unwrap().released);
    assert_eq!(milestones_before.get(1).unwrap().amount, m2);

    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
    // Amount synced down from stale total and cleared after refunding m2 only.
    assert_eq!(client.get_escrow_info().amount, 0);
}

// ── #1288 arbiter fee split on dispute resolution ─────────────────────────────

/// Helper: funded escrow with an arbiter fee configured (in bps).
fn setup_funded_escrow_with_arbiter_fee<'a>(
    env: &'a Env,
    fee_bps: u32,
) -> (EscrowContractClient<'a>, Address, Address, Address, Address, i128) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(env);
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &amount,
        &deadline,
        &0,
        &None,
        &Some(fee_bps),
    );
    client.fund();
    (client, buyer, seller, arbiter, token, amount)
}

#[test]
fn test_resolve_dispute_deducts_arbiter_fee_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    // 5% arbiter fee.
    let (client, buyer, _seller, arbiter, _token, amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 500);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    assert_eq!(client.get_state(), Some(EscrowState::Completed));

    // 1_000 * 500 / 10_000 = 50 fee; remaining 950 goes to the seller.
    let fee = amount * 500 / 10_000;
    let seller_share = amount - fee;
    assert_eq!(fee + seller_share, amount, "funds must be conserved");
    assert_eq!(last_event_data::<(i128, i128)>(&env), (seller_share, fee));
}

#[test]
fn test_resolve_dispute_deducts_arbiter_fee_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 1_000);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));

    // 10% fee; buyer receives the remaining 900.
    let fee = amount * 1_000 / 10_000;
    let buyer_share = amount - fee;
    assert_eq!(fee + buyer_share, amount, "funds must be conserved");
    assert_eq!(last_event_data::<(i128, i128)>(&env), (buyer_share, fee));
}

#[test]
fn test_resolve_dispute_emits_arbiter_fee_paid_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 500);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    let fee = amount * 500 / 10_000;
    let all = env.events().all();
    let found = all.iter().any(|(_, topics, data)| {
        topics == (Symbol::new(&env, "arbiter_fee_paid"), arbiter.clone()).into_val(&env)
            && data == fee.into_val(&env)
    });
    assert!(found, "ArbiterFeePaid event not emitted");
}

#[test]
fn test_resolve_dispute_zero_arbiter_fee_pays_full_share() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 0);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    assert_eq!(last_event_data::<(i128, i128)>(&env), (amount, 0i128));
}

#[test]
fn test_initialize_arbiter_fee_at_cap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    // 1_000 bps == 10% is the hardcoded maximum arbiter fee (#1439).
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &1_000,
        &deadline,
        &0,
        &None,
        &Some(1_000u32),
    );
    assert_eq!(client.get_escrow_info().amount, 1_000);
}

#[test]
#[should_panic]
fn test_initialize_arbiter_fee_above_cap_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);

    // 1_001 bps > 10% cap — must be rejected.
    client.initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &1_000,
        &deadline,
        &0,
        &None,
        &Some(1_001u32),
    );
}

// ── #1289 mutual cancellation ─────────────────────────────────────────────────

#[test]
fn test_mutual_cancel_refunds_depositor_and_cancels() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, seller, ..) = setup_funded_escrow(&env);
    client.mutual_cancel(&buyer, &seller);

    assert_eq!(client.get_state(), Some(EscrowState::Cancelled));
    // Unreleased funds are cleared after refunding the depositor.
    assert_eq!(client.get_escrow_info().amount, 0);
}

#[test]
fn test_mutual_cancel_after_partial_release_refunds_remainder() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, seller, _arbiter, _token, amount) = setup_funded_escrow(&env);
    client.release_partial(&(amount / 4));

    client.mutual_cancel(&buyer, &seller);

    assert_eq!(client.get_state(), Some(EscrowState::Cancelled));
    assert_eq!(client.get_escrow_info().amount, 0);
}

#[test]
#[should_panic]
fn test_mutual_cancel_missing_seller_signature_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_address, buyer, seller, ..) = setup_funded_escrow(&env);

    // Only the buyer authorizes — seller.require_auth() must fail.
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    env.mock_auths(&[MockAuth {
        address: &buyer,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "mutual_cancel",
            args: soroban_sdk::vec![&env, buyer.clone().into_val(&env), seller.clone().into_val(&env)]
                .into(),
            sub_invokes: &[],
        },
    }]);
    client.mutual_cancel(&buyer, &seller);
}

#[test]
#[should_panic]
fn test_mutual_cancel_missing_buyer_signature_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_address, buyer, seller, ..) = setup_funded_escrow(&env);

    // Only the seller authorizes — buyer.require_auth() must fail.
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    env.mock_auths(&[MockAuth {
        address: &seller,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "mutual_cancel",
            args: soroban_sdk::vec![&env, buyer.clone().into_val(&env), seller.clone().into_val(&env)]
                .into(),
            sub_invokes: &[],
        },
    }]);
    client.mutual_cancel(&buyer, &seller);
}

// ── #1290 partial milestone approval ──────────────────────────────────────────

#[test]
fn test_approve_milestone_partial_releases_part_and_refunds_rest() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, _arbiter, _token, m1, m2) =
        setup_funded_milestone_escrow(&env);

    // Release 250 of milestone 0's 400 allocation.
    client.approve_milestone_partial(&buyer, &0u32, &250i128);

    let milestones = client.get_milestones();
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    // 150 refunded to buyer, m2 (600) still escrowed.
    assert_eq!(client.get_escrow_info().amount, m2 + (m1 - 250));
}

#[test]
fn test_approve_milestone_partial_full_amount_releases_all() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, _arbiter, _token, m1, m2) =
        setup_funded_milestone_escrow(&env);

    client.approve_milestone_partial(&buyer, &0u32, &m1);

    let milestones = client.get_milestones();
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    // No refund for milestone 0; only m2 remains.
    assert_eq!(client.get_escrow_info().amount, m2);
}

#[test]
#[should_panic]
fn test_approve_milestone_partial_exceeds_allocation_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, _arbiter, _token, m1, _m2) =
        setup_funded_milestone_escrow(&env);

    // m1 + 1 exceeds the milestone's allocated amount — must be rejected.
    client.approve_milestone_partial(&buyer, &0u32, &(m1 + 1));
}

#[test]
#[should_panic]
fn test_approve_milestone_partial_requires_buyer_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _buyer, seller, _arbiter, _token, _m1, _m2) =
        setup_funded_milestone_escrow(&env);

    // Clear auths so the buyer's authorization is absent.
    env.set_auths(&[]);
    client.approve_milestone_partial(&seller, &0u32, &100i128);
}

// ── #1291 multi-arbiter M-of-N dispute resolution ─────────────────────────────

/// Helper: funded escrow with a 2-of-3 arbiter committee.
fn setup_funded_multi_arbiter_escrow<'a>(
    env: &'a Env,
) -> (
    EscrowContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter1 = Address::generate(env);
    let arbiter2 = Address::generate(env);
    let arbiter3 = Address::generate(env);
    let token = create_mock_token(env);
    let deadline = env.ledger().sequence() + 100;

    let arbiters = soroban_sdk::vec![
        env,
        arbiter1.clone(),
        arbiter2.clone(),
        arbiter3.clone()
    ];
    let (client, _) = create_escrow_contract(env);
    client.initialize_with_arbiters(
        &admin, &buyer, &seller, &arbiters, &token, &1_000, &deadline, &2, &0, &None, &None,
    );
    client.fund();
    (client, buyer, arbiter1, arbiter2, arbiter3, token)
}

#[test]
fn test_multi_arbiter_threshold_met_resolves_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, arbiter1, arbiter2, _arbiter3, _token) =
        setup_funded_multi_arbiter_escrow(&env);
    client.raise_dispute(&buyer);

    // First approval alone must not release funds.
    client.approve_dispute(&arbiter1, &true);
    assert_eq!(client.get_state(), Some(EscrowState::Disputed));

    // Second distinct approval reaches the 2-of-3 threshold.
    client.approve_dispute(&arbiter2, &true);
    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_multi_arbiter_threshold_not_met_keeps_funds_escrowed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, arbiter1, _arbiter2, _arbiter3, _token) =
        setup_funded_multi_arbiter_escrow(&env);
    client.raise_dispute(&buyer);

    client.approve_dispute(&arbiter1, &false);
    // Only 1 of 2 required approvals — still disputed, funds untouched.
    assert_eq!(client.get_state(), Some(EscrowState::Disputed));
    assert_eq!(client.get_escrow_info().amount, 1_000);
}

#[test]
#[should_panic]
fn test_multi_arbiter_duplicate_approval_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, arbiter1, _arbiter2, _arbiter3, _token) =
        setup_funded_multi_arbiter_escrow(&env);
    client.raise_dispute(&buyer);

    client.approve_dispute(&arbiter1, &true);
    // Same arbiter approving twice must not count toward the threshold.
    client.approve_dispute(&arbiter1, &true);
}

#[test]
#[should_panic]
fn test_multi_arbiter_non_committee_approval_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _arbiter1, _arbiter2, _arbiter3, _token) =
        setup_funded_multi_arbiter_escrow(&env);
    client.raise_dispute(&buyer);

    let outsider = Address::generate(&env);
    client.approve_dispute(&outsider, &true);
}

// ---------------------------------------------------------------------------
// Staged percentage milestones with per-milestone dispute windows (#1437)
// ---------------------------------------------------------------------------

mod staged_milestones {
    use super::*;
    use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

    struct Staged<'a> {
        client: EscrowContractClient<'a>,
        buyer: Address,
        seller: Address,
        arbiter: Address,
        token: TokenClient<'a>,
    }

    fn stages(env: &Env, spec: &[(u32, u32)]) -> Vec<MilestoneStage> {
        let mut v = Vec::new(env);
        for (percentage_bps, dispute_window_ledgers) in spec {
            v.push_back(MilestoneStage {
                percentage_bps: *percentage_bps,
                dispute_window_ledgers: *dispute_window_ledgers,
            });
        }
        v
    }

    /// Register a staged escrow backed by a real Stellar Asset Contract so
    /// balances can be asserted, then fund it with `total`.
    fn setup<'a>(env: &'a Env, total: i128, spec: &[(u32, u32)]) -> Staged<'a> {
        env.ledger().with_mut(|l| l.sequence_number = 100);
        let admin = Address::generate(env);
        let buyer = Address::generate(env);
        let seller = Address::generate(env);
        let arbiter = Address::generate(env);

        let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
        let token_addr = sac.address();
        StellarAssetClient::new(env, &token_addr).mint(&buyer, &total);

        let (client, _) = create_escrow_contract(env);
        client.initialize_with_milestone_schedule(
            &admin,
            &buyer,
            &seller,
            &arbiter,
            &token_addr,
            &total,
            &stages(env, spec),
            &(env.ledger().sequence() + 10_000),
            &None,
        );
        client.fund();

        Staged {
            client,
            buyer,
            seller,
            arbiter,
            token: TokenClient::new(env, &token_addr),
        }
    }

    fn set_ledger(env: &Env, seq: u32) {
        env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    #[test]
    fn schedule_splits_total_by_percentage() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(3_000, 10), (3_000, 10), (4_000, 10)]);

        let schedule = s.client.get_staged_milestones();
        assert_eq!(schedule.len(), 3);
        assert_eq!(schedule.get(0).unwrap().amount, 300);
        assert_eq!(schedule.get(1).unwrap().amount, 300);
        assert_eq!(schedule.get(2).unwrap().amount, 400);
        for m in schedule.iter() {
            assert_eq!(m.status, MilestoneStatus::Pending);
        }
        assert_eq!(s.token.balance(&s.client.address), 1_000);
    }

    #[test]
    fn last_stage_absorbs_rounding_remainder() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(3_333, 10), (3_333, 10), (3_334, 10)]);

        let schedule = s.client.get_staged_milestones();
        let total: i128 = schedule.iter().map(|m| m.amount).sum();
        assert_eq!(total, 1_000);
        assert_eq!(schedule.get(0).unwrap().amount, 333);
        assert_eq!(schedule.get(1).unwrap().amount, 333);
        assert_eq!(schedule.get(2).unwrap().amount, 334);
    }

    #[test]
    fn multi_stage_disbursement_releases_fractional_amounts() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(3_000, 20), (3_000, 20), (4_000, 20)]);

        // Stage 0: seller submits, buyer approves immediately.
        s.client.submit_milestone(&0);
        s.client.approve_milestone(&0);
        assert_eq!(s.token.balance(&s.seller), 300);
        assert_eq!(s.client.get_escrow_info().amount, 700);
        assert_eq!(s.client.get_state(), Some(EscrowState::Funded));

        // Stage 1: seller submits, window lapses, seller claims automatically.
        s.client.submit_milestone(&1);
        set_ledger(&env, env.ledger().sequence() + 21);
        s.client.claim_milestone(&1);
        assert_eq!(s.token.balance(&s.seller), 600);
        assert_eq!(s.client.get_escrow_info().amount, 400);

        // Stage 2: buyer disputes in-window, arbiter releases to seller.
        s.client.submit_milestone(&2);
        s.client.dispute_milestone(&2);
        assert_eq!(
            s.client.get_staged_milestones().get(2).unwrap().status,
            MilestoneStatus::Disputed
        );
        s.client.resolve_milestone_dispute(&s.arbiter, &2, &true);

        assert_eq!(s.token.balance(&s.seller), 1_000);
        assert_eq!(s.token.balance(&s.client.address), 0);
        assert_eq!(s.client.get_escrow_info().amount, 0);
        assert_eq!(s.client.get_state(), Some(EscrowState::Completed));
        for m in s.client.get_staged_milestones().iter() {
            assert_eq!(m.status, MilestoneStatus::Released);
        }
    }

    #[test]
    fn disputed_stage_can_be_refunded_to_buyer() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(5_000, 10), (5_000, 10)]);

        s.client.submit_milestone(&0);
        // Dispute on the last in-window ledger is still accepted.
        set_ledger(&env, env.ledger().sequence() + 10);
        s.client.dispute_milestone(&0);
        s.client.resolve_milestone_dispute(&s.arbiter, &0, &false);

        assert_eq!(s.token.balance(&s.buyer), 500);
        assert_eq!(s.token.balance(&s.seller), 0);
        assert_eq!(s.client.get_escrow_info().amount, 500);
        assert_eq!(
            s.client.get_staged_milestones().get(0).unwrap().status,
            MilestoneStatus::Refunded
        );
        // Escrow stays open for the remaining stage.
        assert_eq!(s.client.get_state(), Some(EscrowState::Funded));

        s.client.approve_milestone(&1);
        assert_eq!(s.token.balance(&s.seller), 500);
        assert_eq!(s.client.get_state(), Some(EscrowState::Completed));
    }

    #[test]
    fn all_stages_refunded_marks_escrow_refunded() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(10_000, 5)]);

        s.client.submit_milestone(&0);
        s.client.dispute_milestone(&0);
        s.client.resolve_milestone_dispute(&s.arbiter, &0, &false);

        assert_eq!(s.token.balance(&s.buyer), 1_000);
        assert_eq!(s.client.get_state(), Some(EscrowState::Refunded));
    }

    #[test]
    fn dispute_after_window_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(5_000, 10), (5_000, 10)]);

        s.client.submit_milestone(&0);
        set_ledger(&env, env.ledger().sequence() + 11);
        assert!(matches!(
            s.client.try_dispute_milestone(&0),
            Err(Ok(EscrowError::DisputeWindowClosed))
        ));
        // Window lapsed: seller can claim.
        s.client.claim_milestone(&0);
        assert_eq!(s.token.balance(&s.seller), 500);
    }

    #[test]
    fn claim_before_window_elapses_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(5_000, 10), (5_000, 10)]);

        s.client.submit_milestone(&0);
        set_ledger(&env, env.ledger().sequence() + 10);
        assert!(matches!(
            s.client.try_claim_milestone(&0),
            Err(Ok(EscrowError::DisputeWindowOpen))
        ));
        assert_eq!(s.token.balance(&s.seller), 0);
    }

    #[test]
    fn dispute_windows_are_per_milestone() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(5_000, 10), (5_000, 100)]);

        s.client.submit_milestone(&0);
        s.client.submit_milestone(&1);
        set_ledger(&env, env.ledger().sequence() + 50);

        // Stage 0's short window has closed; stage 1's long window is open.
        assert!(matches!(
            s.client.try_dispute_milestone(&0),
            Err(Ok(EscrowError::DisputeWindowClosed))
        ));
        s.client.dispute_milestone(&1);
        assert!(matches!(
            s.client.try_claim_milestone(&1),
            Err(Ok(EscrowError::InvalidState))
        ));
    }

    #[test]
    fn disputed_stage_cannot_be_approved_or_claimed() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(10_000, 5)]);

        s.client.submit_milestone(&0);
        s.client.dispute_milestone(&0);
        set_ledger(&env, env.ledger().sequence() + 6);
        assert!(s.client.try_approve_milestone(&0).is_err());
        assert!(s.client.try_claim_milestone(&0).is_err());
        assert_eq!(s.token.balance(&s.client.address), 1_000);
    }

    #[test]
    fn only_arbiter_can_resolve_milestone_dispute() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(10_000, 5)]);

        s.client.submit_milestone(&0);
        s.client.dispute_milestone(&0);
        let outsider = Address::generate(&env);
        assert!(matches!(
            s.client.try_resolve_milestone_dispute(&outsider, &0, &true),
            Err(Ok(EscrowError::NotAuthorized))
        ));
        assert!(matches!(
            s.client.try_resolve_milestone_dispute(&s.buyer, &0, &false),
            Err(Ok(EscrowError::NotAuthorized))
        ));
    }

    #[test]
    fn settled_stage_cannot_be_released_twice() {
        let env = Env::default();
        env.mock_all_auths();
        let s = setup(&env, 1_000, &[(5_000, 5), (5_000, 5)]);

        s.client.approve_milestone(&0);
        assert!(s.client.try_approve_milestone(&0).is_err());
        assert!(s.client.try_submit_milestone(&0).is_err());
        assert!(matches!(
            s.client.try_approve_milestone(&9),
            Err(Ok(EscrowError::MilestoneNotFound))
        ));
        assert_eq!(s.token.balance(&s.seller), 500);
    }

    #[test]
    fn invalid_schedules_are_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let token = create_mock_token(&env);
        let deadline = env.ledger().sequence() + 1_000;

        let cases: [&[(u32, u32)]; 4] = [
            &[],                       // empty
            &[(5_000, 10), (4_000, 10)], // sums to 9 000
            &[(10_000, 10), (0, 10)],  // zero share
            &[(10_000, 0)],            // zero dispute window
        ];
        for spec in cases {
            let (client, _) = create_escrow_contract(&env);
            let res = client.try_initialize_with_milestone_schedule(
                &admin,
                &buyer,
                &seller,
                &arbiter,
                &token,
                &1_000,
                &stages(&env, spec),
                &deadline,
                &None,
            );
            assert!(matches!(res, Err(Ok(EscrowError::InvalidMilestoneSchedule))));
        }

        // A stage that would round down to zero funds is rejected too.
        let (client, _) = create_escrow_contract(&env);
        let res = client.try_initialize_with_milestone_schedule(
            &admin,
            &buyer,
            &seller,
            &arbiter,
            &token,
            &3,
            &stages(&env, &[(1, 10), (9_999, 10)]),
            &deadline,
            &None,
        );
        assert!(matches!(res, Err(Ok(EscrowError::InvalidMilestoneSchedule))));
    }
// ── #1439 arbiter fee ceiling enforcement ─────────────────────────────────────

/// Initialize (but do not fund) a single-arbiter escrow with the given fee.
fn try_init_with_arbiter_fee(env: &Env, fee_bps: u32) -> Option<EscrowError> {
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(env);
    match client.try_initialize(
        &admin,
        &buyer,
        &seller,
        &arbiter,
        &token,
        &1_000,
        &deadline,
        &0,
        &None,
        &None,
        &Some(fee_bps),
    ) {
        Ok(_) => None,
        Err(Ok(e)) => Some(e),
        Err(Err(e)) => panic!("unexpected invoke error: {:?}", e),
    }
}

#[test]
fn test_arbiter_fee_ceiling_boundary() {
    let env = Env::default();
    env.mock_all_auths();

    assert!(try_init_with_arbiter_fee(&env, 0).is_none());
    assert!(try_init_with_arbiter_fee(&env, 999).is_none());
    assert!(try_init_with_arbiter_fee(&env, 1_000).is_none());
    assert!(matches!(
        try_init_with_arbiter_fee(&env, 1_001),
        Some(EscrowError::ArbiterFeeTooHigh)
    ));
    assert!(matches!(
        try_init_with_arbiter_fee(&env, 2_000),
        Some(EscrowError::ArbiterFeeTooHigh)
    ));
    assert!(matches!(
        try_init_with_arbiter_fee(&env, u32::MAX),
        Some(EscrowError::ArbiterFeeTooHigh)
    ));
}

#[test]
fn test_resolve_dispute_at_fee_ceiling_pays_at_most_ten_percent() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 1_000);
    client.raise_dispute(&buyer);
    client.resolve_dispute(&arbiter, &true);

    let fee = amount * 1_000 / 10_000;
    assert_eq!(last_event_data::<(i128, i128)>(&env), (amount - fee, fee));
}

#[test]
fn test_resolve_dispute_rejects_fee_above_ceiling() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, _seller, arbiter, _token, _amount) =
        setup_funded_escrow_with_arbiter_fee(&env, 500);
    client.raise_dispute(&buyer);

    // Simulate a corrupted / out-of-range fee reaching storage.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ArbiterFeeBps, &5_000u32);
    });

    assert!(matches!(
        client.try_resolve_dispute(&arbiter, &true),
        Err(Ok(EscrowError::ArbiterFeeTooHigh))
    ));
    // Resolution was rejected: escrow remains disputed, no funds moved.
    assert_eq!(client.get_state(), Some(EscrowState::Disputed));
}

// ── #1438 buyer lockup enforcement ────────────────────────────────────────────

/// Returns (client, buyer, seller, lockup_deadline) for a funded escrow whose
/// buyer configured a lockup `lockup_secs` seconds in the future.
fn setup_funded_escrow_with_lockup<'a>(
    env: &'a Env,
    lockup_secs: u64,
) -> (EscrowContractClient<'a>, Address, Address, u64) {
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let token = create_mock_token(env);
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None, &None,
    );
    let lockup_deadline = env.ledger().timestamp() + lockup_secs;
    client.set_lockup_deadline(&lockup_deadline);
    client.fund();
    (client, buyer, seller, lockup_deadline)
}

#[test]
fn test_lockup_deadline_is_stored() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, lockup_deadline) = setup_funded_escrow_with_lockup(&env, 3_600);
    assert_eq!(client.get_lockup_deadline(), Some(lockup_deadline));
}

#[test]
fn test_approve_delivery_rejected_before_lockup() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow_with_lockup(&env, 3_600);
    client.mark_delivered();

    assert!(matches!(
        client.try_approve_delivery(),
        Err(Ok(EscrowError::LockupPeriodActive))
    ));
    assert_eq!(client.get_state(), Some(EscrowState::Delivered));
}

#[test]
fn test_release_partial_rejected_before_lockup() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow_with_lockup(&env, 3_600);
    assert!(matches!(
        client.try_release_partial(&100),
        Err(Ok(EscrowError::LockupPeriodActive))
    ));
    assert_eq!(client.get_escrow_info().amount, 1_000);
}

#[test]
fn test_release_rejected_one_second_before_lockup() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, lockup_deadline) = setup_funded_escrow_with_lockup(&env, 3_600);
    client.mark_delivered();
    env.ledger().with_mut(|l| l.timestamp = lockup_deadline - 1);

    assert!(matches!(
        client.try_approve_delivery(),
        Err(Ok(EscrowError::LockupPeriodActive))
    ));
}

#[test]
fn test_release_succeeds_once_lockup_elapsed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, lockup_deadline) = setup_funded_escrow_with_lockup(&env, 3_600);
    client.mark_delivered();
    env.ledger().with_mut(|l| l.timestamp = lockup_deadline);

    client.approve_delivery();
    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_dispute_release_to_seller_rejected_before_lockup() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, buyer, ..) = setup_funded_escrow_with_lockup(&env, 3_600);
    let arbiter = client.get_escrow_info().arbiter;
    client.raise_dispute(&buyer);

    assert!(matches!(
        client.try_resolve_dispute(&arbiter, &true),
        Err(Ok(EscrowError::LockupPeriodActive))
    ));
}

#[test]
fn test_set_lockup_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    let future = env.ledger().timestamp() + 3_600;
    assert!(matches!(
        client.try_set_lockup_deadline(&future),
        Err(Ok(EscrowError::InvalidState))
    ));
}

#[test]
fn test_set_lockup_rejects_past_timestamp() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = create_mock_token(&env);
    let deadline = env.ledger().sequence() + 100;
    let (client, _) = create_escrow_contract(&env);
    client.initialize(
        &admin, &buyer, &seller, &arbiter, &token, &1_000, &deadline, &0, &None, &None, &None,
    );

    assert!(matches!(
        client.try_set_lockup_deadline(&1_000),
        Err(Ok(EscrowError::InvalidState))
    ));
    assert_eq!(client.get_lockup_deadline(), None);
}
