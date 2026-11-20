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

    #[test]
    fn prop_duplicate_signatures_never_satisfy_threshold(
        signer_count in 2u32..=8,
        threshold in 2u32..=8,
        duplicate_count in 2u32..=8,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let signers = signer_vec(&env, signer_count);
        let alice = signers.get(0).unwrap();
        let contract_address = env.register_contract(None, MultisigContract);
        let client = MultisigContractClient::new(&env, &contract_address);
        client.initialize(&signers, &threshold, &None);

        // A single signer repeated many times must never reach the threshold.
        let mut duplicates = soroban_sdk::Vec::new(&env);
        for _ in 0..duplicate_count {
            duplicates.push_back(alice.clone());
        }
        let new_signer = Address::generate(&env);
        let result = client.try_add_signer(&duplicates, &new_signer, &1, &threshold);
        prop_assert!(result.is_err());
        prop_assert!(!client.is_signer(&new_signer));

        // The same signer repeated exactly `threshold` times must still fail.
        let mut threshold_duplicates = soroban_sdk::Vec::new(&env);
        for _ in 0..threshold {
            threshold_duplicates.push_back(alice.clone());
        }
        let result = client.try_add_signer(&threshold_duplicates, &new_signer, &1, &threshold);
        prop_assert!(result.is_err());
        prop_assert!(!client.is_signer(&new_signer));
    }

    #[test]
    fn prop_expired_proposals_never_execute(
        signer_count in 2u32..=8,
        threshold in 1u32..=8,
        expiry_offset in 0u32..=100,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let signers = signer_vec(&env, signer_count);
        let contract_address = env.register_contract(None, MultisigContract);
        let client = MultisigContractClient::new(&env, &contract_address);
        client.initialize(&signers, &threshold, &None);

        let new_signer = Address::generate(&env);
        let mut approvals = soroban_sdk::Vec::new(&env);
        for i in 0..threshold.min(signer_count) {
            approvals.push_back(signers.get(i).unwrap());
        }

        let expiry = env.ledger().sequence() + expiry_offset;
        let result = client.try_add_signer(&approvals, &new_signer, &1, &expiry);
        if expiry_offset == 0 {
            // Expiry at or before the current ledger sequence must not execute.
            prop_assert!(result.is_err());
            prop_assert!(!client.is_signer(&new_signer));
        } else {
            // A future expiry may succeed; if it does, the signer is added.
            if result.is_ok() {
                prop_assert!(client.is_signer(&new_signer));
            } else {
                prop_assert!(!client.is_signer(&new_signer));
            }
        }
    }
}
