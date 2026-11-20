#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![cfg(test)]

use std::format;

use proptest::prelude::*;
use soroban_sdk::{Address, Env, testutils::Address as _, vec};

use crate::{MultisigContract, MultisigContractClient};

fn signer_vec(env: &Env, count: u32) -> soroban_sdk::Vec<Address> {
    let mut signers = soroban_sdk::Vec::new(env);
    for _ in 0..count {
        signers.push_back(Address::generate(env));
    }
    signers
}

proptest! {
    #[test]
    fn prop_valid_thresholds_initialize(signer_count in 1u32..=10, threshold in 1u32..=10) {
        let env = Env::default();
        env.mock_all_auths();
        let signers = signer_vec(&env, signer_count);
        let contract_address = env.register_contract(None, MultisigContract);
        let client = MultisigContractClient::new(&env, &contract_address);

        let result = client.try_initialize(&signers, &threshold, &None);
        if threshold <= signer_count {
            prop_assert!(result.is_ok());
            prop_assert_eq!(client.get_threshold(), Some(threshold));
            prop_assert_eq!(client.get_signers().len(), signer_count);
        } else {
            prop_assert!(result.is_err());
        }
    }

    #[test]
    fn prop_threshold_change_requires_valid_range_and_quorum(signer_count in 1u32..=10, new_threshold in 0u32..=12) {
        let env = Env::default();
        env.mock_all_auths();
        let signers = signer_vec(&env, signer_count);
        let contract_address = env.register_contract(None, MultisigContract);
        let client = MultisigContractClient::new(&env, &contract_address);
        client.initialize(&signers, &1, &None);

        let proposer = signers.get(0).unwrap();
        let result = client.try_propose_threshold_change(&proposer, &new_threshold);

        if new_threshold >= 1 && new_threshold <= signer_count {
            prop_assert!(result.is_ok());

            // Execution must fail until the current threshold (1) approves.
            let no_approvals = soroban_sdk::Vec::new(&env);
            prop_assert!(client.try_execute_threshold_change(&no_approvals).is_err());

            let approvals = vec![&env, proposer.clone()];
            let exec = client.try_execute_threshold_change(&approvals);
            prop_assert!(exec.is_ok());
            prop_assert_eq!(client.get_threshold(), Some(new_threshold));
        } else {
            prop_assert!(result.is_err());
            prop_assert_eq!(client.get_threshold(), Some(1));
        }
    }

    #[test]
    fn prop_duplicate_approvals_never_satisfy_threshold(extra_signers in 0u32..=6) {
        let env = Env::default();
        env.mock_all_auths();
        let signers = signer_vec(&env, 2 + extra_signers);
        let alice = signers.get(0).unwrap();
        let bob = signers.get(1).unwrap();
        let contract_address = env.register_contract(None, MultisigContract);
        let client = MultisigContractClient::new(&env, &contract_address);
        client.initialize(&signers, &2, &None);

        let new_signer = Address::generate(&env);
        let duplicate_approvals = vec![&env, alice.clone(), alice.clone()];
        let result = client.try_add_signer(&duplicate_approvals, &new_signer, &1, &2);

        prop_assert!(result.is_err());
        prop_assert!(!client.is_signer(&new_signer));

        let valid_approvals = vec![&env, alice, bob];
        let result = client.try_add_signer(&valid_approvals, &new_signer, &1, &2);
        prop_assert!(result.is_ok());
        prop_assert!(client.is_signer(&new_signer));
    }
}
