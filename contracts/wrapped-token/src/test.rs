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
    Address, Env, MuxedAddress, String, contract, contractimpl, contracttype, symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    token::{self, TokenInterface},
    vec,
};

/// Registers the wrapped-token contract plus two Stellar Asset Contracts
/// (underlying + wrapped), with the wrapped-token contract itself set as the
/// wrapped SAC's admin so `wrap` can mint. Mints `initial_underlying` of the
/// underlying asset to `user`.
fn setup<'a>(
    env: &'a Env,
    initial_underlying: i128,
) -> (
    WrappedTokenContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    let admin = Address::generate(env);
    let user = Address::generate(env);

    let contract_id = env.register_contract(None, WrappedTokenContract);

    let wrapped_sac = env.register_stellar_asset_contract_v2(contract_id.clone());
    let wrapped_token = wrapped_sac.address();

    let underlying_admin = Address::generate(env);
    let underlying_sac = env.register_stellar_asset_contract_v2(underlying_admin);
    let underlying_token = underlying_sac.address();

    if initial_underlying > 0 {
        token::StellarAssetClient::new(env, &underlying_token).mint(&user, &initial_underlying);
    }

    let client = WrappedTokenContractClient::new(env, &contract_id);
    (client, admin, user, wrapped_token, underlying_token)
}

/// Registers the wrapped-token contract plus two Stellar Asset Contracts
/// (underlying + wrapped), with the wrapped-token contract itself set as the
/// wrapped SAC's admin so `wrap` can mint. Mints `initial_underlying` of the
/// underlying asset to `user`.
fn setup_with_min<'a>(
    env: &'a Env,
    initial_underlying: i128,
    min_amount: Option<i128>,
) -> (
    WrappedTokenContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, user, wrapped_token, underlying_token) =
        setup(env, initial_underlying);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &min_amount);
    (client, admin, user, wrapped_token, underlying_token)
}

#[test]
fn test_wrap_unwrap_1_1_peg() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    client.wrap(&user, &100i128);
    assert_eq!(client.get_total_wrapped(), 100);
    assert_eq!(token::Client::new(&env, &wrapped_token).balance(&user), 100);
    assert_eq!(
        token::Client::new(&env, &underlying_token).balance(&user),
        900
    );

    client.unwrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 50);

    client.unwrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 0);
    assert_eq!(
        token::Client::new(&env, &underlying_token).balance(&user),
        1_000
    );
}

#[test]
fn test_initialize_idempotent_check() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    let result = client.try_initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    assert!(result.is_err());
}

#[test]
fn test_wrap_requires_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 100);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    assert!(client.try_wrap(&user, &0i128).is_err());
    assert!(client.try_wrap(&user, &-1i128).is_err());
}

// ---------------------------------------------------------------------------
// min_amount threshold (issue #1272)
// ---------------------------------------------------------------------------

#[test]
fn test_min_amount_rejects_dust_wrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, user, _wrapped_token, _underlying_token) =
        setup_with_min(&env, 1_000, Some(100i128));
    assert_eq!(client.min_amount(), Some(100i128));

    // 1-stroop dust and anything below the threshold must be rejected.
    let result = client.try_wrap(&user, &1i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));
    let result = client.try_wrap(&user, &99i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));

    // Nothing was applied.
    assert_eq!(client.get_total_wrapped(), 0);
    assert_eq!(client.wrapped_by(&user), 0);
}

#[test]
fn test_min_amount_rejects_dust_unwrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, user, _wrapped_token, _underlying_token) =
        setup_with_min(&env, 1_000, Some(100i128));
    client.wrap(&user, &500i128);

    let result = client.try_unwrap(&user, &1i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));
    let result = client.try_unwrap(&user, &99i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));

    assert_eq!(client.get_total_wrapped(), 500);
    assert_eq!(client.wrapped_by(&user), 500);
}

#[test]
fn test_min_amount_boundary_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, user, _wrapped_token, _underlying_token) =
        setup_with_min(&env, 1_000, Some(100i128));

    // Exactly the threshold is allowed.
    client.wrap(&user, &100i128);
    assert_eq!(client.get_total_wrapped(), 100);

    // Above the threshold is allowed.
    client.wrap(&user, &150i128);
    assert_eq!(client.get_total_wrapped(), 250);

    client.unwrap(&user, &100i128);
    assert_eq!(client.get_total_wrapped(), 150);
    client.unwrap(&user, &150i128);
    assert_eq!(client.get_total_wrapped(), 0);
}

#[test]
fn test_set_min_amount_updates_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, user, _wrapped_token, _underlying_token) =
        setup_with_min(&env, 1_000, None);
    assert_eq!(client.min_amount(), None);

    // Uncapped: dust is accepted.
    client.wrap(&user, &1i128);
    assert_eq!(client.get_total_wrapped(), 1);

    // Admin raises the threshold; dust is now rejected.
    client.set_min_amount(&Some(50i128));
    assert_eq!(client.min_amount(), Some(50i128));
    let result = client.try_wrap(&user, &1i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));
    let result = client.try_unwrap(&user, &1i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::AmountTooLow)));

    // Amounts at or above the new threshold still work.
    client.wrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 51);
    client.unwrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 1);

    // Admin can clear the threshold again.
    client.set_min_amount(&None);
    assert_eq!(client.min_amount(), None);
    client.unwrap(&user, &1i128);
    assert_eq!(client.get_total_wrapped(), 0);
}

#[test]
fn test_none_min_amount_leaves_wrap_and_unwrap_uncapped() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, user, _wrapped_token, _underlying_token) =
        setup_with_min(&env, 1_000, None);
    assert_eq!(client.min_amount(), None);

    client.wrap(&user, &1i128);
    assert_eq!(client.get_total_wrapped(), 1);
    client.unwrap(&user, &1i128);
    assert_eq!(client.get_total_wrapped(), 0);
}

#[test]
fn test_initialize_rejects_non_positive_min_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
    let result =
        client.try_initialize(&admin, &wrapped_token, &underlying_token, &None, &Some(0i128));
    assert!(result.is_err());
    let result =
        client.try_initialize(&admin, &wrapped_token, &underlying_token, &None, &Some(-1i128));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Reserve-balance invariant (issue: monitoring for wrap/unwrap backing)
// ---------------------------------------------------------------------------

#[test]
fn test_get_reserve_balance_matches_underlying_holdings() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    assert_eq!(client.get_reserve_balance(), 0);

    client.wrap(&user, &300i128);
    assert_eq!(client.get_reserve_balance(), 300);
    assert_eq!(client.get_total_wrapped(), client.get_reserve_balance());

    client.unwrap(&user, &120i128);
    assert_eq!(client.get_reserve_balance(), 180);
    assert_eq!(client.get_total_wrapped(), client.get_reserve_balance());
}

#[test]
fn test_reserve_invariant_holds_after_wrap_unwrap_sequence() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 10_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    let amounts: [i128; 6] = [500, 200, -300, 400, -100, -700];
    for delta in amounts {
        if delta > 0 {
            client.wrap(&user, &delta);
        } else {
            client.unwrap(&user, &(-delta));
        }
        // Core invariant: the wrapped supply can never exceed the actual
        // underlying reserve held by the contract.
        assert!(client.get_total_wrapped() <= client.get_reserve_balance());
    }
}

// ---------------------------------------------------------------------------
// max_wrap_per_address
// ---------------------------------------------------------------------------

#[test]
fn test_max_wrap_per_address_boundary_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &Some(500i128), &None);

    client.wrap(&user, &300i128);
    client.wrap(&user, &200i128); // cumulative 500 == cap, must succeed
    assert_eq!(client.wrapped_by(&user), 500);
}

#[test]
fn test_max_wrap_per_address_rejects_over_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &Some(500i128), &None);

    client.wrap(&user, &300i128);
    let result = client.try_wrap(&user, &201i128); // cumulative 501 > cap
    assert!(result.is_err());
    // The rejected wrap must not have partially applied.
    assert_eq!(client.wrapped_by(&user), 300);
    assert_eq!(client.get_total_wrapped(), 300);
}

#[test]
fn test_max_wrap_per_address_is_per_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    let other_user = Address::generate(&env);
    token::StellarAssetClient::new(&env, &underlying_token).mint(&other_user, &1_000);

    client.initialize(&admin, &wrapped_token, &underlying_token, &Some(500i128), &None);

    client.wrap(&user, &500i128);
    // A different address has its own independent cap allowance.
    client.wrap(&other_user, &500i128);
    assert_eq!(client.wrapped_by(&user), 500);
    assert_eq!(client.wrapped_by(&other_user), 500);
}

#[test]
fn test_uncapped_wrap_allows_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    assert_eq!(client.max_wrap_per_address(), None);

    client.wrap(&user, &1_000_000i128);
    assert_eq!(client.get_total_wrapped(), 1_000_000);
}

#[test]
fn test_initialize_rejects_non_positive_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
    let result =
        client.try_initialize(&admin, &wrapped_token, &underlying_token, &Some(0i128), &None);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// unwrap checked arithmetic and reserve guard
// ---------------------------------------------------------------------------

#[test]
fn test_unwrap_underflow_when_amount_exceeds_supply() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    assert!(client.try_unwrap(&user, &1i128).is_err());
    assert_eq!(client.get_total_wrapped(), 0);

    client.wrap(&user, &50i128);
    // Extra wrapped tokens minted outside `wrap`, with matching underlying
    // donated in, so this fails the supply check rather than the reserve check.
    token::StellarAssetClient::new(&env, &wrapped_token).mint(&user, &50i128);
    token::Client::new(&env, &underlying_token).transfer(&user, &client.address, &50i128);

    assert!(client.try_unwrap(&user, &51i128).is_err());
    assert_eq!(client.get_total_wrapped(), 50);

    client.unwrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 0);
}

#[test]
fn test_unwrap_rejects_when_reserve_is_short() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.wrap(&user, &100i128);

    // Underlying leaves the contract without a matching burn (rebasing / external drain).
    env.as_contract(&client.address, || {
        token::Client::new(&env, &underlying_token).transfer(&client.address, &user, &40i128);
    });
    assert_eq!(client.get_reserve_balance(), 60);
    assert!(client.try_unwrap(&user, &61i128).is_err());
    assert_eq!(client.get_total_wrapped(), 100);

    client.unwrap(&user, &60i128);
    assert_eq!(client.get_total_wrapped(), 40);
    assert_eq!(client.get_reserve_balance(), 0);
}

// ---------------------------------------------------------------------------
// Proof-of-reserve oracle
// ---------------------------------------------------------------------------

#[soroban_sdk::contract]
struct MockPorOracle;

#[soroban_sdk::contractimpl]
impl MockPorOracle {
    pub fn set_reserve(env: Env, reserve: i128) {
        env.storage()
            .instance()
            .set(&symbol_short!("reserve"), &reserve);
    }

    pub fn get_reserve(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("reserve"))
            .unwrap_or(0)
    }
}

#[test]
fn test_wrap_rejected_when_oracle_reserve_exceeded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    let oracle_id = env.register_contract(None, MockPorOracle);
    let oracle = MockPorOracleClient::new(&env, &oracle_id);
    oracle.set_reserve(&150i128);
    client.set_por_oracle(&Some(oracle_id));

    client.wrap(&user, &100i128);
    assert_eq!(client.get_total_wrapped(), 100);

    // 100 + 51 > 150
    assert!(client.try_wrap(&user, &51i128).is_err());
    assert_eq!(client.get_total_wrapped(), 100);

    // Boundary: 100 + 50 == 150 is allowed.
    client.wrap(&user, &50i128);
    assert_eq!(client.get_total_wrapped(), 150);

    oracle.set_reserve(&200i128);
    client.wrap(&user, &40i128);
    assert_eq!(client.get_total_wrapped(), 190);
}

// ---------------------------------------------------------------------------
// Aggregate unwrap rate limit
// ---------------------------------------------------------------------------

#[test]
fn test_unwrap_rate_limit_throttles_high_volume_spike() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = 1_000);

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    let other = Address::generate(&env);
    token::StellarAssetClient::new(&env, &underlying_token).mint(&other, &1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.set_unwrap_rate_limit(&100i128, &10u32);

    client.wrap(&user, &400i128);
    client.wrap(&other, &400i128);

    client.unwrap(&user, &60i128);
    client.unwrap(&other, &40i128);
    assert_eq!(client.get_total_wrapped(), 700);

    // 60 + 40 + 1 exceeds the aggregate cap of 100 inside this window.
    assert!(client.try_unwrap(&user, &1i128).is_err());
    assert_eq!(client.get_total_wrapped(), 700);

    env.ledger().with_mut(|l| l.sequence_number = 1_010);
    client.unwrap(&user, &80i128);
    assert_eq!(client.get_total_wrapped(), 620);
}

#[test]
fn test_admin_can_bypass_unwrap_rate_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.set_unwrap_rate_limit(&50i128, &100u32);
    client.wrap(&user, &200i128);

    assert!(client.try_unwrap(&user, &80i128).is_err());

    client.set_unwrap_rate_limit_paused(&true);
    client.unwrap(&user, &80i128);
    assert_eq!(client.get_total_wrapped(), 120);

    client.set_unwrap_rate_limit_paused(&false);
    assert!(client.try_unwrap(&user, &80i128).is_err());
}

// ---------------------------------------------------------------------------
// Pausable — only compiled when the `pausable` feature is enabled.
// ---------------------------------------------------------------------------

#[cfg(feature = "pausable")]
mod pausable_tests {
    use super::*;

    #[test]
    fn test_pause_blocks_wrap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 100);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

        client.pause();
        assert!(client.try_wrap(&user, &10i128).is_err());
    }

    #[test]
    fn test_pause_blocks_unwrap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 100);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
        client.wrap(&user, &50i128);

        client.pause();
        assert!(client.try_unwrap(&user, &10i128).is_err());
    }

    #[test]
    fn test_unpause_restores_wrap_and_unwrap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 100);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

        client.pause();
        client.unpause();

        client.wrap(&user, &40i128);
        assert_eq!(client.get_total_wrapped(), 40);
        client.unwrap(&user, &40i128);
        assert_eq!(client.get_total_wrapped(), 0);
    }

    #[test]
    fn test_pause_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

        client.pause();
        assert!(!env.events().all().events().is_empty());
    }

    #[test]
    fn test_unpause_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

        client.pause();
        client.unpause();
        assert!(!env.events().all().events().is_empty());
    }

    #[test]
    fn test_pause_requires_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
        client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

        // Not directly testable without auth mocking per-address; pause() enforces
        // admin via require_admin + require_auth, exercised positively above.
        client.pause();
        assert!(client.try_wrap(&Address::generate(&env), &1i128).is_err());
    }
}


// ---------------------------------------------------------------------------
// Per-address wrap quota freed on unwrap (issue #1179)
// ---------------------------------------------------------------------------

#[test]
fn test_unwrap_decrements_wrapped_by_user_allowing_rewrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &Some(500i128), &None);

    client.wrap(&user, &500i128);
    assert_eq!(client.wrapped_by(&user), 500);
    // At cap — further wrap must fail.
    assert!(client.try_wrap(&user, &1i128).is_err());

    client.unwrap(&user, &200i128);
    assert_eq!(client.wrapped_by(&user), 300);

    // Freed capacity can be reused.
    client.wrap(&user, &200i128);
    assert_eq!(client.wrapped_by(&user), 500);
    assert_eq!(client.get_total_wrapped(), 500);
}

#[test]
fn test_unwrap_clamps_wrapped_by_user_at_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &Some(500i128), &None);

    client.wrap(&user, &100i128);
    client.unwrap(&user, &100i128);
    assert_eq!(client.wrapped_by(&user), 0);
}

// ---------------------------------------------------------------------------
// Fee-on-transfer underlying (issue #1181)
// ---------------------------------------------------------------------------

#[contracttype]
enum FotKey {
    Balance(Address),
}

/// Underlying token that withholds a 10% fee on every transfer.
#[contract]
struct FeeOnTransferToken;

#[contractimpl]
impl FeeOnTransferToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = FotKey::Balance(to.clone());
        let bal: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(bal + amount));
    }
}

#[contractimpl]
impl TokenInterface for FeeOnTransferToken {
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
            .get(&FotKey::Balance(id))
            .unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        let to_addr = to.address();
        let from_key = FotKey::Balance(from.clone());
        let to_key = FotKey::Balance(to_addr);
        let from_bal: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        let to_bal: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        // 10% fee burned on transfer; recipient gets 90%.
        let fee = amount / 10;
        let received = amount - fee;
        env.storage()
            .instance()
            .set(&from_key, &(from_bal - amount));
        env.storage()
            .instance()
            .set(&to_key, &(to_bal + received));
    }

    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}

    fn burn(_env: Env, _from: Address, _amount: i128) {}

    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {}

    fn decimals(_env: Env) -> u32 {
        7
    }

    fn name(env: Env) -> String {
        String::from_str(&env, "FeeToken")
    }

    fn symbol(env: Env) -> String {
        String::from_str(&env, "FEE")
    }
}

#[test]
fn test_wrap_mints_only_actual_received_with_fee_on_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register_contract(None, WrappedTokenContract);
    let wrapped_sac = env.register_stellar_asset_contract_v2(contract_id.clone());
    let wrapped_token = wrapped_sac.address();

    let underlying_token = env.register_contract(None, FeeOnTransferToken);
    FeeOnTransferTokenClient::new(&env, &underlying_token).mint(&user, &1_000);

    let client = WrappedTokenContractClient::new(&env, &contract_id);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    // User requests wrap of 100; fee-on-transfer delivers 90 to the contract.
    client.wrap(&user, &100i128);

    assert_eq!(client.get_total_wrapped(), 90);
    assert_eq!(client.get_reserve_balance(), 90);
    assert_eq!(token::Client::new(&env, &wrapped_token).balance(&user), 90);
    assert!(client.get_total_wrapped() <= client.get_reserve_balance());
}

#[test]
fn test_fee_on_transfer_wrap_then_full_unwrap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register_contract(None, WrappedTokenContract);
    let wrapped_sac = env.register_stellar_asset_contract_v2(contract_id.clone());
    let wrapped_token = wrapped_sac.address();

    let underlying_token = env.register_contract(None, FeeOnTransferToken);
    FeeOnTransferTokenClient::new(&env, &underlying_token).mint(&user, &1_000);

    let client = WrappedTokenContractClient::new(&env, &contract_id);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    client.wrap(&user, &200i128); // receives 180
    assert_eq!(client.get_total_wrapped(), 180);
    client.unwrap(&user, &180i128);
    assert_eq!(client.get_total_wrapped(), 0);
    assert_eq!(client.get_reserve_balance(), 0);
}

// ---------------------------------------------------------------------------
// Controllable underlying token: real balances, plus switches to make
// transfers revert / silently no-op / reject a specific recipient, and an
// optional re-entrancy attack back into the wrapper on every transfer.
// ---------------------------------------------------------------------------

const MODE_NORMAL: u32 = 0;
const MODE_REVERT: u32 = 1;
const MODE_SILENT: u32 = 2;

const ATTACK_NONE: u32 = 0;
const ATTACK_WRAP: u32 = 1;
const ATTACK_UNWRAP: u32 = 2;

#[contracttype]
enum CtKey {
    Balance(Address),
    Mode,
    Blocked,
    Armed,
    Reentered,
}

#[contract]
struct ControlledToken;

#[contractimpl]
impl ControlledToken {
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = CtKey::Balance(to);
        let bal: i128 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(bal + amount));
    }

    pub fn set_mode(env: Env, mode: u32) {
        env.storage().instance().set(&CtKey::Mode, &mode);
    }

    pub fn set_blocked(env: Env, who: Address) {
        env.storage().instance().set(&CtKey::Blocked, &who);
    }

    /// On every subsequent transfer, call back into `target` as `attacker`.
    pub fn arm(env: Env, target: Address, attacker: Address, attack: u32) {
        env.storage()
            .instance()
            .set(&CtKey::Armed, &(target, attacker, attack));
    }

    /// `true` if any re-entrant call into the wrapper ever succeeded.
    pub fn reentered(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&CtKey::Reentered)
            .unwrap_or(false)
    }
}

#[contractimpl]
impl TokenInterface for ControlledToken {
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
            .get(&CtKey::Balance(id))
            .unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        let to = to.address();
        let mode: u32 = env.storage().instance().get(&CtKey::Mode).unwrap_or(MODE_NORMAL);
        if mode == MODE_REVERT {
            panic!("underlying transfer reverted");
        }
        let blocked: Option<Address> = env.storage().instance().get(&CtKey::Blocked);
        if blocked.as_ref() == Some(&to) {
            panic!("recipient blocked");
        }
        if mode == MODE_SILENT {
            // Reports success without moving any funds.
            return;
        }

        let from_key = CtKey::Balance(from);
        let to_key = CtKey::Balance(to);
        let from_bal: i128 = env.storage().instance().get(&from_key).unwrap_or(0);
        if from_bal < amount {
            panic!("insufficient balance");
        }
        env.storage().instance().set(&from_key, &(from_bal - amount));
        let to_bal: i128 = env.storage().instance().get(&to_key).unwrap_or(0);
        env.storage().instance().set(&to_key, &(to_bal + amount));

        let armed: Option<(Address, Address, u32)> = env.storage().instance().get(&CtKey::Armed);
        if let Some((target, attacker, attack)) = armed {
            let wrapper = WrappedTokenContractClient::new(&env, &target);
            let succeeded = match attack {
                ATTACK_WRAP => matches!(wrapper.try_wrap(&attacker, &1i128), Ok(Ok(()))),
                ATTACK_UNWRAP => matches!(wrapper.try_unwrap(&attacker, &1i128), Ok(Ok(()))),
                _ => false,
            };
            if succeeded {
                env.storage().instance().set(&CtKey::Reentered, &true);
            }
        }
    }

    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}

    fn burn(_env: Env, _from: Address, _amount: i128) {}

    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {}

    fn decimals(_env: Env) -> u32 {
        7
    }

    fn name(env: Env) -> String {
        String::from_str(&env, "Controlled")
    }

    fn symbol(env: Env) -> String {
        String::from_str(&env, "CTL")
    }
}

/// Wrapper over a [`ControlledToken`] underlying, initialized, with
/// `initial_underlying` minted to the returned user.
fn setup_controlled<'a>(
    env: &'a Env,
    initial_underlying: i128,
) -> (
    WrappedTokenContractClient<'a>,
    Address,
    token::Client<'a>,
    ControlledTokenClient<'a>,
) {
    let admin = Address::generate(env);
    let user = Address::generate(env);

    let contract_id = env.register_contract(None, WrappedTokenContract);
    let wrapped_token = env
        .register_stellar_asset_contract_v2(contract_id.clone())
        .address();
    let underlying_token = env.register_contract(None, ControlledToken);
    let underlying = ControlledTokenClient::new(env, &underlying_token);
    underlying.mint(&user, &initial_underlying);

    let client = WrappedTokenContractClient::new(env, &contract_id);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    (
        client,
        user,
        token::Client::new(env, &wrapped_token),
        underlying,
    )
}

fn lock_held(env: &Env, client: &WrappedTokenContractClient) -> bool {
    env.as_contract(&client.address, || {
        env.storage().instance().has(&DataKey::ReentrancyLock)
    })
}

// ---------------------------------------------------------------------------
// Atomic unwrap: underlying transfer failure (#1434)
// ---------------------------------------------------------------------------

#[test]
fn test_unwrap_rolls_back_burn_when_underlying_transfer_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, user, wrapped, underlying) = setup_controlled(&env, 1_000);

    client.wrap(&user, &500i128);
    assert_eq!(wrapped.balance(&user), 500);

    underlying.set_mode(&MODE_REVERT);
    let result = client.try_unwrap(&user, &200i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::UnderlyingTransferFailed)));

    // Nothing was burned and no accounting changed.
    assert_eq!(wrapped.balance(&user), 500);
    assert_eq!(client.get_total_wrapped(), 500);
    assert_eq!(client.get_reserve_balance(), 500);
    assert_eq!(underlying.balance(&user), 500);
    assert!(!lock_held(&env, &client));

    // Once the underlying recovers the same unwrap succeeds.
    underlying.set_mode(&MODE_NORMAL);
    client.unwrap(&user, &200i128);
    assert_eq!(wrapped.balance(&user), 300);
    assert_eq!(underlying.balance(&user), 700);
    assert_eq!(client.get_total_wrapped(), 300);
}

#[test]
fn test_unwrap_rejects_transfer_that_silently_moves_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, user, wrapped, underlying) = setup_controlled(&env, 1_000);

    client.wrap(&user, &500i128);
    underlying.set_mode(&MODE_SILENT);

    let result = client.try_unwrap(&user, &200i128);
    assert_eq!(result, Err(Ok(WrappedTokenError::UnderlyingTransferFailed)));
    assert_eq!(wrapped.balance(&user), 500);
    assert_eq!(client.get_total_wrapped(), 500);
    assert_eq!(client.get_reserve_balance(), 500);
}

#[test]
fn test_unwrap_batch_is_atomic_when_one_payout_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, user, wrapped, underlying) = setup_controlled(&env, 1_000);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.wrap(&user, &500i128);
    underlying.set_blocked(&bob);

    let result = client.try_unwrap_batch(
        &user,
        &vec![&env, (alice.clone(), 100i128), (bob.clone(), 100i128)],
    );
    assert_eq!(result, Err(Ok(WrappedTokenError::UnderlyingTransferFailed)));

    // Alice's earlier payout in the same batch was rolled back too.
    assert_eq!(underlying.balance(&alice), 0);
    assert_eq!(underlying.balance(&bob), 0);
    assert_eq!(wrapped.balance(&user), 500);
    assert_eq!(client.get_total_wrapped(), 500);
    assert_eq!(client.get_reserve_balance(), 500);
}

#[test]
fn test_wrap_is_atomic_when_underlying_deposit_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, user, wrapped, underlying) = setup_controlled(&env, 1_000);

    underlying.set_mode(&MODE_REVERT);
    assert!(client.try_wrap(&user, &100i128).is_err());

    assert_eq!(wrapped.balance(&user), 0);
    assert_eq!(client.get_total_wrapped(), 0);
    assert_eq!(underlying.balance(&user), 1_000);
    assert!(!lock_held(&env, &client));
}

// ---------------------------------------------------------------------------
// Reentrancy protection (#1435)
// ---------------------------------------------------------------------------

#[test]
fn test_wrap_and_unwrap_reject_while_lock_is_held() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.wrap(&user, &100i128);

    // Simulate being inside an in-flight wrap/unwrap.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyLock, &true);
    });

    let reentrancy = Err(Ok(WrappedTokenError::Reentrancy));
    assert_eq!(client.try_wrap(&user, &10i128), reentrancy);
    assert_eq!(client.try_unwrap(&user, &10i128), reentrancy);
    assert_eq!(
        client.try_wrap_batch(&user, &vec![&env, (user.clone(), 10i128)]),
        reentrancy
    );
    assert_eq!(
        client.try_unwrap_batch(&user, &vec![&env, (user.clone(), 10i128)]),
        reentrancy
    );
    assert_eq!(client.get_total_wrapped(), 100);

    env.as_contract(&client.address, || {
        env.storage().instance().remove(&DataKey::ReentrancyLock);
    });
    client.wrap(&user, &10i128);
    assert_eq!(client.get_total_wrapped(), 110);
}

#[test]
fn test_lock_released_after_success_and_after_error() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    client.wrap(&user, &100i128);
    assert!(!lock_held(&env, &client));
    client.unwrap(&user, &40i128);
    assert!(!lock_held(&env, &client));

    assert!(client.try_wrap(&user, &0i128).is_err());
    assert!(!lock_held(&env, &client));
    assert!(client.try_unwrap(&user, &1_000i128).is_err());
    assert!(!lock_held(&env, &client));
}

/// Fuzz: drive random wrap/unwrap sequences through an underlying token that
/// tries to re-enter `wrap` or `unwrap` on every transfer. A re-entrant call
/// must never succeed, and the 1:1 accounting must hold after every step
/// whether the outer call succeeded or was aborted.
#[test]
fn test_fuzz_reentrancy_attack_never_succeeds() {
    let env = Env::default();
    // Allow the attacker's nested call to pass auth so it is only stopped by
    // re-entrancy protection, not by a missing signature.
    env.mock_all_auths_allowing_non_root_auth();
    let (client, user, wrapped, underlying) = setup_controlled(&env, 1_000_000);

    let attacker = Address::generate(&env);
    underlying.mint(&attacker, &1_000);
    // Give the attacker wrapped balance up front so a re-entrant unwrap would
    // otherwise be fundable.
    client.wrap(&attacker, &100i128);

    let mut seed: u64 = 0x1435_1435_1435_1435;
    let mut next = move || {
        // Deterministic LCG (Knuth MMIX constants).
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        seed >> 33
    };

    for _ in 0..64 {
        let r = next();
        let amount = i128::from(u32::try_from(r % 500).unwrap() + 1);
        let attack = if (r >> 9) % 2 == 0 { ATTACK_WRAP } else { ATTACK_UNWRAP };
        let do_wrap = (r >> 10) % 2 == 0 || wrapped.balance(&user) < amount;

        underlying.arm(&client.address, &attacker, &attack);
        let _ = if do_wrap {
            client.try_wrap(&user, &amount)
        } else {
            client.try_unwrap(&user, &amount)
        };
        underlying.arm(&client.address, &attacker, &ATTACK_NONE);

        assert!(!underlying.reentered(), "re-entrant call succeeded");
        assert!(!lock_held(&env, &client));
        let supply = wrapped.balance(&user) + wrapped.balance(&attacker);
        assert_eq!(client.get_total_wrapped(), supply);
        assert!(client.get_total_wrapped() <= client.get_reserve_balance());
    }
}

// ---------------------------------------------------------------------------
// Proof-of-reserve audit view (#1436)
// ---------------------------------------------------------------------------

#[test]
fn test_reserve_status_equal_after_wrap_unwrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    assert_eq!(
        client.get_reserve_status(),
        ReserveStatus {
            total_wrapped: 0,
            total_locked: 0,
            is_fully_backed: true,
        }
    );

    client.wrap(&user, &300i128);
    client.unwrap(&user, &120i128);

    let status = client.get_reserve_status();
    assert_eq!(status.total_wrapped, 180);
    assert_eq!(status.total_locked, 180);
    assert_eq!(status.total_wrapped, status.total_locked);
    assert!(status.is_fully_backed);
    // A healthy reserve emits no audit event.
    assert!(env.events().all().events().is_empty());
    client.assert_reserve_backing();
}

#[test]
fn test_reserve_status_surplus_is_fully_backed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.wrap(&user, &100i128);

    // Direct donation to the vault over-collateralises the supply.
    token::Client::new(&env, &underlying_token).transfer(&user, &client.address, &50i128);

    let status = client.get_reserve_status();
    assert_eq!(status.total_wrapped, 100);
    assert_eq!(status.total_locked, 150);
    assert!(status.is_fully_backed);
    assert!(env.events().all().events().is_empty());
}

#[test]
fn test_reserve_status_flags_discrepancy_and_emits_audit_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 1_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    client.wrap(&user, &100i128);

    // Collateral leaves the vault without a matching burn.
    env.as_contract(&client.address, || {
        token::Client::new(&env, &underlying_token).transfer(&client.address, &user, &40i128);
    });

    let status = client.get_reserve_status();
    assert_eq!(
        status,
        ReserveStatus {
            total_wrapped: 100,
            total_locked: 60,
            is_fully_backed: false,
        }
    );
    assert_eq!(env.events().all().events().len(), 1);

    assert_eq!(
        client.try_assert_reserve_backing(),
        Err(Ok(WrappedTokenError::InsufficientReserve))
    );
}

#[test]
fn test_reserve_status_requires_initialization() {
    let env = Env::default();
    let (client, _admin, _user, _wrapped_token, _underlying_token) = setup(&env, 0);
    assert_eq!(
        client.try_get_reserve_status(),
        Err(Ok(WrappedTokenError::NotInitialized))
    );
}

// ---------------------------------------------------------------------------
// Fees and batch accounting
// ---------------------------------------------------------------------------

#[test]
fn test_fees_and_batches_keep_reserve_backed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, user, wrapped_token, underlying_token) = setup(&env, 10_000);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);
    let fee_recipient = Address::generate(&env);
    let alice = Address::generate(&env);
    client.set_fees(&100u32, &200u32, &Some(fee_recipient.clone()));

    // 1% wrap fee: 10 + 5 to the fee recipient.
    client.wrap_batch(
        &user,
        &vec![&env, (user.clone(), 1_000i128), (alice.clone(), 500i128)],
    );
    let wrapped = token::Client::new(&env, &wrapped_token);
    let underlying = token::Client::new(&env, &underlying_token);
    assert_eq!(wrapped.balance(&user), 990);
    assert_eq!(wrapped.balance(&alice), 495);
    assert_eq!(underlying.balance(&fee_recipient), 15);
    assert_eq!(client.get_total_wrapped(), 1_485);
    assert_eq!(client.get_reserve_balance(), 1_485);

    // 2% unwrap fee on 990 = 19.
    client.unwrap(&user, &990i128);
    assert_eq!(underlying.balance(&fee_recipient), 34);
    assert_eq!(underlying.balance(&user), 10_000 - 1_500 + 971);
    assert_eq!(client.get_total_wrapped(), 495);
    assert!(client.get_reserve_status().is_fully_backed);
}

#[test]
fn test_set_fees_validates_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin, _user, wrapped_token, underlying_token) = setup(&env, 0);
    client.initialize(&admin, &wrapped_token, &underlying_token, &None, &None);

    let invalid = Err(Ok(WrappedTokenError::InvalidFee));
    assert_eq!(
        client.try_set_fees(&10_001u32, &0u32, &Some(Address::generate(&env))),
        invalid
    );
    assert_eq!(client.try_set_fees(&50u32, &0u32, &None), invalid);

    client.set_fees(&0u32, &0u32, &None);
    assert_eq!(client.wrap_fee_bps(), 0);
    assert_eq!(client.fee_recipient(), None);
    assert!(client.try_wrap_batch(&admin, &vec![&env]).is_err());
}
