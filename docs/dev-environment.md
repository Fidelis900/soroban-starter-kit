# Development Environment Setup

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Node.js | 20+ | https://nodejs.org |
| Docker | 24+ | https://docs.docker.com/get-docker/ |
| Rust | 1.78+ | https://rustup.rs |
| Stellar CLI | 21.7.7 | `cargo install --locked stellar-cli@21.7.7 --features opt` |
| just (optional) | latest | `cargo install just` |

### Installing `just`

`just` is a command runner popular in the Rust ecosystem with better cross-platform support than `make`:

```bash
# Via cargo (all platforms)
cargo install just

# Via homebrew (macOS/Linux)
brew install just

# Via winget (Windows)
winget install Casey.Just
```

Run `just --list` from the project root to see all available recipes. The
`justfile` mirrors the `Makefile` targets (`build`, `test`, `clippy`, `fmt`,
`fmt-check`, `lint-fix`, `watch`, `install-hooks`, `deploy-testnet`,
`deploy-local`, `bench`, `clean`) so either tool works interchangeably.

---

## Faster test runs with cargo-nextest

[cargo-nextest](https://nexte.st/) is a faster test runner for Rust. It
parallelizes better than `cargo test` for a workspace this size and gives
cleaner per-test output.

### Installing cargo-nextest

```bash
cargo install cargo-nextest --locked
```

### Running tests

```bash
# Run the full workspace test suite
cargo nextest run --workspace

# With all features enabled
cargo nextest run --workspace --all-features

# Via the Makefile / justfile
make test-nextest
just test-nextest
```

`cargo nextest` runs the same test binaries as `cargo test`, so it's a drop-in
alternative — no test code changes are required.

---

## Quick Start (recommended)

```bash
git clone https://github.com/your-org/soroban-starter-kit.git
cd soroban-starter-kit
bash scripts/setup.sh        # installs deps, creates .env, adds git hook
npm run dev                  # http://localhost:3000
```

---

## Docker Compose

All services (contract builder, local Stellar node) are defined in `docker/docker-compose.yml`.

```bash
# Start everything
docker compose -f docker/docker-compose.yml up

# Contract builder only
docker compose -f docker/docker-compose.yml up contracts
```

---

## Local Stellar Network

### Starting the node

```bash
# Start only the Stellar/Soroban node
docker compose -f docker/docker-compose.yml up stellar-node

# Or use the helper script (starts node and waits for healthy status)
./scripts/local-net.sh start    # start node, wait for healthy
./scripts/local-net.sh status   # check status
./scripts/local-net.sh reset    # wipe chain data and restart
./scripts/local-net.sh stop     # stop node
./scripts/local-net.sh logs     # tail logs
```

### Full teardown and restart

If the node gets stuck or you need a totally clean slate, use `scripts/reset-localnet.sh`. It stops, removes, and restarts the `stellar-node` container, then blocks until Docker reports the container healthy:

```bash
./scripts/reset-localnet.sh          # default 180s timeout
./scripts/reset-localnet.sh 300      # custom timeout in seconds
```

The script exits 0 once the node is healthy and 1 if it times out.

### Waiting for readiness

The node takes ~30 seconds to initialize. Use `scripts/wait-for-node.sh` to block until the Soroban RPC endpoint is accepting requests:

```bash
# Default: polls http://localhost:8000/soroban/rpc, waits up to 120s
./scripts/wait-for-node.sh

# Custom URL and timeout
./scripts/wait-for-node.sh http://localhost:8000/soroban/rpc 180
```

The script exits 0 when the node is healthy and 1 if it times out, making it safe to use in CI and deployment pipelines.

### Health check

The `stellar-node` service in `docker/docker-compose.yml` includes a built-in health check that queries `POST /soroban/rpc` with `getHealth`. Docker marks the container healthy only after the RPC responds with `"healthy"`, so the dependent `contracts` service will not start until the node is ready.

Endpoints when running locally:

| Service | URL |
|---------|-----|
| Soroban RPC | http://localhost:8000/soroban/rpc |
| Horizon API | http://localhost:8001 |

Set `VITE_STELLAR_NETWORK=local` in `.env` to point the frontend at the local node.

---

## Contract Deployment

```bash
./scripts/deploy.sh testnet          # deploy all contracts to testnet
./scripts/deploy.sh testnet token    # deploy only the token contract
./scripts/deploy.sh local            # deploy to local node
```

---

## Dev Container (VS Code / GitHub Codespaces)

Open the repo in VS Code and click **"Reopen in Container"** when prompted, or run:

```
Dev Containers: Reopen in Container
```

The container automatically runs `scripts/setup.sh` on creation and forwards ports 3000, 8000, and 8001.

---

## Environment Variables

Copy `.env.example` to `.env` and fill in values:

```bash
cp .env.example .env
```

| Variable | Default | Description |
|----------|---------|-------------|
| `VITE_STELLAR_NETWORK` | `testnet` | `testnet`, `mainnet`, or `local` |
| `VITE_SOROBAN_RPC_URL` | testnet RPC | Soroban RPC endpoint |
| `VITE_HORIZON_URL` | testnet Horizon | Horizon REST endpoint |
| `VITE_NETWORK_PASSPHRASE` | testnet passphrase | Network passphrase |
| `VITE_VAPID_PUBLIC_KEY` | _(empty)_ | VAPID key for push notifications |
| `LOCAL_RPC_PORT` | `8000` | Local node RPC port |
| `LOCAL_HORIZON_PORT` | `8001` | Local node Horizon port |

---

## Troubleshooting

**`npm ci` fails**
- Ensure Node.js 20+: `node --version`
- Delete `node_modules/` and retry

**Docker Compose port conflict**
- Change `LOCAL_RPC_PORT` / `LOCAL_HORIZON_PORT` in `.env`

**`stellar` command not found**
- Run: `cargo install --locked stellar-cli --features opt`
- Ensure `~/.cargo/bin` is in `$PATH`

**Local node never becomes healthy**
- Check Docker has enough memory (≥4 GB recommended)
- Run `./scripts/local-net.sh logs` to inspect errors

**Contract build fails (`wasm32` target missing)**
```bash
rustup target add wasm32-unknown-unknown
```

**Freighter wallet not connecting**
- Install the [Freighter extension](https://freighter.app)
- Switch Freighter to the matching network (Testnet / Mainnet)

---

## Benchmarks with cargo-criterion

The project uses [cargo-criterion](https://github.com/bheisler/cargo-criterion) for
benchmarking. It wraps the standard `criterion` crate and adds:

- **Machine-readable JSON output** for CI regression comparison
- **HTML reports** with charts saved under `target/criterion/`
- Cleaner CLI output with per-benchmark statistics

### Installing cargo-criterion

```bash
cargo install cargo-criterion --locked
```

### Running benchmarks locally

```bash
# Run all benchmarks and generate HTML reports
cargo criterion --package contract-benchmarks

# Run a single benchmark group
cargo criterion --package contract-benchmarks --bench escrow_ops

# Open the HTML report in your browser (macOS)
open target/criterion/report/index.html

# Linux
xdg-open target/criterion/report/index.html
```

### Comparing against a baseline

```bash
# Save a baseline before your change
cargo criterion --package contract-benchmarks -- --save-baseline before

# Make your change, then compare
cargo criterion --package contract-benchmarks -- --baseline before
```

### CI integration

The `.github/workflows/bench.yml` workflow:
1. Runs `cargo criterion` on every push and PR
2. Uploads HTML reports as a GitHub Actions artifact (`criterion-html-reports-<sha>`)
3. Compares results against the stored `main` baseline and fails if any
   benchmark regresses by more than 10%
4. Posts a markdown regression table as a PR comment

---

## Fuzz Testing

Fuzz testing helps discover edge cases and potential vulnerabilities in contract code.

### Running Fuzz Tests

```bash
# Install cargo-fuzz if not already installed
cargo install cargo-fuzz

# Run fuzz target for token contract
cd fuzz
cargo fuzz run token_fuzz

# Run with a specific number of iterations
cargo fuzz run token_fuzz -- -max_len=1024 -runs=10000

# Run with a corpus directory
cargo fuzz run token_fuzz -- corpus/
```

### Fuzz Targets

- **token_fuzz**: Exercises token contract operations (mint, burn, transfer, approve, transfer_from, balance) with arbitrary inputs

### Interpreting Results

- Fuzz tests generate random inputs and monitor for crashes, panics, or undefined behavior
- Crashes are saved to `fuzz/artifacts/` for reproduction
- Use `RUST_BACKTRACE=1` for detailed crash information

```bash
RUST_BACKTRACE=1 cargo fuzz run token_fuzz -- fuzz/artifacts/token_fuzz/crash-*
```
## Debugging Macros

Soroban contracts rely heavily on proc macros (`#[contract]`, `#[contractimpl]`, `#[contracttype]`, etc.). When macro-related errors are cryptic or the generated code behaves unexpectedly, `cargo-expand` lets you inspect the exact Rust code the macros emit.

### Installing cargo-expand

```bash
cargo install cargo-expand
```

`cargo-expand` requires the nightly toolchain to pretty-print expanded output:

```bash
rustup toolchain install nightly
```

### Example Commands

**Expand an entire contract file:**

```bash
# From the contract's crate directory (e.g. contracts/token)
cargo +nightly expand
```

**Expand a specific module:**

```bash
cargo +nightly expand token
```

**Expand a specific item (struct, impl block, function):**

```bash
cargo +nightly expand --lib token::Token
```

**Expand and pipe to a file for easier inspection:**

```bash
cargo +nightly expand > expanded.rs
```

**Expand with all features enabled:**

```bash
cargo +nightly expand --all-features
```

**Expand for the wasm32 target (matches the actual build target):**

```bash
cargo +nightly expand --target wasm32-unknown-unknown
```

### Common Macro Expansion Issues

**`#[contract]` / `#[contractimpl]` not generating expected trait impls**

Run `cargo +nightly expand` and search for the `IntoVal` / `TryFromVal` implementations. If they are missing, check that:
- The struct derives or is annotated correctly (`#[contract]` on the struct, `#[contractimpl]` on the `impl` block).
- There is no conflicting manual `impl` of the same trait in the expanded output.

**Mysterious "method not found" or "trait bound not satisfied" errors**

These often mean a macro failed silently. Expand the file and look for `compile_error!` calls or incomplete `impl` blocks in the output — they indicate which macro panicked and why.

**`#[contracttype]` enum/struct not serializing correctly**

Expand the type and verify the generated `ScVal` conversion arms match your expected variants. A missing `#[contracttype]` on a nested type will produce a stub that falls back to a raw `ScVal::Void`.

**Duplicate symbol errors when building for wasm32**

Expand with `--target wasm32-unknown-unknown` to see if two macros are emitting the same symbol. The duplicated `fn __call` or `fn __spec` entry in the expansion points to the conflicting `#[contractimpl]` block.

**Expanded output is unreadable**

Use `rustfmt` to format the expanded output:

```bash
cargo +nightly expand | rustfmt --edition 2021 > expanded.rs
```

---

## Secrets Management

### Golden rules

- **Never commit `.env`** — it is listed in `.gitignore`. If you accidentally stage it, run `git reset HEAD .env`.
- **Never commit real keys, mnemonics, or tokens** — not even in comments or test fixtures.
- **`.env.example` is the source of truth** for which variables exist. It must contain only placeholder values (e.g. `SXXX…`, `your-api-key-here`).
- **Rotate immediately** if a secret is ever pushed to a remote branch. Treat the key as permanently compromised regardless of whether the commit was later removed.

### Local setup

```bash
cp .env.example .env   # create your local config from the template
# edit .env and fill in real values — this file is gitignored
```

### Sharing configuration between developers

Share the *shape* of config (variable names, descriptions) via `.env.example`. Share actual values through a secrets manager (e.g. AWS Secrets Manager, 1Password, Doppler) or an encrypted channel — never via chat, email, or a repository.

### Pre-commit hook

A sample hook lives at `.githooks/pre-commit`. Enable it once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/pre-commit
```

The hook blocks commits that contain common secret patterns (Stellar secret keys, mnemonics, bearer tokens, API keys) or that stage a `.env` file directly.
