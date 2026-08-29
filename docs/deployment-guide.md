# Deployment Guide

Step-by-step instructions for deploying Soroban contracts and the frontend across all environments.

---

## Prerequisites

### Install Stellar CLI

The Stellar CLI is required to generate identities, fund accounts, and deploy contracts.

```bash
# Install Stellar CLI
cargo install --locked stellar-cli --features opt

# Verify installation
stellar --version
```

**Expected output:**
```
stellar 21.x.x
```

### Generate a Stellar CLI Identity

Create a new identity for contract deployment:

```bash
# Generate a new identity (interactive)
stellar keys generate --global deployer

# Or generate non-interactively
stellar keys generate deployer --network testnet
```

**Expected output:**
```
Created identity "deployer" with public key: GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

The secret key is stored securely in `~/.stellar/keys.yaml` (never commit this file).

### Fund Your Account on Testnet

Use Friendbot to fund your testnet account with 10,000 XLM:

```bash
# Fund the deployer account
stellar keys fund deployer --network testnet
```

**Expected output:**
```
Funded account GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX with 10000 XLM
```

Verify the account is funded:

```bash
stellar account info --source-account deployer --network testnet
```

**Expected output:**
```
Account ID: GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
Sequence: 0
Balances:
  10000.0000000 XLM
```

### Verify Stellar CLI Configuration

Check that your identity is properly configured:

```bash
stellar keys list
```

**Expected output:**
```
deployer (testnet)
```

---

## Environments

| Environment | Network | RPC URL |
|-------------|---------|---------|
| `local` | Standalone | `http://localhost:8000` |
| `testnet` | Test SDF Network | `https://soroban-testnet.stellar.org` |
| `mainnet` | Public Global Stellar | `https://soroban.stellar.org` |

---

## 1. Environment Setup

Copy and configure environment variables:

```bash
cp .env.example .env
```

### Local

```env
VITE_STELLAR_NETWORK=local
VITE_SOROBAN_RPC_URL=http://localhost:8000
VITE_HORIZON_URL=http://localhost:8001
VITE_NETWORK_PASSPHRASE="Standalone Network ; February 2017"
```

### Testnet

```env
VITE_STELLAR_NETWORK=testnet
VITE_SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
VITE_HORIZON_URL=https://horizon-testnet.stellar.org
VITE_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
```

### Mainnet

```env
VITE_STELLAR_NETWORK=mainnet
VITE_SOROBAN_RPC_URL=https://soroban.stellar.org
VITE_HORIZON_URL=https://horizon.stellar.org
VITE_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
```

---

## 2. Identity Setup

`scripts/deploy.sh` uses a Stellar CLI identity (key pair) as the transaction source account. Before deploying, ensure the identity exists on your machine.

### Check if the default identity exists

```bash
stellar keys show default
```

If the command fails with "not found", create it:

```bash
# Generate a new key pair and store it globally
stellar keys generate --global default

# Or import an existing secret key
stellar keys add default --secret-key
# (you will be prompted to enter the secret key)
```

### Using a custom identity

Pass `--identity <name>` to use a key other than `default`:

```bash
./scripts/deploy.sh testnet --identity my-deployer
./scripts/deploy.sh local token --identity alice
```

`deploy.sh` validates the identity before building any contracts and prints setup guidance if it is missing.

### Funding the identity (testnet)

```bash
stellar keys fund default --network testnet
```

---

## 3. Local Deployment

```bash
# Start local Stellar node
./scripts/local-net.sh start

# Build and deploy all contracts
./scripts/deploy.sh local

# Deploy a single contract
./scripts/deploy.sh local token

# Start frontend dev server
npm run dev
```

---

## 4. Testnet Deployment

### Step 1: Ensure Your Identity is Funded

Verify your deployer account has sufficient XLM:

```bash
stellar account info --source-account deployer --network testnet
```

If the account shows 0 XLM, fund it via Friendbot:

```bash
stellar keys fund deployer --network testnet
```

### Step 2: Deploy Contracts

Deploy all contracts:

```bash
./scripts/deploy.sh testnet
```

Or use `./scripts/deploy-all.sh testnet` for the same full-workspace deploy with a
`contract -> contract ID` summary table printed at the end, and a fail-fast stop on
the first contract that errors.

Deploy a single contract:

```bash
./scripts/deploy.sh testnet token
# or
./scripts/deploy.sh testnet escrow
```

**Expected output:**
```
Building token contract...
Deploying token contract to testnet...
Contract ID: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
Admin: GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

### Step 3: Save Contract IDs

Save the contract IDs printed to stdout — you'll need them in `.env`:

```env
VITE_TOKEN_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
VITE_ESCROW_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

### Step 4: Verify Deployment

Verify the contract is deployed and responsive:

```bash
stellar contract info --id <CONTRACT_ID> --network testnet
```

**Expected output:**
```
Contract ID: CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
WASM Hash: XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

---

## 5. Post-Deploy Initialization

After deploying contracts, run the initialization script to call each contract's `initialize` function:

```bash
# Populate .contract-ids with deployed IDs (format written by deploy.sh: name: CONTRACT_ID)
echo "token: CABC..." >> .contract-ids
echo "escrow: CDEF..." >> .contract-ids

# Initialize all contracts on the current network
./scripts/initialize.sh testnet   # or local / mainnet
```

**Environment variable overrides:**

| Variable | Default | Description |
|----------|---------|-------------|
| `INIT_FN` | `initialize` | Function name to invoke |
| `SOURCE_ACCOUNT` | `default` | Stellar key to sign transactions |
| `CONTRACT_IDS_FILE` | `.contract-ids` | Path to contract ID list |
| `INIT_ARGS_<NAME>` | _(none)_ | Extra CLI args for a specific contract, e.g. `INIT_ARGS_TOKEN="--admin GABC..."` |

The script is idempotent: contracts that return an "already initialized" error are reported as skipped rather than failures.

---

## 6. Mainnet Deployment

> ⚠️ Mainnet deployments are irreversible. Complete testnet validation first.

```bash
# Ensure deployer account has sufficient XLM for fees
stellar keys generate --global deployer-mainnet

# Dry-run simulation (no broadcast)
stellar contract deploy --wasm <path>.wasm \
  --rpc-url https://soroban.stellar.org \
  --network-passphrase "Public Global Stellar Network ; September 2015" \
  --source-account deployer-mainnet \
  --simulate-only

# Deploy
./scripts/deploy.sh mainnet
```

---

## 7. Frontend Deployment

### Build

```bash
npm run build          # outputs to dist/
```

### Docker

```bash
docker build -f docker/Dockerfile --target prod -t soroban-kit:prod .
docker run -p 80:80 soroban-kit:prod
```

### Docker Compose (full stack)

```bash
docker compose -f docker/docker-compose.yml up --build
```

### Static Hosting (Netlify / Vercel / S3)

```bash
npm run build
# Upload dist/ to your static host
# Set environment variables in the host's dashboard (not in .env)
```

---

## 8. CI/CD Pipeline

The GitHub Actions workflow at `.github/workflows/ci.yml` runs on every push:

1. `npm ci` — install dependencies
2. `npm run lint` — ESLint checks
3. `npm test` — Vitest unit tests
4. `cargo test` — Rust contract tests
5. `stellar contract build` — WASM compilation check

To add automated deployment, extend `.github/workflows/ci.yml`:

```yaml
- name: Deploy to testnet
  if: github.ref == 'refs/heads/main'
  run: ./scripts/deploy.sh testnet
  env:
    STELLAR_SECRET_KEY: ${{ secrets.STELLAR_SECRET_KEY }}
```

### Structured deployment logs (JSON)

`deploy.sh`, `deploy_staking.sh`, and `deploy_vesting.sh` emit one structured
JSON log line per deploy event on **stdout**, while all human-readable progress
goes to **stderr**. Each line has the shape:

```json
{"timestamp":"2026-06-28T12:00:00Z","network":"testnet","contract":"escrow","contractId":"C...","txHash":"","status":"deployed"}
```

`status` is one of `building`, `deployed`, or `failed`. Because stdout is a clean
NDJSON stream, you can pipe it straight into a log aggregator or filter it with
`jq`:

```bash
# Pretty-print only successful deployments
./scripts/deploy.sh testnet 2>/dev/null | jq -c 'select(.status == "deployed")'

# Persist a deploy ledger and ship lines to a collector at the same time
./scripts/deploy.sh testnet 2>deploy.stderr.log \
  | tee -a deploys.ndjson \
  | curl -s -H 'Content-Type: application/x-ndjson' --data-binary @- \
      https://logs.example.com/ingest
```

---

## 9. Automated Guide Generation

Run the docs check script to validate documentation coverage and regenerate the docs report:

```bash
node scripts/check-docs.mjs
# Outputs: docs-report.json
```

To regenerate environment-specific config snippets automatically:

```bash
node scripts/generate-guides.mjs
# Outputs: docs/integration-guide.md, docs/deployment-guide.md
```

---

## 10. Validation Checklist

### Pre-deployment

- [ ] All contract tests pass: `cargo test`
- [ ] Frontend tests pass: `npm test`
- [ ] Lint passes: `npm run lint`
- [ ] `.env` is configured for the target network
- [ ] Deployer account is funded
- [ ] Contract IDs from previous deployments are recorded

### Post-deployment

- [ ] **Contract health verified** — run `./scripts/check-contract-ids.sh` (see §15)
- [ ] Contract responds to `simulate` calls
- [ ] Frontend connects to the correct RPC endpoint
- [ ] Admin functions are restricted to the correct address
- [ ] Event emission verified via Stellar Laboratory
- [ ] Contract ID saved to `.env` and committed (not the secret key)

---

## 11. Security Considerations

- Store `STELLAR_SECRET_KEY` only in CI secrets, never in `.env` committed to git
- Use separate deployer accounts per environment (local / testnet / mainnet)
- Verify WASM hash after deployment: `stellar contract info --id <CONTRACT_ID>`
- Enable `set_admin` only during initialization; lock it down immediately after
- Audit contract code with `cargo audit` before mainnet deployment:
  ```bash
  cargo install cargo-audit
  cargo audit
  ```
- Use `--simulate-only` to estimate fees before broadcasting

---

## 12. Performance Optimization

- Build contracts with `--release` flag (default in `deploy.sh`)
- Minimize contract storage reads — cache values in `Env::storage().instance()`
- Use `TTL` extensions for long-lived contract data to avoid expiry
- Frontend: set `Cache-Control: max-age=31536000` for hashed static assets

### WASM Binary Size Optimisation with wasm-opt

The build pipeline already sets `opt-level = "z"` and `lto = true` in the
release profile. Running `wasm-opt -Oz` (from [Binaryen](https://github.com/WebAssembly/binaryen))
as a post-build step typically reduces WASM size by an additional **10–30%**,
lowering upload fees and ledger footprint.

#### Install Binaryen

```bash
# macOS
brew install binaryen

# Ubuntu / Debian
apt-get install binaryen

# From source / pre-built release
# https://github.com/WebAssembly/binaryen/releases
```

Verify the install:

```bash
wasm-opt --version
```

#### Run manually

```bash
# Optimise a single WASM file in-place
wasm-opt -Oz --output optimized.wasm input.wasm

# Check size reduction
ls -lh input.wasm optimized.wasm
```

#### Automated via deploy.sh

`scripts/deploy.sh` automatically runs `wasm-opt -Oz` on each compiled WASM
after building, if `wasm-opt` is present on `$PATH`. It prints before/after
sizes for every file:

```
── Building escrow ──
  wasm-opt: 84320B → 61440B (-27%)
── Deploying escrow to testnet ──
```

If `wasm-opt` is not installed, the script skips optimisation with an `INFO`
message — the build still succeeds.

#### Automated via Docker

`docker/Dockerfile.contracts` installs `binaryen` in the build stage and runs
`wasm-opt -Oz` on every WASM artifact before copying them to the runtime stage.
Size comparisons are printed during the Docker build:

```
=== wasm-opt size reduction ===
  soroban_escrow_template.wasm: 84320B → 61440B (-27%)
  soroban_token_template.wasm:  72800B → 54200B (-25%)
```

#### CI size comparison

To track WASM sizes across PRs, add a step to your CI workflow:

```yaml
- name: Report WASM sizes
  run: |
    echo "| Contract | Size |" >> $GITHUB_STEP_SUMMARY
    echo "|----------|------|" >> $GITHUB_STEP_SUMMARY
    find target/wasm32-unknown-unknown/release -maxdepth 1 -name "*.wasm" | sort | \
      while read f; do
        echo "| $(basename $f) | $(stat -c%s $f)B |" >> $GITHUB_STEP_SUMMARY
      done
```

---

## 13. Contract Upgrades (Timelock)

Both the Token and Escrow contracts enforce a **two-step upgrade process** when
built with the `upgradeable` / `pausable` feature flags. A minimum delay of
`UPGRADE_DELAY_LEDGERS` (17 280 ledgers ≈ 24 hours at 5 s/ledger) is enforced
between proposing and executing a WASM upgrade.

### Step 1 — Propose the upgrade

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network testnet \
  -- propose_upgrade \
  --wasm_hash <NEW_WASM_HASH>
```

This stores the hash and a `ready_after` ledger number on-chain and emits an
`upgrade_proposed` event. Announce the upgrade publicly so users have time to
review and exit if needed.

### Step 2 — Execute the upgrade (after the timelock)

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network testnet \
  -- execute_upgrade
```

The call will fail with `NotAuthorized` / `Unauthorized` if the current ledger
is still before `ready_after`. Once executed, an `upgrade_executed` event is
emitted and the WASM is replaced atomically.

### Verify the new WASM hash

```bash
stellar contract info --id <CONTRACT_ID> --network testnet
```

Compare the reported WASM hash against the expected value before announcing the
upgrade as complete.

### Security notes

- Never skip the timelock on mainnet. The delay gives users time to react.
- Gate `propose_upgrade` behind a multi-sig or governance vote for production.
- Rehearse the full upgrade flow on testnet before executing on mainnet.

---

## 14. Troubleshooting

### Identity and Funding Issues

| Problem | Solution |
|---------|----------|
| `stellar: command not found` | Install Stellar CLI: `cargo install --locked stellar-cli --features opt` |
| `Identity "deployer" not found` | Generate identity: `stellar keys generate --global deployer` |
| `Account not found` | Fund account via Friendbot: `stellar keys fund deployer --network testnet` |
| `Insufficient balance for fees` | Verify funding: `stellar account info --source-account deployer --network testnet` |
| `Friendbot rate limit exceeded` | Wait 5 minutes and retry, or use a different account |
| `Invalid network passphrase` | Ensure `.env` has correct `VITE_NETWORK_PASSPHRASE` for the target network |

### Deployment Issues

| Problem | Solution |
|---------|----------|
| `wasm32` target missing | `rustup target add wasm32-unknown-unknown` |
| Deploy fails: insufficient fee | Increase fee in `deploy.sh` or fund account |
| Local node unhealthy | `./scripts/local-net.sh reset` then retry |
| Contract already initialized | Deploy a fresh contract; initialization is one-time |
| Frontend shows wrong network | Check `VITE_STELLAR_NETWORK` in `.env` |
| CORS errors from RPC | Use a proxy or the official RPC endpoints |

---

## 15. Post-Deployment Contract Verification

After deploying, confirm that every contract in `.contract-ids` is alive:

```bash
./scripts/check-contract-ids.sh
```

The script reads `.contract-ids` (format: `name: <CONTRACT_ID>`, written by `deploy.sh`), invokes `contract_version`
on each contract, and categorises results:

| Status | Meaning |
|--------|---------|
| **ALIVE** | Contract responded normally |
| **EXPIRED TTL** | Entry has expired; extend TTL with `stellar contract extend` |
| **UNREACHABLE** | Contract not found or RPC error; re-deploy if necessary |

Override the default file or network:

```bash
./scripts/check-contract-ids.sh .contract-ids.testnet
STELLAR_NETWORK=mainnet ./scripts/check-contract-ids.sh
```

The script exits non-zero if any contract is expired or unreachable, making it
suitable for use in CI/CD pipelines.

---

## 16. Escrow Status Monitoring

After deploying an escrow contract, use `scripts/monitor-escrow.sh` to check its current state at any time:

```bash
./scripts/monitor-escrow.sh [network] <CONTRACT_ID>
```

### Example

```bash
./scripts/monitor-escrow.sh testnet CABC123...

# Output:
# Escrow Monitor — CABC123...
# Network: testnet  RPC: https://soroban-testnet.stellar.org
# ────────────────────────────────────────────────────
# State:          Funded
#
# Buyer:          GABC...
# Seller:         GDEF...
# Arbiter:        GHIJ...
#
# Amount:         1000000000 (base units)
# Deadline:       ledger 5432100
# Time remaining: 2d 3h 45m remaining
# ────────────────────────────────────────────────────
```

### State color coding

| Color | States |
|-------|--------|
| Green | `Completed` |
| Yellow | `Funded`, `Delivered` |
| Red | `Disputed`, `Cancelled` |
| Cyan | `Created`, `Refunded` |

The script also prints a warning when the deadline has passed and funds are still locked.

---

## 17. Monitoring Token Contract Status

Use `scripts/monitor-token.sh` to quickly inspect a deployed token contract:

```bash
TOKEN_CONTRACT_ID=<CONTRACT_ID> ./scripts/monitor-token.sh testnet
```

**Output:**

```
==============================
 Token Contract Monitor
 Network : testnet
 Contract: C...
==============================
Admin:         G...
Total Supply:  1000000
Max Supply:    uncapped
Paused:        false
Version:       abc1234
==============================
```

| Field | Source |
|-------|--------|
| Admin | `admin()` |
| Total Supply | `total_supply()` |
| Max Supply | `max_supply()` — `uncapped` if `capped-supply` feature is not enabled |
| Paused | Detected via simulate-only call; `n/a` if `pausable` feature is not enabled |
| Version | `version()` — git commit hash baked in at compile time |

Set `TOKEN_CONTRACT_ID` as an environment variable or export it in your shell profile for repeated use:

```bash
export TOKEN_CONTRACT_ID=<CONTRACT_ID>
./scripts/monitor-token.sh mainnet
```

---

## 18. Cargo Feature Defaults

`scripts/deploy.sh` and `scripts/deploy-all.sh` run a bare `stellar contract
build` with no `--features` flag, so a contract's compiled-in capabilities are
whatever its Cargo.toml `[features] default = [...]` declares. The table
below is what each contract ships with when deployed the standard way:

| Contract | Default features | What they enable |
|----------|-------------------|-------------------|
| `token` | `pausable`, `upgradeable`, `capped-supply`, `freeze`, `transfer-hook` | `pause()`/`unpause()`; two-step `propose_upgrade()`/`execute_upgrade()`; a `max_supply` cap enforced on mint; `freeze_account()`/`unfreeze_account()`; an optional `on_transfer` hook contract |
| `escrow` | `pausable` | `pause()`/`unpause()` and the same `propose_upgrade()`/`execute_upgrade()` timelock pair — both currently live under the single `pausable` gate in `contracts/escrow/src/lib.rs`, not a separate `upgradeable` gate |
| `wrapped-token` | `pausable` | `pause()`/`unpause()` (closes #817) |
| `multisig` | `pausable`, `upgradeable` | Neither currently guards any code — no pause or upgrade entry points exist yet in `contracts/multisig`. Both are declared for parity with the other contracts and defaulted on so they take effect automatically once implemented |

To opt out of a default for a given deployment, build with
`--no-default-features --features <subset>` before running `deploy.sh`, or
export a modified `Cargo.toml` for that build.

`.github/workflows/ci.yml`'s `audit` job runs
`.github/scripts/check_feature_exports.py` after building every contract's
release WASM. It parses each WASM's export section directly (no `wasm-tools`
dependency) and fails CI if an expected feature-gated function — e.g.
`pause`, `execute_upgrade`, `max_supply` — is missing, so a default silently
regressing to off is caught before it reaches a deploy.

---

## Resources

- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/stellar-cli)
- [Soroban Deployment Docs](https://soroban.stellar.org/docs/getting-started/deploy-to-testnet)
- [Stellar Friendbot (testnet funding)](https://friendbot.stellar.org)
- [Stellar Laboratory](https://laboratory.stellar.org/)
- [cargo-audit](https://crates.io/crates/cargo-audit)
- [Contract Monitoring Guide](./monitoring.md) — event indexing, anomaly detection, Prometheus alerts, and a sample Grafana dashboard for deployed contracts
