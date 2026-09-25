#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, FromVal, IntoVal, Symbol, contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    vec,
};

#[contract]
pub struct CounterContract;

#[contractimpl]
impl CounterContract {
    pub fn increment(env: Env, amount: u32) -> u32 {
        let current = Self::get(env.clone());
        let next = current + amount;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "count"), &next);
        next
    }

    pub fn get(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "count"))
            .unwrap_or(0)
    }
}

/// Helper: create a 2-of-3 multisig with uniform weights (no weights param).
fn create_multisig<'a>(
    env: &'a Env,
) -> (
    MultisigContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    let carol = Address::generate(env);
    let contract_address = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(env, &contract_address);

    client.initialize(
        &vec![env, alice.clone(), bob.clone(), carol.clone()],
        &2,
        &None,
    );

    (client, alice, bob, carol, contract_address)
}

#[test]
fn initialize_stores_signers_and_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, alice, bob, carol, contract_address) = create_multisig(&env);

    assert_eq!(client.get_threshold(), Some(2));
    assert_eq!(client.get_signers(), vec![&env, alice, bob, carol]);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_address,
                (Symbol::new(&env, "initialized"), 2u32).into_val(&env),
                3u32.into_val(&env),
            )
        ]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn initialize_rejects_zero_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let alice = Address::generate(&env);
    let contract_address = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(&env, &contract_address);

    client.initialize(&vec![&env, alice], &0, &None);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn initialize_rejects_duplicate_signers() {
    let env = Env::default();
    env.mock_all_auths();

    let alice = Address::generate(&env);
    let contract_address = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(&env, &contract_address);

    client.initialize(&vec![&env, alice.clone(), alice], &1, &None);
}

#[test]
fn add_signer_with_threshold_approvals_updates_signer_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, _) = create_multisig(&env);
    let dave = Address::generate(&env);

    client.add_signer(&vec![&env, alice.clone(), bob.clone()], &dave, &1, &3);

    assert_eq!(client.get_threshold(), Some(3));
    assert_eq!(client.get_signers(), vec![&env, alice, bob, carol, dave]);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn add_signer_rejects_insufficient_approvals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let dave = Address::generate(&env);

    client.add_signer(&vec![&env, alice], &dave, &1, &2);
}

#[test]
fn remove_signer_with_threshold_approvals_updates_signer_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, _) = create_multisig(&env);

    client.remove_signer(&vec![&env, alice.clone(), bob.clone()], &carol, &2);

    assert_eq!(client.get_threshold(), Some(2));
    assert_eq!(client.get_signers(), vec![&env, alice, bob]);
    assert!(!client.is_signer(&carol));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn remove_signer_rejects_threshold_above_remaining_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, _) = create_multisig(&env);

    client.remove_signer(&vec![&env, alice, bob], &carol, &3);
}

#[test]
fn propose_transaction_stores_transaction_and_auto_signature() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 7u32.into_val(&env)],
        &100u32,
    );

    let transaction = client.get_transaction(&tx_id).expect("transaction exists");
    assert_eq!(tx_id, 0);
    assert_eq!(transaction.proposer, alice.clone());
    assert_eq!(transaction.target, target);
    assert_eq!(transaction.signatures, vec![&env, alice]);
    assert!(!transaction.executed);
    assert_eq!(client.signature_count(&tx_id), Some(1));
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn non_signer_cannot_propose_transaction() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = create_multisig(&env);
    let outsider = Address::generate(&env);
    let target = env.register_contract(None, CounterContract);

    client.propose_transaction(
        &outsider,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn signer_cannot_sign_same_transaction_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );

    client.sign_transaction(&alice, &tx_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn execute_rejects_when_threshold_not_met() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );

    client.execute_transaction(&tx_id);
}

#[test]
fn execute_runs_target_call_once_when_threshold_met() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let counter = CounterContractClient::new(&env, &target);
    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 5u32.into_val(&env)],
        &100u32,
    );

    client.sign_transaction(&bob, &tx_id);
    let result = client.execute_transaction(&tx_id);
    let value = u32::from_val(&env, &result);

    assert_eq!(value, 5);
    assert_eq!(counter.get(), 5);
    assert!(
        client
            .get_transaction(&tx_id)
            .expect("transaction exists")
            .executed
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn execute_rejects_second_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 5u32.into_val(&env)],
        &100u32,
    );

    client.sign_transaction(&bob, &tx_id);
    client.execute_transaction(&tx_id);
    client.execute_transaction(&tx_id);
}

// ── #715 proposal expiry tests ───────────────────────────────────────────────

#[test]
fn proposal_stores_expiry_ledger() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &50u32,
    );

    let tx = client.get_transaction(&tx_id).expect("transaction exists");
    assert_eq!(tx.expiry_ledger, env.ledger().sequence() + 50);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn sign_after_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &10u32,
    );

    // Advance ledger past expiry
    env.ledger().with_mut(|l| l.sequence_number += 11);
    client.sign_transaction(&bob, &tx_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn execute_after_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &10u32,
    );
    client.sign_transaction(&bob, &tx_id);

    // Advance ledger past expiry
    env.ledger().with_mut(|l| l.sequence_number += 11);
    client.execute_transaction(&tx_id);
}

#[test]
fn cleanup_expired_removes_proposal_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, contract_address) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &10u32,
    );

    // Advance past expiry
    env.ledger().with_mut(|l| l.sequence_number += 11);
    client.cleanup_expired(&tx_id);

    // Proposal should be gone
    assert_eq!(client.get_transaction(&tx_id), None);

    // expired event emitted
    let found = env.events().all().iter().any(|(_, topics, _)| {
        topics == (Symbol::new(&env, "expired"), tx_id).into_val(&env)
    });
    assert!(found, "expired event not emitted");
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn cleanup_not_yet_expired_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );

    // Not yet expired — should fail
    client.cleanup_expired(&tx_id);
}

// ── #825 weighted voting tests ───────────────────────────────────────────────

/// Helper: weighted multisig where alice=3, bob=2, carol=1; threshold=4.
fn create_weighted_multisig<'a>(
    env: &'a Env,
) -> (
    MultisigContractClient<'a>,
    Address,
    Address,
    Address,
) {
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    let carol = Address::generate(env);
    let contract_address = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(env, &contract_address);

    let weights = vec![
        env,
        SignerWeight { signer: alice.clone(), weight: 3 },
        SignerWeight { signer: bob.clone(),   weight: 2 },
        SignerWeight { signer: carol.clone(), weight: 1 },
    ];
    // threshold = 4; alice alone (3) is not enough; alice+bob (5) is enough.
    client.initialize(
        &vec![env, alice.clone(), bob.clone(), carol.clone()],
        &4,
        &Some(weights),
    );

    (client, alice, bob, carol)
}

#[test]
fn weighted_initialize_stores_weights() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol) = create_weighted_multisig(&env);

    assert_eq!(client.get_signer_weight(&alice), 3);
    assert_eq!(client.get_signer_weight(&bob),   2);
    assert_eq!(client.get_signer_weight(&carol),  1);
    assert_eq!(client.get_threshold(), Some(4));
}

#[test]
fn weighted_threshold_met_with_two_heavy_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _carol) = create_weighted_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let counter = CounterContractClient::new(&env, &target);

    // alice proposes (accumulated = 3), bob signs (accumulated = 5 >= 4)
    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 10u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx_id);
    client.execute_transaction(&tx_id);

    assert_eq!(counter.get(), 10);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn weighted_threshold_not_met_with_single_heavy_signer() {
    let env = Env::default();
    env.mock_all_auths();
    // alice (weight 3) alone does not meet threshold 4
    let (client, alice, _, _) = create_weighted_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    // should panic: weight 3 < threshold 4
    client.execute_transaction(&tx_id);
}

#[test]
fn weighted_accumulated_weight_stored_on_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _) = create_weighted_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice,
        &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    let tx_after_propose = client.get_transaction(&tx_id).unwrap();
    assert_eq!(tx_after_propose.accumulated_weight, 3); // alice weight

    client.sign_transaction(&bob, &tx_id);
    let tx_after_bob = client.get_transaction(&tx_id).unwrap();
    assert_eq!(tx_after_bob.accumulated_weight, 5); // alice(3) + bob(2)
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn initialize_rejects_zero_weight() {
    let env = Env::default();
    env.mock_all_auths();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let contract_address = env.register_contract(None, MultisigContract);
    let client = MultisigContractClient::new(&env, &contract_address);

    let weights = vec![
        &env,
        SignerWeight { signer: alice.clone(), weight: 0 }, // zero weight → error
        SignerWeight { signer: bob.clone(),   weight: 1 },
    ];
    client.initialize(&vec![&env, alice, bob], &1, &Some(weights));
}

// ── #826 batch execution tests ───────────────────────────────────────────────

#[test]
fn execute_batch_executes_all_ready_proposals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);
    let counter = CounterContractClient::new(&env, &target);

    // Propose two transactions, both signed by alice+bob (threshold=2).
    let tx0 = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx0);

    let tx1 = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 2u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx1);

    let executed = client.execute_batch(&vec![&env, tx0, tx1]);

    assert_eq!(executed.len(), 2);
    assert_eq!(counter.get(), 3); // 1 + 2
}

#[test]
fn execute_batch_skips_already_executed_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 5u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx_id);
    // Execute individually first.
    client.execute_transaction(&tx_id);

    // A second proposal that can still execute.
    let tx1 = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx1);

    // Batch: already-executed tx_id is skipped, tx1 executes.
    let executed = client.execute_batch(&vec![&env, tx_id, tx1]);
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), tx1);
}

#[test]
fn execute_batch_skips_nonexistent_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 7u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx_id);

    // Include a nonexistent ID (999) alongside valid tx_id.
    let executed = client.execute_batch(&vec![&env, 999u64, tx_id]);
    // 999 skipped, tx_id executed.
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), tx_id);
}

#[test]
fn execute_batch_skips_threshold_not_met_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    // tx0: only alice signed (weight 1, threshold 2) — not ready.
    let tx0 = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );

    // tx1: alice+bob signed — ready.
    let tx1 = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 3u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx1);

    let executed = client.execute_batch(&vec![&env, tx0, tx1]);
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), tx1);
}

#[test]
fn execute_batch_emits_batch_executed_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    let target = env.register_contract(None, CounterContract);

    let tx_id = client.propose_transaction(
        &alice, &target,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &tx_id);

    client.execute_batch(&vec![&env, tx_id]);

    let found = env
        .events()
        .all()
        .iter()
        .any(|(_, topics, _)| topics == (Symbol::new(&env, "batch_executed"),).into_val(&env));
    assert!(found, "batch_executed event not emitted");
}

// ── #1117 proposal enumeration and pagination ────────────────────────────────

/// Helper: propose `n` counter increments from `proposer`, returning the target.
fn propose_n(env: &Env, client: &MultisigContractClient, proposer: &Address, n: u32) -> Address {
    let target = env.register_contract(None, CounterContract);
    for _ in 0..n {
        client.propose_transaction(
            proposer,
            &target,
            &Symbol::new(env, "increment"),
            &vec![env, 1u32.into_val(env)],
            &100u32,
        );
    }
    target
}

fn page_ids(page: &TransactionPage) -> std::vec::Vec<u64> {
    page.items.iter().map(|tx| tx.id).collect()
}

#[test]
fn get_transactions_paginates_forward() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    propose_n(&env, &client, &alice, 5);

    let p1 = client.get_transactions(&0, &2, &None, &false);
    assert_eq!(page_ids(&p1), [0, 1]);
    assert_eq!(p1.next_cursor, Some(2));

    let p2 = client.get_transactions(&2, &2, &None, &false);
    assert_eq!(page_ids(&p2), [2, 3]);
    assert_eq!(p2.next_cursor, Some(4));

    let p3 = client.get_transactions(&4, &2, &None, &false);
    assert_eq!(page_ids(&p3), [4]);
    assert_eq!(p3.next_cursor, None);

    let past_end = client.get_transactions(&5, &2, &None, &false);
    assert!(past_end.items.is_empty());
    assert_eq!(past_end.next_cursor, None);
}

#[test]
fn get_transactions_paginates_backward() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    propose_n(&env, &client, &alice, 5);

    let p1 = client.get_transactions(&u64::MAX, &2, &None, &true);
    assert_eq!(page_ids(&p1), [4, 3]);
    assert_eq!(p1.next_cursor, Some(2));

    let p2 = client.get_transactions(&2, &2, &None, &true);
    assert_eq!(page_ids(&p2), [2, 1]);
    assert_eq!(p2.next_cursor, Some(0));

    let p3 = client.get_transactions(&0, &2, &None, &true);
    assert_eq!(page_ids(&p3), [0]);
    assert_eq!(p3.next_cursor, None);
}

#[test]
fn get_transactions_empty_wallet_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = create_multisig(&env);

    for descending in [false, true] {
        let page = client.get_transactions(&0, &10, &None, &descending);
        assert!(page.items.is_empty());
        assert_eq!(page.next_cursor, None);
    }
}

#[test]
fn get_transactions_filters_by_status() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, _) = create_multisig(&env);
    propose_n(&env, &client, &alice, 4);

    // 0: pending, 1: executed, 2: cancelled, 3: pending
    client.sign_transaction(&bob, &1);
    client.execute_transaction(&1);
    client.cancel_transaction(&carol, &2);

    let executed = client.get_transactions(&0, &10, &Some(TxStatus::Executed), &false);
    assert_eq!(page_ids(&executed), [1]);

    let cancelled = client.get_transactions(&0, &10, &Some(TxStatus::Cancelled), &false);
    assert_eq!(page_ids(&cancelled), [2]);

    let pending = client.get_transactions(&0, &10, &Some(TxStatus::Pending), &false);
    assert_eq!(page_ids(&pending), [0, 3]);

    env.ledger().with_mut(|l| l.sequence_number += 101);
    let expired = client.get_transactions(&0, &10, &Some(TxStatus::Expired), &false);
    assert_eq!(page_ids(&expired), [0, 3]);
}

// ── #1114 reentrancy guard ───────────────────────────────────────────────────

/// Malicious target that tries to re-enter the multisig and execute another
/// pending proposal while its own dispatch is still in progress.
#[contract]
pub struct ReentrantAttacker;

#[contractimpl]
impl ReentrantAttacker {
    pub fn attack(env: Env, multisig: Address, tx_id: u64) {
        MultisigContractClient::new(&env, &multisig).execute_transaction(&tx_id);
    }
}

#[test]
#[should_panic]
fn reentrant_execution_from_invoked_contract_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, contract_address) = create_multisig(&env);
    let attacker = env.register_contract(None, ReentrantAttacker);
    let counter = env.register_contract(None, CounterContract);

    // Proposal 1: an innocent-looking counter call the attacker wants to run early.
    let victim_id = 1u64;
    let attack_id = client.propose_transaction(
        &alice,
        &attacker,
        &Symbol::new(&env, "attack"),
        &vec![&env, contract_address.into_val(&env), victim_id.into_val(&env)],
        &100u32,
    );
    client.propose_transaction(
        &alice,
        &counter,
        &Symbol::new(&env, "increment"),
        &vec![&env, 1u32.into_val(&env)],
        &100u32,
    );
    client.sign_transaction(&bob, &attack_id);
    client.sign_transaction(&bob, &victim_id);

    client.execute_transaction(&attack_id);
}

#[test]
fn entry_points_reject_calls_while_lock_is_held() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, contract_address) = create_multisig(&env);
    propose_n(&env, &client, &alice, 1);
    client.sign_transaction(&bob, &0);

    env.as_contract(&contract_address, || {
        env.storage().instance().set(&DataKey::ReentrancyLock, &true);
    });

    let reentrant = Err(Ok(MultisigError::Reentrant));
    let approvals = vec![&env, alice.clone(), bob.clone()];
    assert_eq!(client.try_execute_transaction(&0).map(|_| ()), reentrant);
    assert_eq!(client.try_sign_transaction(&carol, &0).map(|_| ()), reentrant);
    assert_eq!(client.try_cancel_transaction(&carol, &0).map(|_| ()), reentrant);
    assert_eq!(
        client
            .try_add_signer(&approvals, &Address::generate(&env), &2)
            .map(|_| ()),
        reentrant
    );
    assert_eq!(
        client.try_remove_signer(&approvals, &carol, &2).map(|_| ()),
        reentrant
    );
    assert_eq!(
        client.try_set_timelock_delay(&approvals, &10).map(|_| ()),
        reentrant
    );
    assert_eq!(
        client.try_spend_allowance(&alice, &bob, &1).map(|_| ()),
        reentrant
    );
    assert!(client.try_execute_batch(&vec![&env, 0u64]).is_err());

    env.as_contract(&contract_address, || {
        env.storage().instance().remove(&DataKey::ReentrancyLock);
    });
    client.execute_transaction(&0);
}

#[test]
fn execution_lock_is_released_after_dispatch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, contract_address) = create_multisig(&env);
    propose_n(&env, &client, &alice, 2);
    client.sign_transaction(&bob, &0);
    client.sign_transaction(&bob, &1);

    client.execute_transaction(&0);
    let locked = env.as_contract(&contract_address, || {
        env.storage().instance().has(&DataKey::ReentrancyLock)
    });
    assert!(!locked);

    // A subsequent, non-reentrant execution still succeeds.
    client.execute_transaction(&1);
}

// ── #1116 timelock ───────────────────────────────────────────────────────────

#[test]
fn timelock_delays_execution_until_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    client.set_timelock_delay(&vec![&env, alice.clone(), bob.clone()], &50);
    assert_eq!(client.get_timelock_delay(), 50);

    let target = propose_n(&env, &client, &alice, 1);
    let counter = CounterContractClient::new(&env, &target);
    assert_eq!(client.get_transaction_status(&0), Some(TxStatus::Pending));

    client.sign_transaction(&bob, &0);
    let queued_at = env.ledger().sequence();
    let tx = client.get_transaction(&0).unwrap();
    assert_eq!(tx.queued_ledger, Some(queued_at));
    assert_eq!(client.get_transaction_status(&0), Some(TxStatus::Queued));

    assert_eq!(
        client.try_execute_transaction(&0).map(|_| ()),
        Err(Ok(MultisigError::TimelockNotElapsed))
    );

    env.ledger().with_mut(|l| l.sequence_number = queued_at + 49);
    assert_eq!(
        client.try_execute_transaction(&0).map(|_| ()),
        Err(Ok(MultisigError::TimelockNotElapsed))
    );

    env.ledger().with_mut(|l| l.sequence_number = queued_at + 50);
    client.execute_transaction(&0);
    assert_eq!(counter.get(), 1);
}

#[test]
fn signer_can_cancel_queued_transaction_during_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, carol, _) = create_multisig(&env);
    client.set_timelock_delay(&vec![&env, alice.clone(), bob.clone()], &50);

    propose_n(&env, &client, &alice, 1);
    client.sign_transaction(&bob, &0);

    client.cancel_transaction(&carol, &0);
    assert_eq!(client.get_transaction_status(&0), Some(TxStatus::Cancelled));

    env.ledger().with_mut(|l| l.sequence_number += 50);
    assert_eq!(
        client.try_execute_transaction(&0).map(|_| ()),
        Err(Ok(MultisigError::TransactionCancelled))
    );
}

#[test]
fn non_signer_cannot_cancel_transaction() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);
    propose_n(&env, &client, &alice, 1);

    assert_eq!(
        client.try_cancel_transaction(&Address::generate(&env), &0),
        Err(Ok(MultisigError::NotSigner))
    );
}

#[test]
fn set_timelock_delay_requires_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, _, _, _) = create_multisig(&env);

    assert_eq!(
        client.try_set_timelock_delay(&vec![&env, alice], &50),
        Err(Ok(MultisigError::InsufficientApprovals))
    );
}

#[test]
fn queue_transaction_queues_after_threshold_lowered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);
    client.set_timelock_delay(&vec![&env, alice.clone(), bob.clone()], &10);
    propose_n(&env, &client, &alice, 1);

    // Not yet at threshold, so the proposal cannot be queued.
    assert_eq!(
        client.try_queue_transaction(&0),
        Err(Ok(MultisigError::ThresholdNotMet))
    );

    // Lower threshold to 1 via a signer-set change; proposal now qualifies.
    let dave = Address::generate(&env);
    client.add_signer(&vec![&env, alice.clone(), bob.clone()], &dave, &1);
    assert_eq!(
        client.try_execute_transaction(&0).map(|_| ()),
        Err(Ok(MultisigError::NotQueued))
    );

    let executable_at = client.queue_transaction(&0);
    assert_eq!(executable_at, env.ledger().sequence() + 10);
    env.ledger().with_mut(|l| l.sequence_number = executable_at);
    client.execute_transaction(&0);
}

// ── #1115 daily spending allowance ───────────────────────────────────────────

fn setup_allowance<'a>(
    env: &'a Env,
    daily_limit: i128,
) -> (
    MultisigContractClient<'a>,
    Address,
    soroban_sdk::token::Client<'a>,
) {
    let (client, alice, bob, _, contract_address) = create_multisig(env);
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token_address = sac.address();
    soroban_sdk::token::StellarAssetClient::new(env, &token_address)
        .mint(&contract_address, &1_000_000);

    let operator = Address::generate(env);
    client.set_spending_limit(
        &vec![env, alice, bob],
        &operator,
        &token_address,
        &daily_limit,
    );
    (
        client,
        operator,
        soroban_sdk::token::Client::new(env, &token_address),
    )
}

#[test]
fn operator_spends_within_allowance_without_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, operator, token) = setup_allowance(&env, 100);
    let payee = Address::generate(&env);

    assert_eq!(client.spend_allowance(&operator, &payee, &60), 60);
    assert_eq!(token.balance(&payee), 60);
    assert_eq!(client.remaining_allowance(), 40);

    assert_eq!(
        client.try_spend_allowance(&operator, &payee, &41),
        Err(Ok(MultisigError::DailyLimitExceeded))
    );
    assert_eq!(client.spend_allowance(&operator, &payee, &40), 100);
    assert_eq!(token.balance(&payee), 100);
}

#[test]
fn allowance_resets_after_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, operator, token) = setup_allowance(&env, 100);
    let payee = Address::generate(&env);

    client.spend_allowance(&operator, &payee, &100);
    assert_eq!(
        client.try_spend_allowance(&operator, &payee, &1),
        Err(Ok(MultisigError::DailyLimitExceeded))
    );

    env.ledger()
        .with_mut(|l| l.sequence_number += SPENDING_WINDOW_LEDGERS);
    assert_eq!(client.remaining_allowance(), 100);
    assert_eq!(client.spend_allowance(&operator, &payee, &70), 70);
    assert_eq!(token.balance(&payee), 170);
}

#[test]
fn allowance_spend_count_is_rate_limited() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, operator, _) = setup_allowance(&env, 1_000);
    let payee = Address::generate(&env);

    for _ in 0..MAX_SPENDS_PER_WINDOW {
        client.spend_allowance(&operator, &payee, &1);
    }
    assert_eq!(
        client.try_spend_allowance(&operator, &payee, &1),
        Err(Ok(MultisigError::RateLimited))
    );
}

#[test]
fn non_operator_cannot_spend_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _) = setup_allowance(&env, 100);

    assert_eq!(
        client.try_spend_allowance(&Address::generate(&env), &Address::generate(&env), &1),
        Err(Ok(MultisigError::NotSpendingOperator))
    );
}

#[test]
fn spend_without_configuration_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);

    assert_eq!(
        client.try_spend_allowance(&alice, &bob, &1),
        Err(Ok(MultisigError::SpendingNotConfigured))
    );
}

#[test]
fn set_spending_limit_requires_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, alice, bob, _, _) = create_multisig(&env);

    assert_eq!(
        client.try_set_spending_limit(&vec![&env, alice.clone()], &alice, &bob, &100),
        Err(Ok(MultisigError::InsufficientApprovals))
    );
}
