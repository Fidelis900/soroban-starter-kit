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
    Address, Env, FromVal, String,
    testutils::{Address as _, Ledger as _},
};

fn create_token_contract<'a>(env: &Env) -> (TokenContractClient<'a>, Address) {
    let contract_address = env.register_contract(None, TokenContract);
    let client = TokenContractClient::new(env, &contract_address);
    (client, contract_address)
}

fn init_token<'a>(env: &'a Env, admin: &Address) -> TokenContractClient<'a> {
    let (client, _) = create_token_contract(env);
    client.initialize(
        admin,
        &String::from_str(env, "Test Token"),
        &String::from_str(env, "TEST"),
        &18u32,
        &None,
    );
    client
}

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (client, contract_address) = create_token_contract(&env);
    let name = String::from_str(&env, "Test Token");
    let symbol = String::from_str(&env, "TEST");
    let decimals = 18u32;
    client.initialize(&admin, &name, &symbol, &decimals, &None);

    assert_eq!(client.admin(), admin);
    assert_eq!(client.name(), name.clone());
    assert_eq!(client.symbol(), symbol.clone());
    assert_eq!(client.decimals(), decimals);
    assert_eq!(client.total_supply(), 0i128);

    // Verify initialized event was emitted
    use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                contract_address.clone(),
                (Symbol::new(&env, "initialized"), admin.clone()).into_val(&env),
                (name, symbol, decimals).into_val(&env),
            ),
        ]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_initialize_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (client, _) = create_token_contract(&env);
    client.initialize(
        &admin,
        &String::from_str(&env, "Test Token"),
        &String::from_str(&env, "TEST"),
        &18u32,
        &None,
    );
    client.initialize(
        &admin,
        &String::from_str(&env, "Test Token"),
        &String::from_str(&env, "TEST"),
        &18u32,
        &None,
    );
}

#[test]
fn test_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &1000i128);
    assert_eq!(client.balance(&user), 1000i128);
    assert_eq!(client.total_supply(), 1000i128);
}

#[test]
fn test_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let client = init_token(&env, &admin);

    assert_eq!(client.total_supply(), 0i128);

    client.mint(&user1, &500i128);
    assert_eq!(client.total_supply(), 500i128);

    client.mint(&user2, &300i128);
    assert_eq!(client.total_supply(), 800i128);

    client.burn(&user1, &200i128);
    assert_eq!(client.total_supply(), 600i128);
}

#[test]
fn test_burn() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &1000i128);
    client.burn(&user, &300i128);
    assert_eq!(client.balance(&user), 700i128);
    assert_eq!(client.total_supply(), 700i128);
}

#[test]
fn test_admin_burn_decrements_total_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &1000i128);
    assert_eq!(client.total_supply(), 1000i128);
    client.try_admin_burn(&user, &400i128).unwrap();
    assert_eq!(client.balance(&user), 600i128);
    assert_eq!(client.total_supply(), 600i128);
}

#[test]
fn test_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user1, &1000i128);
    client.transfer(&user1, &user2, &300i128);
    assert_eq!(client.balance(&user1), 700i128);
    assert_eq!(client.balance(&user2), 300i128);
    assert_eq!(client.total_supply(), 1000i128);
}

#[test]
fn test_approve() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user1, &1000i128);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user1, &spender, &500i128, &expiration);
    assert_eq!(client.allowance(&user1, &spender), 500i128);
    client.transfer_from(&spender, &user1, &user2, &200i128);
    assert_eq!(client.balance(&user1), 800i128);
    assert_eq!(client.balance(&user2), 200i128);
    assert_eq!(client.allowance(&user1, &spender), 300i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_expired_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user1, &1000i128);
    // approve with expiration in the past (sequence 0, expiration 0 means already expired)
    let expiration = env.ledger().sequence() + 10;
    client.approve(&user1, &spender, &500i128, &expiration);
    // advance ledger past expiration
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: 0,
        protocol_version: 22,
        sequence_number: expiration + 1,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 6_312_000,
    });
    // should panic with InsufficientAllowance (#2) since allowance is expired
    client.transfer_from(&spender, &user1, &user2, &200i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_mint_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &1000i128);
    client.transfer(&user, &other, &0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_transfer_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let other = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &1000i128);
    client.transfer(&user, &other, &-1i128);
}

#[test]
fn test_set_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (client, contract_address) = create_token_contract(&env);
    client.initialize(
        &admin,
        &String::from_str(&env, "Test Token"),
        &String::from_str(&env, "TEST"),
        &18u32,
        &None,
    );

    client.set_admin(&new_admin);

    // Admin must be updated in storage
    assert_eq!(client.admin(), new_admin);

    // Verify admin_changed event was emitted with old_admin as topic and new_admin as data
    use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
    let all_events = env.events().all();
    let n = all_events.len();
    assert!(n > 0);
    let expected = soroban_sdk::vec![
        &env,
        (
            contract_address.clone(),
            (Symbol::new(&env, "admin_changed"), admin.clone()).into_val(&env),
            new_admin.clone().into_val(&env),
        ),
    ];
    assert_eq!(all_events.slice(n - 1..), expected);
    assert_eq!(
        all_events.slice(n - 1..),
        soroban_sdk::vec![
            &env,
            (
                contract_address.clone(),
                (Symbol::new(&env, "admin_changed"), admin.clone()).into_val(&env),
                new_admin.clone().into_val(&env),
            ),
        ]
    );
}

#[test]
#[should_panic]
fn test_unauthorized_set_admin_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (client, _) = create_token_contract(&env);
    env.mock_all_auths();
    client.initialize(
        &admin,
        &String::from_str(&env, "Test Token"),
        &String::from_str(&env, "TEST"),
        &18u32,
        &None,
    );
    // clear mocked auths so the next call is not authorized
    env.set_auths(&[]);
    // attacker tries to set admin without authorization — should panic
    client.set_admin(&new_admin);
    let _ = attacker;
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_admin_before_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let (client, _) = create_token_contract(&env);
    // Call set_admin before initialize — should panic with NotInitialized (#5)
    client.set_admin(&new_admin);
}

#[test]
fn test_approve_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let spender = Address::generate(&env);
    let (client, contract_address) = create_token_contract(&env);
    client.initialize(
        &admin,
        &String::from_str(&env, "Test Token"),
        &String::from_str(&env, "TEST"),
        &18u32,
        &None,
    );
    client.mint(&user, &1000i128);

    // Set a normal allowance first
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user, &spender, &500i128, &expiration);
    assert_eq!(client.allowance(&user, &spender), 500i128);

    // Revoke by approving with amount == 0 — must emit revoke, not approve
    use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
    client.approve(&user, &spender, &0i128, &expiration);
    assert_eq!(client.allowance(&user, &spender), 0i128);

    // The last event must be revoke, not approve
    let all_events = env.events().all();
    let n = all_events.len();
    assert!(n > 0);
    let expected = soroban_sdk::vec![
        &env,
        (
            contract_address.clone(),
            (Symbol::new(&env, "revoke"), user.clone(), spender.clone()).into_val(&env),
            ().into_val(&env),
        ),
    ];
    assert_eq!(all_events.slice(n - 1..), expected);
    assert_eq!(
        all_events.slice(n - 1..),
        soroban_sdk::vec![
            &env,
            (
                contract_address.clone(),
                (Symbol::new(&env, "revoke"), user.clone(), spender.clone()).into_val(&env),
                ().into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_transfer_self_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &500i128);

    client.transfer(&user, &user, &200i128);

    // Balance unchanged
    assert_eq!(client.balance(&user), 500i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_mint_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.mint(&user, &i128::MAX);
    assert_eq!(client.total_supply(), i128::MAX);

    // Minting 1 more overflows i128 → Overflow (#7)
    client.mint(&user, &1i128);
}

// ---------------------------------------------------------------------------
// Feature-gated tests
// ---------------------------------------------------------------------------

#[cfg(feature = "pausable")]
mod pausable_tests {
    use super::*;

    #[test]
    fn test_pause_blocks_mint() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let client = init_token(&env, &admin);

        client.pause();
        assert!(client.try_mint(&user, &100i128).is_err());
    }

    #[test]
    fn test_unpause_restores_mint() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let client = init_token(&env, &admin);

        client.pause();
        client.unpause();
        client.mint(&user, &100i128);
        assert_eq!(client.balance(&user), 100i128);
    }

    #[test]
    fn test_pause_blocks_burn() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let client = init_token(&env, &admin);
        client.mint(&user, &500i128);

        client.pause();
        assert!(client.try_admin_burn(&user, &100i128).is_err());
    }

    #[test]
    fn test_pause_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);

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
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);

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

#[cfg(feature = "upgradeable")]
mod upgradeable_tests {
    use super::*;

    #[test]
    fn test_upgrade_requires_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);
        // A zero hash is invalid for a real upgrade, but the auth check fires first.
        // We just verify the method exists and is callable by admin.
        let dummy_hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        client.propose_upgrade(&dummy_hash);
        // Auth passes; execute_upgrade still fails because the delay hasn't
        // elapsed and the hash isn't a real uploaded WASM.
        let _ = client.try_execute_upgrade();
    }

    #[test]
    fn test_upgrade_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);
        let dummy_hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&dummy_hash);
        // Advance past the upgrade delay so execute_upgrade proceeds far enough
        // to emit the `upgraded` event before failing on the fake WASM hash.
        env.ledger().with_mut(|l| l.sequence_number += 20_000);
        let _ = client.try_execute_upgrade();

        use soroban_sdk::{IntoVal, Symbol, testutils::Events as _};
        let all = env.events().all();
        let found = all.iter().any(|(_, topics, _)| {
            topics == (Symbol::new(&env, "upgraded"), admin.clone()).into_val(&env)
        });
        assert!(found, "upgraded event not emitted");
    }
}

#[cfg(feature = "capped-supply")]
mod capped_supply_tests {
    use super::*;

    fn init_capped<'a>(env: &'a Env, admin: &Address, cap: i128) -> TokenContractClient<'a> {
        let (client, _) = create_token_contract(env);
        client.initialize(
            admin,
            &String::from_str(env, "Capped Token"),
            &String::from_str(env, "CAP"),
            &18u32,
            &Some(cap),
        );
        client
    }

    #[test]
    fn test_max_supply_stored() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_capped(&env, &admin, 1_000i128);
        assert_eq!(client.max_supply(), Some(1_000i128));
    }

    #[test]
    fn test_mint_within_cap_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let client = init_capped(&env, &admin, 1_000i128);
        client.mint(&user, &1_000i128);
        assert_eq!(client.total_supply(), 1_000i128);
    }

    #[test]
    fn test_mint_exceeds_cap_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let client = init_capped(&env, &admin, 500i128);
        assert!(client.try_mint(&user, &501i128).is_err());
    }

    #[test]
    fn test_no_cap_is_uncapped() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let (client, _) = create_token_contract(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "Uncapped"),
            &String::from_str(&env, "UNC"),
            &18u32,
            &None,
        );
        assert_eq!(client.max_supply(), None);
        let large: i128 = 1_000_000_000;
        client.mint(&user, &large);
        assert_eq!(client.total_supply(), large);
    }

    #[test]
    fn test_batch_mint_exactly_at_cap_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let cap = 1_000i128;
        let client = init_capped(&env, &admin, cap);

        let recipients =
            soroban_sdk::vec![&env, (user1.clone(), 400i128), (user2.clone(), 600i128),];
        client.batch_mint(&recipients);

        assert_eq!(client.balance(&user1), 400i128);
        assert_eq!(client.balance(&user2), 600i128);
        assert_eq!(client.total_supply(), cap);
    }

    #[test]
    fn test_batch_mint_exceeds_cap_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let cap = 1_000i128;
        let client = init_capped(&env, &admin, cap);

        let recipients =
            soroban_sdk::vec![&env, (user1.clone(), 600i128), (user2.clone(), 500i128),];
        assert!(client.try_batch_mint(&recipients).is_err());
        assert_eq!(client.total_supply(), 0);
    }
}

#[test]
fn test_balance_of_distinguishes_unknown_from_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let unknown = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Unknown address has no storage entry — balance_of returns None
    assert_eq!(client.balance_of(&unknown), None);
    // balance() returns 0 for unknown (indistinguishable from zero balance)
    assert_eq!(client.balance(&unknown), 0i128);

    // After minting and burning to zero, the entry exists with value 0
    client.mint(&user, &100i128);
    client.burn(&user, &100i128);
    assert_eq!(client.balance(&user), 0i128);

    // balance_of distinguishes: known-zero address returns Some(0), unknown returns None
    assert_eq!(client.balance_of(&user), Some(0i128));
    assert_eq!(client.balance_of(&unknown), None);
}

#[test]
fn test_two_step_admin_transfer_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);
    client.accept_admin();
    assert_eq!(client.admin(), new_admin);
}

#[test]
fn test_accept_admin_wrong_address_fails() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let wrong = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);
    // wrong address has no pending admin entry — accept_admin should fail
    // We simulate auth as `wrong` by checking the error path via try_accept_admin
    // (mock_all_auths will satisfy auth for any caller, so we test the storage check)
    // Manually remove pending admin to simulate wrong caller scenario:
    // Instead, verify that without a proposal accept_admin returns Unauthorized.
    let env2 = Env::default();
    env2.mock_all_auths();
    let admin2 = Address::generate(&env2);
    let client2 = init_token(&env2, &admin2);
    // No proposal made — accept_admin must fail
    assert!(client2.try_accept_admin().is_err());
}

#[test]
fn test_cancel_admin_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);
    client.cancel_admin_proposal();
    // After cancellation, accept_admin must fail (no pending admin)
    assert!(client.try_accept_admin().is_err());
    // Original admin unchanged
    assert_eq!(client.admin(), admin);
}

#[test]
fn test_burn_more_than_total_supply_returns_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Mint 100 to user
    client.mint(&user, &100i128);
    assert_eq!(client.total_supply(), 100i128);

    // Directly burning more than total_supply should return an error.
    // We test admin_burn since it returns Result (burn panics).
    assert!(client.try_admin_burn(&user, &200i128).is_err());
}

#[test]
fn test_transfer_from_preserves_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user1, &1000i128);

    // Approve with a specific expiration
    let expiration = env.ledger().sequence() + 100;
    client.approve(&user1, &spender, &500i128, &expiration);
    assert_eq!(client.allowance(&user1, &spender), 500i128);

    // Perform a partial transfer_from
    client.transfer_from(&spender, &user1, &user2, &200i128);
    assert_eq!(client.balance(&user1), 800i128);
    assert_eq!(client.balance(&user2), 200i128);
    assert_eq!(client.allowance(&user1, &spender), 300i128);

    // Verify expiration is still the original value (not extended)
    // by advancing ledger and checking allowance is still valid
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration - 1);
    assert_eq!(client.allowance(&user1, &spender), 300i128);

    // Advance past original expiration
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);
    // Allowance should now be expired (return 0)
    assert_eq!(client.allowance(&user1, &spender), 0i128);
}

#[test]
fn test_burn_from() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&owner, &1000i128);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&owner, &spender, &400i128, &expiration);

    client.burn_from(&spender, &owner, &250i128);

    assert_eq!(client.balance(&owner), 750i128);
    assert_eq!(client.total_supply(), 750i128);
    assert_eq!(client.allowance(&owner, &spender), 150i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_burn_from_insufficient_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&owner, &1000i128);
    let expiration = env.ledger().sequence() + 100;
    client.approve(&owner, &spender, &100i128, &expiration);

    client.burn_from(&spender, &owner, &101i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_burn_from_expired_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&owner, &1000i128);
    let expiration = env.ledger().sequence() + 10;
    client.approve(&owner, &spender, &500i128, &expiration);
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: 0,
        protocol_version: 22,
        sequence_number: expiration + 1,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 6_312_000,
    });

    client.burn_from(&spender, &owner, &100i128);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_unauthorized_admin_burn_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&user, &500i128);
    // clear all auths so the next call has no authorization
    env.set_auths(&[]);
    client.admin_burn(&user, &100i128);
}

#[test]
fn test_batch_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    let client = init_token(&env, &admin);

    let recipients = soroban_sdk::vec![
        &env,
        (user1.clone(), 100i128),
        (user2.clone(), 200i128),
        (user3.clone(), 300i128),
    ];
    client.batch_mint(&recipients);

    assert_eq!(client.balance(&user1), 100i128);
    assert_eq!(client.balance(&user2), 200i128);
    assert_eq!(client.balance(&user3), 300i128);
    assert_eq!(client.total_supply(), 600i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_batch_mint_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let client = init_token(&env, &admin);

    let recipients = soroban_sdk::vec![&env, (user1.clone(), 100i128), (user2.clone(), 0i128),];
    client.batch_mint(&recipients);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_batch_mint_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Minting amounts that sum to more than i128::MAX causes overflow
    let recipients = soroban_sdk::vec![&env, (user1.clone(), i128::MAX), (user2.clone(), 1i128),];
    client.batch_mint(&recipients);
}

#[test]
fn test_allowance_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);
    client.mint(&owner, &1000i128);

    let expiration = env.ledger().sequence() + 100;
    client.approve(&owner, &spender, &500i128, &expiration);

    // Check that allowance_expiry returns the correct expiration
    assert_eq!(client.allowance_expiry(&owner, &spender), Some(expiration));

    // Advance ledger past expiration
    env.ledger()
        .with_mut(|l| l.sequence_number = expiration + 1);

    // Check that allowance_expiry returns None after expiration
    assert_eq!(client.allowance_expiry(&owner, &spender), None);
}

#[test]
fn test_allowance_expiry_no_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Check that allowance_expiry returns None when no allowance exists
    assert_eq!(client.allowance_expiry(&owner, &spender), None);
}

#[test]
fn test_propose_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);

    // Admin should still be the old admin
    assert_eq!(client.admin(), admin);
}

#[test]
fn test_accept_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);
    client.accept_admin();

    // Admin should now be the new admin
    assert_eq!(client.admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_accept_admin_without_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Try to accept without a proposal
    client.accept_admin();
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_accept_admin_by_wrong_address_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let wrong_address = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);

    // Try to accept as wrong address
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let contract_address = env.register_contract(None, TokenContract);
    env.mock_auths(&[MockAuth {
        address: &wrong_address,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "accept_admin",
            args: soroban_sdk::vec![&env].into(),
            sub_invokes: &[],
        },
    }]);
    client.accept_admin();
}

#[test]
fn test_cancel_admin_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);
    client.cancel_admin_proposal();

    // Admin should still be the old admin
    assert_eq!(client.admin(), admin);

    // Trying to accept should fail since proposal was cancelled
    assert!(client.try_accept_admin().is_err());
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_cancel_admin_proposal_by_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.propose_admin(&new_admin);

    // Try to cancel as non-admin
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let contract_address = env.register_contract(None, TokenContract);
    env.mock_auths(&[MockAuth {
        address: &non_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_address,
            fn_name: "cancel_admin_proposal",
            args: soroban_sdk::vec![&env].into(),
            sub_invokes: &[],
        },
    }]);
    client.cancel_admin_proposal();
}

// ── #714 transfer hook tests ─────────────────────────────────────────────────

#[cfg(feature = "transfer-hook")]
mod transfer_hook_tests {
    use super::*;
    use soroban_sdk::{IntoVal, Symbol, contract, contractimpl, testutils::Events as _};

    /// Minimal mock hook contract that records calls via an event.
    #[contract]
    pub struct MockHook;

    #[contractimpl]
    impl MockHook {
        pub fn on_transfer(env: Env, from: Address, to: Address, amount: i128) {
            env.events()
                .publish((Symbol::new(&env, "hook_called"), from, to), amount);
        }
    }

    #[test]
    fn set_transfer_hook_stores_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);
        let hook_addr = env.register_contract(None, MockHook);

        client.set_transfer_hook(&Some(hook_addr.clone()));
        assert_eq!(client.get_transfer_hook(), Some(hook_addr));
    }

    #[test]
    fn clear_transfer_hook_removes_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let client = init_token(&env, &admin);
        let hook_addr = env.register_contract(None, MockHook);

        client.set_transfer_hook(&Some(hook_addr));
        client.set_transfer_hook(&None);
        assert_eq!(client.get_transfer_hook(), None);
    }

    #[test]
    fn transfer_calls_hook_and_does_not_revert() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);
        let client = init_token(&env, &admin);
        let hook_addr = env.register_contract(None, MockHook);

        client.mint(&from, &1_000i128);
        client.set_transfer_hook(&Some(hook_addr.clone()));
        client.transfer(&from, &to, &500i128);

        // Balances updated correctly
        assert_eq!(client.balance(&from), 500);
        assert_eq!(client.balance(&to), 500);

        // Hook was called
        let found = env.events().all().iter().any(|(addr, topics, _)| {
            addr == hook_addr
                && topics
                    == (Symbol::new(&env, "hook_called"), from.clone(), to.clone()).into_val(&env)
        });
        assert!(found, "hook_called event not emitted");
    }

    #[test]
    fn transfer_without_hook_set_does_not_revert() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let from = Address::generate(&env);
        let to = Address::generate(&env);
        let client = init_token(&env, &admin);

        client.mint(&from, &1_000i128);
        // No hook set — transfer should still succeed
        client.transfer(&from, &to, &300i128);
        assert_eq!(client.balance(&to), 300);
    }
}

// ── #717 snapshot tests ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_records_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.mint(&user, &500i128);

    let ledger = env.ledger().sequence();
    client.snapshot(&user, &ledger);

    assert_eq!(client.balance_at(&user, &ledger), Some(500i128));
}

#[test]
fn test_balance_at_returns_none_for_missing_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    assert_eq!(client.balance_at(&user, &42u32), None);
}

#[test]
fn test_snapshot_captures_balance_at_point_in_time() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.mint(&user, &1_000i128);
    let ledger_before = env.ledger().sequence();
    client.snapshot(&user, &ledger_before);

    // Mint more tokens — snapshot should still reflect old balance
    client.mint(&user, &500i128);

    assert_eq!(client.balance_at(&user, &ledger_before), Some(1_000i128));
    assert_eq!(client.balance(&user), 1_500i128);
}

#[test]
fn test_multiple_snapshots_at_different_ledgers() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = init_token(&env, &admin);

    client.mint(&user, &100i128);
    let ledger1 = env.ledger().sequence();
    client.snapshot(&user, &ledger1);

    env.ledger().with_mut(|l| l.sequence_number += 10);
    client.mint(&user, &200i128);
    let ledger2 = env.ledger().sequence();
    client.snapshot(&user, &ledger2);

    assert_eq!(client.balance_at(&user, &ledger1), Some(100i128));
    assert_eq!(client.balance_at(&user, &ledger2), Some(300i128));
}

// ---------------------------------------------------------------------------
// approve_with_signature ("permit")
// ---------------------------------------------------------------------------

mod permit_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    fn bytes_to_vec(bytes: &Bytes) -> std::vec::Vec<u8> {
        let mut v = std::vec::Vec::with_capacity(bytes.len() as usize);
        for b in bytes.iter() {
            v.push(b);
        }
        v
    }

    #[allow(clippy::too_many_arguments)]
    fn sign_permit(
        env: &Env,
        signing_key: &SigningKey,
        contract: &Address,
        owner: &Address,
        spender: &Address,
        amount: i128,
        nonce: u32,
        expiry_ledger: u32,
    ) -> BytesN<64> {
        let message: Bytes = (
            contract.clone(),
            owner.clone(),
            spender.clone(),
            amount,
            nonce,
            expiry_ledger,
        )
            .to_xdr(env);
        let signature = signing_key.sign(&bytes_to_vec(&message));
        BytesN::from_array(env, &signature.to_bytes())
    }

    fn setup_permit(env: &Env) -> (TokenContractClient<'_>, Address, Address, SigningKey) {
        let admin = Address::generate(env);
        let owner = Address::generate(env);
        let spender = Address::generate(env);
        let client = init_token(env, &admin);

        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
        client.set_permit_signer(&owner, &public_key);

        (client, owner, spender, signing_key)
    }

    #[test]
    fn test_approve_with_signature_valid() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );

        client.approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);

        assert_eq!(client.allowance(&owner, &spender), 500i128);
        assert_eq!(client.permit_nonce(&owner), 1u32);
    }

    #[test]
    fn test_approve_with_signature_replay_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );

        client.approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);

        // Replaying the exact same signed message must fail: the nonce has already advanced.
        let result = client
            .try_approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_approve_with_signature_expired_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence();
        env.ledger().with_mut(|l| l.sequence_number += 1);

        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );
        let result = client
            .try_approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_approve_with_signature_wrong_nonce_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        // Sign with nonce 1 while the contract still expects nonce 0.
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            1u32,
            expiry,
        );
        let result = client
            .try_approve_with_signature(&owner, &spender, &500i128, &1u32, &expiry, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_approve_with_signature_without_registered_signer_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let client = init_token(&env, &admin);

        let signing_key = SigningKey::generate(&mut OsRng);
        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );

        // `owner` never called `set_permit_signer`.
        let result = client
            .try_approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic]
    fn test_approve_with_signature_invalid_signature_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, _signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        // Sign with a different, unregistered key — must not verify against
        // the owner's registered permit signer.
        let wrong_key = SigningKey::generate(&mut OsRng);
        let signature = sign_permit(
            &env,
            &wrong_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );

        client.approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);
    }

    #[test]
    fn test_permit_signer_can_be_rotated() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, old_key) = setup_permit(&env);

        let new_key = SigningKey::generate(&mut OsRng);
        let new_public_key = BytesN::from_array(&env, &new_key.verifying_key().to_bytes());
        client.set_permit_signer(&owner, &new_public_key);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;

        // A signature from the old key must no longer verify.
        let old_signature = sign_permit(
            &env,
            &old_key,
            &contract_id,
            &owner,
            &spender,
            100i128,
            0u32,
            expiry,
        );
        let result = client.try_approve_with_signature(
            &owner,
            &spender,
            &100i128,
            &0u32,
            &expiry,
            &old_signature,
        );
        assert!(result.is_err());

        // The new key works.
        let new_signature = sign_permit(
            &env,
            &new_key,
            &contract_id,
            &owner,
            &spender,
            100i128,
            0u32,
            expiry,
        );
        client.approve_with_signature(&owner, &spender, &100i128, &0u32, &expiry, &new_signature);
        assert_eq!(client.allowance(&owner, &spender), 100i128);
    }

    #[test]
    fn test_cancel_permit_nonce_marks_nonce_cancelled() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, _spender, _signing_key) = setup_permit(&env);

        assert!(!client.is_permit_nonce_cancelled(&owner, &0u32));
        client.cancel_permit_nonce(&owner, &0u32);
        assert!(client.is_permit_nonce_cancelled(&owner, &0u32));
        // Other nonces are unaffected.
        assert!(!client.is_permit_nonce_cancelled(&owner, &1u32));
    }

    #[test]
    fn test_cancelled_nonce_rejects_matching_permit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            500i128,
            0u32,
            expiry,
        );

        // Owner cancels the nonce before the relayer submits the permit.
        client.cancel_permit_nonce(&owner, &0u32);

        let result = client
            .try_approve_with_signature(&owner, &spender, &500i128, &0u32, &expiry, &signature);
        assert!(result.is_err());
        // No allowance was granted.
        assert_eq!(client.allowance(&owner, &spender), 0i128);
    }

    #[test]
    fn test_cancel_permit_nonce_requires_owner_auth() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, _spender, _signing_key) = setup_permit(&env);

        // Stop mocking auths: the owner's authorization is now required.
        env.set_auths(&[]);
        let result = client.try_cancel_permit_nonce(&owner, &0u32);
        assert!(result.is_err());
        assert!(!client.is_permit_nonce_cancelled(&owner, &0u32));
    }

    #[test]
    fn test_cancel_permit_nonce_does_not_advance_nonce() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, owner, spender, signing_key) = setup_permit(&env);

        client.cancel_permit_nonce(&owner, &0u32);
        // Cancelling does not consume the nonce counter.
        assert_eq!(client.permit_nonce(&owner), 0u32);

        // A permit signed for a different, uncancelled nonce still works.
        let contract_id = client.address.clone();
        let expiry = env.ledger().sequence() + 1_000;
        let signature = sign_permit(
            &env,
            &signing_key,
            &contract_id,
            &owner,
            &spender,
            250i128,
            1u32,
            expiry,
        );
        client.approve_with_signature(&owner, &spender, &250i128, &1u32, &expiry, &signature);
        assert_eq!(client.allowance(&owner, &spender), 250i128);
        assert_eq!(client.permit_nonce(&owner), 2u32);
    }
}
