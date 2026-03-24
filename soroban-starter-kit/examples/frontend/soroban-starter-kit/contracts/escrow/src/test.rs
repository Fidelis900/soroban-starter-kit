#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn create_escrow_contract<'a>(env: &Env) -> (EscrowContractClient<'a>, Address) {
    let contract_address = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &contract_address);
    (client, contract_address)
}

#[test]
fn test_initialize_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Initialize escrow
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);

    // Verify initialization
    let (stored_buyer, stored_seller, stored_arbiter, stored_token, stored_amount, stored_deadline, state) = 
        client.get_escrow_info();
    
    assert_eq!(stored_buyer, buyer);
    assert_eq!(stored_seller, seller);
    assert_eq!(stored_arbiter, arbiter);
    assert_eq!(stored_token, token_contract);
    assert_eq!(stored_amount, amount);
    assert_eq!(stored_deadline, deadline);
    assert_eq!(state, EscrowState::Created);
}

#[test]
#[should_panic(expected = "AlreadyInitialized")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Initialize once
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    
    // Try to initialize again - should panic
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
}

#[test]
#[should_panic(expected = "Deadline must be in the future")]
fn test_initialize_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() - 1; // Past deadline

    let (client, _) = create_escrow_contract(&env);

    // Should panic due to past deadline
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
}

#[test]
fn test_mark_delivered() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Initialize, fund, and mark delivered
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.fund();
    client.mark_delivered();

    // Verify state change
    assert_eq!(client.get_state(), EscrowState::Delivered);
}

#[test]
fn test_approve_delivery() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Full happy path: initialize → fund → deliver → approve
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.fund();
    client.mark_delivered();
    client.approve_delivery();

    // Verify completion
    assert_eq!(client.get_state(), EscrowState::Completed);
}

#[test]
fn test_deadline_passed() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 5;

    let (client, _) = create_escrow_contract(&env);

    // Initialize
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    
    // Initially deadline not passed
    assert_eq!(client.is_deadline_passed(), false);
    
    // Jump past deadline
    env.ledger().with_mut(|li| li.sequence_number = deadline + 1);
    
    // Now deadline should be passed
    assert_eq!(client.is_deadline_passed(), true);
}

#[test]
fn test_arbiter_resolve_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Initialize and fund
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.fund();
    
    // Arbiter resolves in favor of seller
    client.resolve_dispute(&true);

    // Verify completion
    assert_eq!(client.get_state(), EscrowState::Completed);
}

#[test]
fn test_arbiter_resolve_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);

    // Initialize and fund
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.fund();
    
    // Arbiter resolves in favor of buyer (refund)
    client.resolve_dispute(&false);

    // Verify refund
    assert_eq!(client.get_state(), EscrowState::Refunded);
}