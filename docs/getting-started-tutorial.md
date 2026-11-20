# Getting Started: Build and Test Your First Soroban Contract

This tutorial walks you through creating, testing, optimizing, and deploying your first custom Soroban smart contract from scratch. By the end you will have a working contract running on a local testnet that you can invoke from the command line.

If you are looking for reference material rather than a guided walkthrough, see:

- [API Reference](./api-reference.md)
- [Storage Layout](./storage-layout.md)
- [Testing Guide](./testing.md)
- [Deployment Guide](./deployment.md)

---

## Table of Contents

1. [Prerequisites & Environment Setup](#1-prerequisites--environment-setup)
2. [Scaffolding a New Contract](#2-scaffolding-a-new-contract)
3. [Writing Storage Types, Errors, and Contract Logic](#3-writing-storage-types-errors-and-contract-logic)
4. [Writing Tests](#4-writing-tests)
5. [Building & Optimizing](#5-building--optimizing)
6. [Deploying to a Local Testnet](#6-deploying-to-a-local-testnet)
7. [Invoking Contract Methods](#7-invoking-contract-methods)
8. [Next Steps](#8-next-steps)

---

## 1. Prerequisites & Environment Setup

### 1.1 Install Rust

Soroban contracts are written in Rust and compiled to WebAssembly (WASM). Install Rust via `rustup`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Restart your shell (or `source "$HOME/.cargo/env"`), then add the WASM target used by Soroban:

```bash
rustup target add wasm32-unknown-unknown
```

Verify the installation:

```bash
rustc --version
cargo --version
```

> **Tip:** Soroban contracts require a recent stable Rust toolchain. If you hit build errors, run `rustup update stable`.

### 1.2 Install the Stellar (Soroban) CLI

The Stellar CLI (formerly `soroban-cli`) is used to build, deploy, and invoke contracts.

**macOS / Linux (Homebrew):**

```bash
brew install stellar-cli
```

**From source (any platform):**

```bash
cargo install --locked stellar-cli --features opt
```

Verify:

```bash
stellar --version
```

> **Note:** Older documentation may refer to the binary as `soroban`. The current binary is `stellar`. If you have an older install, run `cargo install --locked stellar-cli` to upgrade.

### 1.3 Use the Devcontainer (Recommended)

This repository ships a [devcontainer](../.devcontainer/devcontainer.json) that pre-installs Rust, the WASM target, the Stellar CLI, and all recommended VS Code extensions. Using it guarantees a reproducible environment.

**With VS Code:**

1. Install the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers).
2. Open this repository in VS Code.
3. Run **Dev Containers: Reopen in Container** from the command palette (`Ctrl+Shift+P` / `Cmd+Shift+P`).

**With the Dev Container CLI:**

```bash
npm install -g @devcontainers/cli
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . bash
```

Once inside the container, confirm the toolchain:

```bash
rustc --version
stellar --version
```

### 1.4 Clone the Repository

```bash
git clone https://github.com/<your-org>/<this-repo>.git
cd <this-repo>
```

---

## 2. Scaffolding a New Contract

We will build a small **counter** contract that stores a value and lets callers increment it, with an owner-only reset. This exercises storage types, custom errors, and authorization.

### 2.1 Create the Crate

From the repository root:

```bash
cargo new --lib contracts/counter
cd contracts/counter
```

### 2.2 Configure `Cargo.toml`

Replace the contents of `contracts/counter/Cargo.toml` with:

```toml
[package]
name = "counter"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
soroban-sdk = "21.0.0"

[dev-dependencies]
soroban-sdk = { version = "21.0.0", features = ["testutils"] }
proptest = "1.4"

[profile.release]
opt-level = "z"
overflow-checks = true
debug = 0
strip = "symbols"
debug-assertions = false
panic = "abort"
codegen-units = 1
lto = true
```

> **Note:** Pin `soroban-sdk` to the version used elsewhere in this repository. Check the workspace `Cargo.toml` for the canonical version.

### 2.3 Verify the Skeleton Builds

```bash
cargo build --target wasm32-unknown-unknown --release
```

You should see a `target/wasm32-unknown-unknown/release/counter.wasm` file produced.

---

## 3. Writing Storage Types, Errors, and Contract Logic

Open `contracts/counter/src/lib.rs` and replace its contents with the following, section by section.

### 3.1 Storage Types

Soroban supports three storage durability classes: **persistent**, **temporary**, and **instance**. We store the counter in instance storage (cheap, tied to the contract instance) and the owner in persistent storage.

```rust
#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

/// Keys used to address values in contract storage.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The current counter value (instance storage).
    Counter,
    /// The address permitted to reset the counter (persistent storage).
    Owner,
}
```

### 3.2 Error Enum

Custom errors are returned as `Result<T, Error>` from contract functions. Keep error codes stable — clients may match on them.

```rust
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// Caller is not authorized to perform this action.
    Unauthorized = 1,
    /// Counter would overflow.
    Overflow = 2,
    /// Counter is already at zero and cannot be decremented.
    Underflow = 3,
}
```

### 3.3 Contract Logic

```rust
#[contract]
pub struct CounterContract;

#[contractimpl]
impl CounterContract {
    /// Initialize the contract with an owner and a starting value.
    pub fn initialize(env: Env, owner: Address, initial: u32) -> Result<(), Error> {
        if env.storage().persistent().has(&DataKey::Owner) {
            return Err(Error::Unauthorized);
        }
        env.storage().persistent().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::Counter, &initial);
        Ok(())
    }

    /// Return the current counter value.
    pub fn get(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0)
    }

    /// Increment the counter by one.
    pub fn increment(env: Env) -> Result<u32, Error> {
        let current: u32 = Self::get(env.clone());
        let next = current.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::Counter, &next);
        Ok(next)
    }

    /// Decrement the counter by one.
    pub fn decrement(env: Env) -> Result<u32, Error> {
        let current: u32 = Self::get(env.clone());
        let next = current.checked_sub(1).ok_or(Error::Underflow)?;
        env.storage().instance().set(&DataKey::Counter, &next);
        Ok(next)
    }

    /// Reset the counter to zero. Only the owner may call this.
    pub fn reset(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let owner: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Owner)
            .ok_or(Error::Unauthorized)?;
        if caller != owner {
            return Err(Error::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Counter, &0u32);
        Ok(())
    }
}
```

### 3.4 Build Check

```bash
cargo build --target wasm32-unknown-unknown --release
```

---

## 4. Writing Tests

Soroban provides a `testutils` feature that spins up an in-memory ledger. We will write three kinds of tests: **unit**, **property-based**, and **snapshot**.

Create `contracts/counter/src/test.rs`:

```rust
#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};
```

### 4.1 Unit Tests

```rust
#[test]
fn test_initialize_and_get() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CounterContract);
    let client = CounterContractClient::new(&env, &contract_id);

    let owner = Address::random(&env);
    client.initialize(&owner, &5);
    assert_eq!(client.get(), 5);
}

#[test]
fn test_increment_and_decrement() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CounterContract);
    let client = CounterContractClient::new(&env, &contract_id);

    let owner = Address::random(&env);
    client.initialize(&owner, &0);

    assert_eq!(client.increment(), 1);
    assert_eq!(client.increment(), 2);
    assert_eq!(client.decrement(), 1);
}

#[test]
fn test_reset_requires_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CounterContract);
    let client = CounterContractClient::new(&env, &contract_id);

    let owner = Address::random(&env);
    let stranger = Address::random(&env);
    client.initialize(&owner, &10);

    let result = client.try_reset(&stranger);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    client.reset(&owner);
    assert_eq!(client.get(), 0);
}
```

### 4.2 Property-Based Tests

Property-based tests use [`proptest`](https://docs.rs/proptest) to generate many inputs and assert invariants hold. Add the following to `test.rs`:

```rust
use proptest::prelude::*;

proptest! {
    /// Incrementing N times from zero always yields N.
    #[test]
    fn prop_increment_n_times(n in 0u32..1000) {
        let env = Env::default();
        let contract_id = env.register_contract(None, CounterContract);
        let client = CounterContractClient::new(&env, &contract_id);

        let owner = Address::random(&env);
        client.initialize(&owner, &0);

        for _ in 0..n {
            client.increment();
        }
        prop_assert_eq!(client.get(), n);
    }

    /// Increment then decrement is the identity.
    #[test]
    fn prop_inc_dec_roundtrip(start in 0u32..1000) {
        let env = Env::default();
        let contract_id = env.register_contract(None, CounterContract);
        let client = CounterContractClient::new(&env, &contract_id);

        let owner = Address::random(&env);
        client.initialize(&owner, &start);

        client.increment();
        client.decrement();
        prop_assert_eq!(client.get(), start);
    }
}
```

### 4.3 Snapshot Tests

Snapshot tests capture the serialized ledger state after a sequence of calls and compare it against a checked-in fixture. This catches unintended storage-layout changes.

```rust
#[test]
fn test_storage_snapshot() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CounterContract);
    let client = CounterContractClient::new(&env, &contract_id);

    let owner = Address::random(&env);
    client.initialize(&owner, &0);
    client.increment();
    client.increment();

    // Serialize the contract's instance storage and compare to a fixture.
    let snapshot = env.to_snapshot();
    let expected = include_str!("../snapshots/storage_snapshot.json");
    assert_eq!(snapshot, expected);
}
```

Generate the fixture the first time by running the test with `UPDATE_SNAPSHOTS=1` (or copy the printed value into `contracts/counter/snapshots/storage_snapshot.json`):

```bash
mkdir -p contracts/counter/snapshots
UPDATE_SNAPSHOTS=1 cargo test test_storage_snapshot
```

> **Note:** If your `soroban-sdk` version does not expose `Env::to_snapshot`, use `env.storage().instance().all()` and serialize the keys/values manually. See the [Testing Guide](./testing.md) for version-specific helpers.

### 4.4 Run All Tests

```bash
cargo test
```

You should see unit, property, and snapshot tests all pass. To run only the property tests:

```bash
cargo test prop_
```

---

## 5. Building & Optimizing

### 5.1 Build the WASM Artifact

```bash
stellar contract build
```

This produces `target/wasm32-unknown-unknown/release/counter.wasm`. Alternatively:

```bash
cargo build --target wasm32-unknown-unknown --release
```

### 5.2 Optimize the WASM

Smaller WASM means lower deployment fees. The `stellar` CLI can optimize the artifact:

```bash
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/counter.wasm
```

This writes an optimized file alongside the original (e.g. `counter.optimized.wasm`). Confirm the size reduction:

```bash
ls -lh target/wasm32-unknown-unknown/release/counter*.wasm
```

> **Tip:** The `[profile.release]` settings in `Cargo.toml` (`opt-level = "z"`, `lto = true`, `strip = "symbols"`) already do most of the work. `stellar contract optimize` squeezes out the rest.

---

## 6. Deploying to a Local Testnet

### 6.1 Start a Local Network

The Stellar CLI can run a local network (a "quickstart" node) with a funded account:

```bash
stellar network start local
```

This starts a local RPC endpoint at `http://localhost:8000/soroban/rpc` and a friendbot for funding accounts. Leave it running in a separate terminal.

### 6.2 Configure the CLI Network

```bash
stellar network add \
  --global local \
  --rpc-url http://localhost:8000/soroban/rpc \
  --network-passphrase "Standalone Network ; February 2017"
```

### 6.3 Create and Fund an Identity

```bash
stellar keys generate --global alice --network local --fund
```

Verify the account was funded:

```bash
stellar keys address alice
```

### 6.4 Deploy the Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/counter.optimized.wasm \
  --source alice \
  --network local
```

The command prints a contract ID (a `C...` string). Save it:

```bash
export COUNTER_ID=<paste-contract-id-here>
```

---

## 7. Invoking Contract Methods

### 7.1 Initialize

```bash
stellar contract invoke \
  --id "$COUNTER_ID" \
  --source alice \
  --network local \
  -- \
  initialize \
  --owner "$(stellar keys address alice)" \
  --initial 0
```

### 7.2 Read the Counter

```bash
stellar contract invoke \
  --id "$COUNTER_ID" \
  --source alice \
  --network local \
  -- \
  get
```

Expected output: `0`.

### 7.3 Increment

```bash
stellar contract invoke \
  --id "$COUNTER_ID" \
  --source alice \
  --network local \
  -- \
  increment
```

Expected output: `1`.

### 7.4 Decrement

```bash
stellar contract invoke \
  --id "$COUNTER_ID" \
  --source alice \
  --network local \
  -- \
  decrement
```

Expected output: `0`.

### 7.5 Reset (Owner Only)

```bash
stellar contract invoke \
  --id "$COUNTER_ID" \
  --source alice \
  --network local \
  -- \
  reset \
  --caller "$(stellar keys address alice)"
```

### 7.6 Inspect the Contract

Fetch the on-chain WASM to confirm the deployed bytecode matches your build:

```bash
stellar contract fetch \
  --id "$COUNTER_ID" \
  --network local \
  --out-file deployed.wasm
```

### 7.7 Tear Down

Stop the local network when you are done:

```bash
stellar network stop local
```

---

## 8. Next Steps

You have now built, tested, optimized, and deployed a Soroban contract end-to-end. From here:

- **Dive deeper into storage:** [Storage Layout](./storage-layout.md) covers TTL management, rent, and migration patterns.
- **Expand your test suite:** [Testing Guide](./testing.md) documents fuzzing, auth mocking, and snapshot helpers in detail.
- **Deploy to testnet/mainnet:** [Deployment Guide](./deployment.md) covers network configuration, upgradeability, and verification.
- **Explore the API:** [API Reference](./api-reference.md) lists every SDK type and function used in this tutorial.
- **Contribute:** See [CONTRIBUTING.md](../CONTRIBUTING.md) for how to add contracts to this repository.

Happy building!