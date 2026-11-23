//! End-to-end escrow lifecycle with a real token contract deployed in the same Env.

use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger as _},
};

use soroban_escrow_template::{EscrowContract, EscrowContractClient, EscrowState};
use soroban_token_template::{TokenContract, TokenContractClient};

fn deploy_token<'a>(env: &'a Env, admin: &Address) -> (TokenContractClient<'a>, Address) {
    let addr = env.register_contract(None, TokenContract);
    let client = TokenContractClient::new(env, &addr);
    client.initialize(
        admin,
        &String::from_str(env, "Test Token"),
        &String::from_str(env, "TEST"),
        &18u32,
        &None,
    );
    (client, addr)
}

fn deploy_escrow<'a>(env: &'a Env) -> (EscrowContractClient<'a>, Address) {
    let addr = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &addr);
    (client, addr)
}

/// initialize → fund → mark_delivered → approve_delivery with real token balances.
#[test]
fn test_full_escrow_lifecycle_with_real_token() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let amount = 2_500i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);

    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(token.total_supply(), 0);

    token.mint(&buyer, &amount);
    assert_eq!(token.balance(&buyer), amount);
    assert_eq!(token.total_supply(), amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    escrow.fund();
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&escrow_addr), amount);
    assert_eq!(token.balance(&seller), 0);

    escrow.mark_delivered();
    assert_eq!(escrow.get_state(), Some(EscrowState::Delivered));

    escrow.approve_delivery();
    assert_eq!(escrow.get_state(), Some(EscrowState::Completed));
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.balance(&seller), amount);
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.total_supply(), amount);
}

/// fund → mutual_cancel with both signatures refunds the depositor in full.
#[test]
fn test_mutual_cancel_refunds_depositor() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let amount = 1_000i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    escrow.fund();
    assert_eq!(token.balance(&buyer), 0);
    assert_eq!(token.balance(&escrow_addr), amount);

    escrow.mutual_cancel(&buyer, &seller);
    assert_eq!(escrow.get_state(), Some(EscrowState::Cancelled));
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.balance(&buyer), amount);
    assert_eq!(token.balance(&seller), 0);
    assert_eq!(token.total_supply(), amount);
}

/// fund → approve_milestone_partial splits the milestone between seller and buyer.
#[test]
fn test_partial_milestone_settlement_with_real_token() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let escrow_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let amount = 1_000i128;
    let release_amount = 800i128;
    let deadline = env.ledger().sequence() + 200;

    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &amount);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(
        &escrow_admin,
        &buyer,
        &seller,
        &arbiter,
        &token_addr,
        &amount,
        &deadline,
        &0u32,
        &None,
        &None,
    );

    escrow.fund();
    assert_eq!(token.balance(&escrow_addr), amount);

    escrow.approve_milestone_partial(&0u32, &release_amount);
    assert_eq!(token.balance(&seller), release_amount);
    assert_eq!(token.balance(&buyer), amount - release_amount);
    assert_eq!(token.balance(&escrow_addr), 0);
    assert_eq!(token.total_supply(), amount);
}
