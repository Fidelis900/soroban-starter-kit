# Testing Guide

This document explains the testing strategy for this repository, when to use each test type, and how to run them.

## Test Types at a Glance

| Type | Location | When to use |
|------|----------|-------------|
| Unit | `contracts/<name>/src/test.rs` | Single function, single contract, no cross-contract calls |
| Property | `contracts/<name>/src/prop_test.rs` | Invariants that must hold for arbitrary inputs |
| Integration | `tests/tests/integration.rs` | Multiple contracts interacting (e.g. token + escrow) |
| Snapshot | auto-generated `test_snapshots/` | Detect unintended changes to ledger state or auth |
| Benchmark | `benches/benches/` | Track compute unit (CU) cost of hot paths |
| Fuzz | `fuzz/fuzz_targets/` | Find panics or unexpected errors from raw byte input |

---

## Unit Tests

Unit tests live inside the contract crate (`src/test.rs`) behind `#![cfg(test)]`. They test one contract in isolation.

```rust
#[test]
fn test_mint() {
    let env = Env::default();
    env.mock_all_auths();           // bypass auth checks for the whole test
    let admin = Address::generate(&env);
    let client = init_token(&env, &admin);
    let user = Address::generate(&env);

    client.mint(&user, &1_000i128);
    assert_eq!(client.balance(&user), 1_000i128);
}
```

**When to write a unit test:**
- Adding or changing a single contract function.
- Testing an error path (use `#[should_panic(expected = "Error(Contract, #N)")]` or `try_*` methods).
- Verifying event emission with `env.events().all()`.

**When _not_ to use unit tests:**
- Cross-contract token transfers — use integration tests instead (mock tokens hide real transfer logic).

### `mock_all_auths`

`env.mock_all_auths()` makes every `require_auth` call succeed without a real signature. Use it in almost every test. The only time you should omit it is when you are explicitly testing that an unauthorized caller is rejected:

```rust
#[test]
fn test_unauthorized_mint_fails() {
    let env = Env::default();
    // Do NOT call env.mock_all_auths() here
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let client = init_token(&env, &admin);

    // Calling mint without auth should panic
    let result = client.try_mint(&attacker, &1_000i128);
    assert!(result.is_err());
}
```

---

## Property Tests

Property tests use [proptest](https://proptest-rs.github.io/proptest/) to verify invariants across a large range of generated inputs. They live in `src/prop_test.rs`.

```rust
proptest! {
    #[test]
    fn prop_mint_burn_roundtrip(amount in 1i128..=i128::MAX / 2) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _) = setup(&env);
        let user = Address::generate(&env);

        client.mint(&user, &amount);
        client.admin_burn(&user, &amount);

        prop_assert_eq!(client.balance(&user), 0);
        prop_assert_eq!(client.total_supply(), 0);
    }
}
```

**When to write a property test:**
- Arithmetic invariants: balances sum to total supply, transfer is conservative, allowance decrements correctly.
- State machine invariants: valid states are always reachable, terminal states cannot transition further.
- Boundary conditions: any amount in a valid range should succeed; any amount outside should fail.

**Use `prop_assert!` / `prop_assert_eq!`** instead of `assert!` inside `proptest!` blocks — they report the failing input rather than just panicking.

### Proptest configuration

The root `proptest.toml` configures proptest for all contracts in this workspace:

```toml
[proptest]
cases = 256
failure_persistence = "MapPath"
```

`failure_persistence = "MapPath"` tells proptest to write failing seeds to `proptest-regressions/<module_path>.txt` next to the test source. These files are listed in `.gitignore` so they are not committed — see the note below on how to share a failing seed.

Override the number of cases for a single run:

```bash
PROPTEST_CASES=10000 cargo test -p soroban-token-template prop_
```

### Reproducing a Failing Seed

When proptest finds a failure it prints a line like:

```
thread 'prop_mint_burn_roundtrip' panicked at ...
PROPTEST_REGRESSIONS=contracts/token/proptest-regressions/prop_test.txt
Failing input: amount = 9223372036854775807
```

The seed is saved to `proptest-regressions/prop_test.txt` inside the contract directory. To reproduce deterministically:

```bash
# Re-run only that test — proptest reads the saved regression file automatically
cargo test -p soroban-token-template prop_mint_burn_roundtrip
```

To share a failing seed with a colleague or in a bug report, copy the content of the regression file and paste it. They can place it at the same path and run the same command.

> **Gitignore choice:** `proptest-regressions/` directories are listed in `.gitignore` and are **not committed**. This keeps the repository clean. If you want CI to always replay a specific known failure, copy the regression file into a test fixture and add a dedicated `#[test]` that runs proptest with that seed using `ProptestConfig::with_cases(1)` and the `Just` strategy.

---

## Integration Tests

Integration tests live in `tests/tests/integration.rs` and are compiled as a separate crate (`soroban-integration-tests`). They deploy multiple contracts into the same `Env` and verify that they interact correctly.

```rust
#[test]
fn test_full_escrow_lifecycle_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let (token, token_addr) = deploy_token(&env, &token_admin);
    token.mint(&buyer, &1_000i128);

    let (escrow, escrow_addr) = deploy_escrow(&env);
    escrow.initialize(&buyer, &seller, &arbiter, &token_addr, &1_000i128, &deadline);

    escrow.fund();
    assert_eq!(token.balance(&escrow_addr), 1_000i128);

    escrow.mark_delivered();
    escrow.approve_delivery();
    assert_eq!(token.balance(&seller), 1_000i128);
}
```

**When to write an integration test:**
- Any change that touches the boundary between two contracts (e.g. escrow calling the token's `transfer_from`).
- Testing that real token balances move correctly through a full lifecycle.
- Verifying behaviour with a Stellar Asset Contract (SAC) token via `env.register_stellar_asset_contract_v2`.

### Advancing the Ledger

Soroban's test environment exposes `env.ledger().with_mut(|l| ...)` to manipulate ledger state. Use it to simulate time passing (deadline checks, allowance expiry):

```rust
// Advance past the escrow deadline
env.ledger().with_mut(|l| l.sequence_number = deadline + 1);
assert!(escrow.is_deadline_passed());
escrow.request_refund();
```

Other fields you can set:

```rust
env.ledger().with_mut(|l| {
    l.sequence_number = 1000;   // current ledger sequence
    l.timestamp = 1_700_000_000; // Unix timestamp in seconds
});
```

**Run integration tests:**

```bash
cargo test -p soroban-integration-tests
```

---

## Snapshot Tests

Every test that runs against a Soroban `Env` automatically writes a JSON snapshot of the final ledger state (auth entries, storage, events) to a `test_snapshots/` directory next to the crate. The snapshot is checked on the next run; if it differs, the test fails.

> **Gitignore choice:** `test_snapshots/` directories are listed in `.gitignore` and are **not committed**. Each contributor generates their own snapshots locally, so they are not a cross-commit regression guard and never appear in `git status`/`git diff` for review. What they *are* useful for is catching accidental changes within a single working session: if you refactor and a snapshot that was passing a moment ago now differs, the failure points at exactly what changed in ledger state or auth calls.

**Updating snapshots** after an intentional change:

```bash
# Delete the stale snapshot(s) and re-run; new snapshots are written automatically
rm contracts/token/test_snapshots/test/test_mint.1.json
cargo test -p soroban-token-template test_mint

# Or delete all snapshots for a contract and regenerate
rm -rf contracts/token/test_snapshots/
cargo test -p soroban-token-template
```

Because the snapshots are gitignored, there is no diff to review or commit — regenerating them simply refreshes your local baseline. If you want a change to ledger state or auth calls to be visible in code review, assert on it explicitly in the test (e.g. `env.events().all()` or `client.balance(...)`) rather than relying on the snapshot.
Review the diff with `git diff` before committing — unexpected changes to auth entries or storage keys are a signal that something is wrong.

---

## Benchmarks

Benchmarks live in `benches/benches/` and use [Criterion](https://bheisler.github.io/criterion.rs/book/). They measure the compute unit (CU) cost of hot paths so regressions are caught before they reach mainnet.

```bash
# Run all benchmarks
cargo bench

# Run only token benchmarks
cargo bench -p contract-benchmarks --bench token_ops

# Save a named baseline for later comparison
cargo bench --save-baseline before-optimization

# Compare against a saved baseline
cargo bench --baseline before-optimization
```

**When to write a benchmark:**
- A hot path whose CU cost matters (token transfer, escrow state transitions).
- Before and after an optimization, to prove the change is worthwhile.

**When _not_ to benchmark:**
- Cold paths that run once per contract deployment.
- Anything whose cost is dominated by host functions you cannot change.

---

## Fuzz Tests

Fuzz tests live in `fuzz/fuzz_targets/` and use [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz). They feed raw bytes into contract entry points to find panics or unexpected errors.

```bash
# Run a fuzz target for a fixed number of iterations
cargo fuzz run fuzz_target_1 -- -runs=100000
```

**When to write a fuzz test:**
- Parsing untrusted input (e.g. a serialized payload).
- Any function that must never panic regardless of input.

---

## Running the Full Suite

```bash
# Unit + property + integration + snapshot tests
cargo test --workspace

# Everything, including benchmarks
cargo test --workspace && cargo bench
```
