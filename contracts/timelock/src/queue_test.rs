//! Tests for the governance transaction queue: cancellation (#1505) and
//! grace-period enforcement (#1504).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

extern crate std;

use crate::{
    DEFAULT_GRACE_PERIOD, TimelockContract, TimelockContractClient, TimelockError, TxState,
};
use soroban_sdk::{
    Address, BytesN, Env, IntoVal, Symbol, Val, Vec, contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
};

/// Minimal target contract whose state changes only if a queued call executes.
#[contract]
pub struct Target;

#[contractimpl]
impl Target {
    pub fn set(env: Env, value: u32) {
        env.storage().instance().set(&symbol_short!("v"), &value);
    }

    pub fn get(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("v"))
            .unwrap_or(0)
    }
}

struct Ctx<'a> {
    env: Env,
    client: TimelockContractClient<'a>,
    target: Address,
}

fn setup<'a>() -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&admin, &1_000);

    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    let release_ledger = env.ledger().sequence() + soroban_common::MIN_DEADLINE_BUFFER + 10;
    client.initialize(&admin, &token, &beneficiary, &release_ledger, &1_000);

    let target = env.register(Target, ());
    Ctx {
        env,
        client,
        target,
    }
}

fn set_args(env: &Env, value: u32) -> Vec<Val> {
    (value,).into_val(env)
}

fn target_value(ctx: &Ctx) -> u32 {
    TargetClient::new(&ctx.env, &ctx.target).get()
}

fn advance_to(env: &Env, ledger: u32) {
    env.ledger().with_mut(|l| l.sequence_number = ledger);
}

/// Execute `hash`, expecting a contract error, and return it.
fn execute_err(ctx: &Ctx, hash: &BytesN<32>) -> TimelockError {
    match ctx.client.try_execute_transaction(hash) {
        Err(Ok(e)) => e,
        other => panic!("expected contract error, got {other:?}"),
    }
}

fn queue(ctx: &Ctx, value: u32, delay: u32) -> (BytesN<32>, u32) {
    let eta = ctx.env.ledger().sequence() + delay;
    let hash = ctx.client.queue_transaction(
        &ctx.target,
        &Symbol::new(&ctx.env, "set"),
        &set_args(&ctx.env, value),
        &delay,
    );
    (hash, eta)
}

// ── Happy path ──────────────────────────────────────────────────────────────

#[test]
fn queued_transaction_executes_inside_window() {
    let ctx = setup();
    let (hash, eta) = queue(&ctx, 42, 100);
    assert_eq!(
        ctx.client.get_transaction(&hash).unwrap().state,
        TxState::Queued
    );

    advance_to(&ctx.env, eta);
    ctx.client.execute_transaction(&hash);

    assert_eq!(target_value(&ctx), 42);
    assert_eq!(
        ctx.client.get_transaction(&hash).unwrap().state,
        TxState::Executed
    );
    assert_eq!(
        execute_err(&ctx, &hash),
        TimelockError::TransactionAlreadyExecuted
    );
}

#[test]
fn execute_before_eta_fails() {
    let ctx = setup();
    let (hash, eta) = queue(&ctx, 1, 100);
    advance_to(&ctx.env, eta - 1);
    assert_eq!(execute_err(&ctx, &hash), TimelockError::NotYetReleasable);
}

#[test]
fn zero_delay_rejected() {
    let ctx = setup();
    let res = ctx.client.try_queue_transaction(
        &ctx.target,
        &Symbol::new(&ctx.env, "set"),
        &set_args(&ctx.env, 1),
        &0,
    );
    assert_eq!(res, Err(Ok(TimelockError::InvalidDelay)));
}

#[test]
fn duplicate_queue_rejected() {
    let ctx = setup();
    queue(&ctx, 7, 50);
    let res = ctx.client.try_queue_transaction(
        &ctx.target,
        &Symbol::new(&ctx.env, "set"),
        &set_args(&ctx.env, 7),
        &50,
    );
    assert_eq!(res, Err(Ok(TimelockError::TransactionAlreadyQueued)));
}

// ── #1504: grace period enforcement ─────────────────────────────────────────

#[test]
fn default_grace_period_applies() {
    let ctx = setup();
    assert_eq!(ctx.client.grace_period(), DEFAULT_GRACE_PERIOD);
}

#[test]
fn execute_at_last_ledger_of_grace_period_succeeds() {
    let ctx = setup();
    ctx.client.set_grace_period(&20);
    let (hash, eta) = queue(&ctx, 5, 100);

    advance_to(&ctx.env, eta + 20);
    ctx.client.execute_transaction(&hash);
    assert_eq!(target_value(&ctx), 5);
}

#[test]
fn expired_transaction_is_rejected() {
    let ctx = setup();
    ctx.client.set_grace_period(&20);
    let (hash, eta) = queue(&ctx, 5, 100);

    advance_to(&ctx.env, eta + 21);
    assert_eq!(execute_err(&ctx, &hash), TimelockError::TransactionExpired);
    assert_eq!(target_value(&ctx), 0);
    assert_eq!(
        ctx.client.get_transaction(&hash).unwrap().state,
        TxState::Queued
    );
}

#[test]
fn expired_with_default_grace_period_is_rejected() {
    let ctx = setup();
    let (hash, eta) = queue(&ctx, 5, 100);

    advance_to(&ctx.env, eta + DEFAULT_GRACE_PERIOD + 1);
    assert_eq!(execute_err(&ctx, &hash), TimelockError::TransactionExpired);
}

#[test]
fn zero_grace_period_rejected() {
    let ctx = setup();
    assert_eq!(
        ctx.client.try_set_grace_period(&0),
        Err(Ok(TimelockError::InvalidGracePeriod))
    );
}

// ── #1505: cancellation ─────────────────────────────────────────────────────

#[test]
fn cancelled_transaction_cannot_be_executed() {
    let ctx = setup();
    let (hash, eta) = queue(&ctx, 99, 100);

    ctx.client.cancel_transaction(&hash);
    assert_eq!(
        ctx.client.get_transaction(&hash).unwrap().state,
        TxState::Cancelled
    );

    advance_to(&ctx.env, eta);
    assert_eq!(execute_err(&ctx, &hash), TimelockError::AlreadyCancelled);
    assert_eq!(target_value(&ctx), 0);
}

#[test]
fn cancel_requires_admin_auth() {
    let ctx = setup();
    let (hash, _) = queue(&ctx, 1, 100);

    // Without any authorization the call must fail and leave the tx queued.
    ctx.env.set_auths(&[]);
    assert!(ctx.client.try_cancel_transaction(&hash).is_err());
    assert_eq!(
        ctx.client.get_transaction(&hash).unwrap().state,
        TxState::Queued
    );

    // With auth mocked, the recorded authorizer is the governance admin.
    let admin = ctx.client.get_info().admin;
    ctx.env.mock_all_auths();
    ctx.client.cancel_transaction(&hash);
    let auths = ctx.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, admin);
}

#[test]
fn cancel_unknown_transaction_fails() {
    let ctx = setup();
    let unknown = BytesN::from_array(&ctx.env, &[0u8; 32]);
    assert_eq!(
        ctx.client.try_cancel_transaction(&unknown),
        Err(Ok(TimelockError::TransactionNotQueued))
    );
}

#[test]
fn cancel_twice_fails() {
    let ctx = setup();
    let (hash, _) = queue(&ctx, 1, 100);
    ctx.client.cancel_transaction(&hash);
    assert_eq!(
        ctx.client.try_cancel_transaction(&hash),
        Err(Ok(TimelockError::AlreadyCancelled))
    );
}

#[test]
fn cancel_after_execution_fails() {
    let ctx = setup();
    let (hash, eta) = queue(&ctx, 1, 100);
    advance_to(&ctx.env, eta);
    ctx.client.execute_transaction(&hash);
    assert_eq!(
        ctx.client.try_cancel_transaction(&hash),
        Err(Ok(TimelockError::TransactionAlreadyExecuted))
    );
}
