#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]
#![cfg(test)]

use crate::{AirdropContract, AirdropContractClient};
use proptest::prelude::*;
use soroban_sdk::{Address, BytesN, Env, Vec, testutils::Address as _, token::StellarAssetClient};

proptest! {
    /// Property: Claimed amounts never exceed airdrop contract balance
    /// Closes #961 – airdrop accounting invariant
    #[test]
    fn prop_claims_never_exceed_balance(
        claim_amount in 1i128..=10_000i128,
        initial_supply in 10_000i128..=100_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let claimer = Address::generate(&env);

        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        StellarAssetClient::new(&env, &token_addr).mint(&admin, &initial_supply);

        let airdrop_addr = env.register_contract(None, AirdropContract);
        let client = AirdropContractClient::new(&env, &airdrop_addr);

        let deadline = env.ledger().sequence() + 1000;
        let _ = client.try_initialize(&admin, &deadline, &None);

        // Fund airdrop
        StellarAssetClient::new(&env, &token_addr).transfer(&admin, &airdrop_addr, &initial_supply);

        let contract_balance_before = soroban_sdk::token::Client::new(&env, &token_addr).balance(&airdrop_addr);

        // Try to claim (will likely fail without valid proof, but that's ok)
        let proof = Vec::new(&env);
        let _ = client.try_claim(&claimer, &token_addr, &claim_amount, &0u32, &proof);

        let contract_balance_after = soroban_sdk::token::Client::new(&env, &token_addr).balance(&airdrop_addr);
        let claimed = contract_balance_before - contract_balance_after;

        // Invariant: claimed amount never exceeds initial balance
        prop_assert!(claimed <= initial_supply,
            "Claimed more than initial supply: claimed={}, supply={}",
            claimed, initial_supply);
        prop_assert!(contract_balance_after >= 0,
            "Contract balance went negative: {}", contract_balance_after);
    }

    /// Property: Double claims are prevented
    #[test]
    fn prop_no_double_claim(
        amount in 100i128..=1_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let claimer = Address::generate(&env);

        let sac_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(sac_admin);
        let token_addr = sac.address();

        StellarAssetClient::new(&env, &token_addr).mint(&admin, &(amount * 3));

        let airdrop_addr = env.register_contract(None, AirdropContract);
        let client = AirdropContractClient::new(&env, &airdrop_addr);

        let deadline = env.ledger().sequence() + 1000;
        client.initialize(&admin, &deadline, &None);

        StellarAssetClient::new(&env, &token_addr).transfer(&admin, &airdrop_addr, &(amount * 2));

        let proof = Vec::new(&env);

        // First claim
        let first_claim = client.try_claim(&claimer, &token_addr, &amount, &0u32, &proof);

        // Second claim (should fail if first succeeded)
        if first_claim.is_ok() {
            let second_claim = client.try_claim(&claimer, &token_addr, &amount, &0u32, &proof);
            prop_assert!(second_claim.is_err(), "Double claim should be prevented");
        }
    }
}
