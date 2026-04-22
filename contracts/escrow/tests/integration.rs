use soroban_escrow_template::{EscrowContract, EscrowContractClient, EscrowState};
use soroban_sdk::{testutils::Address as _, Address, Env, String};
use soroban_token_template::{TokenContract, TokenContractClient};

#[test]
fn token_and_escrow_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_address = env.register_contract(None, TokenContract);
    let token_client = TokenContractClient::new(&env, &token_address);

    token_client.initialize(
        &admin,
        &String::from_str(&env, "Integration Token"),
        &String::from_str(&env, "ITK"),
        &7u32,
    );

    let escrow_address = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_address);

    let amount = 500i128;
    token_client.mint(&buyer, &amount);

    let deadline = env.ledger().sequence() + 100;
    escrow_client.initialize(
        &buyer,
        &seller,
        &arbiter,
        &token_address,
        &amount,
        &deadline,
    );

    escrow_client.fund();
    assert_eq!(token_client.balance(&buyer), 0);
    assert_eq!(token_client.balance(&escrow_address), amount);
    assert_eq!(escrow_client.get_state(), EscrowState::Funded);

    escrow_client.mark_delivered();
    assert_eq!(escrow_client.get_state(), EscrowState::Delivered);

    escrow_client.approve_delivery();
    assert_eq!(escrow_client.get_state(), EscrowState::Completed);
    assert_eq!(token_client.balance(&escrow_address), 0);
    assert_eq!(token_client.balance(&seller), amount);
}
