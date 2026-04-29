#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env,
};

fn create_escrow_contract(env: &Env) -> (EscrowContractClient<'_>, Address) {
    let contract_address = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &contract_address);
    (client, contract_address)
}

/// Deploy a SAC token, mint `amount` to `buyer`, return the token address.
fn setup_token(env: &Env, buyer: &Address, amount: i128) -> Address {
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = sac.address();
    StellarAssetClient::new(env, &token_address).mint(buyer, &amount);
    token_address
}

/// Sets up a funded escrow with a real SAC token.
/// Returns (client, escrow_address, buyer, seller, arbiter, token_address, amount)
fn setup_funded_escrow(
    env: &Env,
) -> (EscrowContractClient<'_>, Address, Address, Address, Address, Address, i128) {
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let arbiter = Address::generate(env);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let token_address = setup_token(env, &buyer, amount);

    let (client, escrow_address) = create_escrow_contract(env);
    client.initialize(&buyer, &seller, &arbiter, &token_address, &amount, &deadline);
    client.fund();

    (client, escrow_address, buyer, seller, arbiter, token_address, amount)
}

#[test]
fn test_initialize_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = setup_token(&env, &buyer, 1000);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);

    let info = client.get_escrow_info();
    assert_eq!(info.buyer, buyer);
    assert_eq!(info.seller, seller);
    assert_eq!(info.arbiter, arbiter);
    assert_eq!(info.token_contract, token_contract);
    assert_eq!(info.amount, amount);
    assert_eq!(info.deadline, deadline);
    assert_eq!(info.state, EscrowState::Created);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = setup_token(&env, &buyer, 1000);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
}

#[test]
#[should_panic(expected = "Deadline must be at least MIN_DEADLINE_BUFFER ledgers in the future")]
fn test_initialize_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 10);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = setup_token(&env, &buyer, 1000);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() - 1;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
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
fn test_approve_delivery() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    client.mark_delivered();
    client.approve_delivery();

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_deadline_passed() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = setup_token(&env, &buyer, 1000);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 105; // > MIN_DEADLINE_BUFFER

    let (client, _) = create_escrow_contract(&env);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);

    assert!(!client.is_deadline_passed());

    env.ledger().with_mut(|li| li.sequence_number = deadline + 1);

    assert!(client.is_deadline_passed());
}

#[test]
fn test_cancel() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_contract = setup_token(&env, &buyer, 1000);
    let amount = 1000i128;
    let deadline = env.ledger().sequence() + 100;

    let (client, _) = create_escrow_contract(&env);
    client.initialize(&buyer, &seller, &arbiter, &token_contract, &amount, &deadline);
    client.cancel();

    assert_eq!(client.get_state(), Some(EscrowState::Cancelled));
}

#[test]
fn test_release_partial() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, _, _, _, amount) = setup_funded_escrow(&env);
    let partial = 400i128;
    client.release_partial(&partial);

    let info = client.get_escrow_info();
    assert_eq!(info.amount, amount - partial);
    assert_eq!(info.state, EscrowState::Funded);
}

#[test]
fn test_arbiter_resolve_to_seller() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    client.resolve_dispute(&true);

    assert_eq!(client.get_state(), Some(EscrowState::Completed));
}

#[test]
fn test_arbiter_resolve_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, ..) = setup_funded_escrow(&env);
    client.resolve_dispute(&false);

    assert_eq!(client.get_state(), Some(EscrowState::Refunded));
}

// ── Property-based tests ──────────────────────────────────────────────────

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    /// For any valid partial release amount, the remaining escrow amount decreases by exactly that amount.
    #[test]
    fn prop_partial_release_reduces_amount() {
        proptest!(|(total in 2i128..=1_000_000i128, pct in 1u32..=99u32)| {
            let env = Env::default();
            env.mock_all_auths();

            let buyer = Address::generate(&env);
            let seller = Address::generate(&env);
            let arbiter = Address::generate(&env);
            let token_address = setup_token(&env, &buyer, total);
            let deadline = env.ledger().sequence() + 200;

            let (client, _) = create_escrow_contract(&env);
            client.initialize(&buyer, &seller, &arbiter, &token_address, &total, &deadline);
            client.fund();

            let partial = (total * pct as i128) / 100;
            if partial == 0 || partial >= total { return Ok(()); }

            client.release_partial(&partial);

            let info = client.get_escrow_info();
            prop_assert_eq!(info.amount, total - partial);
            prop_assert_eq!(info.state, EscrowState::Funded);
        });
    }

    /// Escrow initialized with any valid positive amount always starts in Created state.
    #[test]
    fn prop_initialize_always_creates_state() {
        proptest!(|(amount in 1i128..=1_000_000i128)| {
            let env = Env::default();
            env.mock_all_auths();

            let buyer = Address::generate(&env);
            let seller = Address::generate(&env);
            let arbiter = Address::generate(&env);
            let token_address = setup_token(&env, &buyer, amount);
            let deadline = env.ledger().sequence() + 200;

            let (client, _) = create_escrow_contract(&env);
            client.initialize(&buyer, &seller, &arbiter, &token_address, &amount, &deadline);

            let info = client.get_escrow_info();
            prop_assert_eq!(info.state, EscrowState::Created);
            prop_assert_eq!(info.amount, amount);
        });
    }
}
