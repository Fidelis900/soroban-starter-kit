# Soroban Contract Templates

[![CI](https://github.com/Fidelis900/soroban-starter-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/Fidelis900/soroban-starter-kit/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Fidelis900/soroban-starter-kit/branch/main/graph/badge.svg)](https://codecov.io/gh/Fidelis900/soroban-starter-kit)
[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://codespaces.new/Fidelis900/soroban-starter-kit)

A curated collection of production-ready Soroban smart contract templates. These templates help developers quickly bootstrap common use cases on Soroban (Stellar's smart contract platform) for DeFi, payments, governance, and more.

**También disponible en**: [Español 🇪🇸](README.es.md) · **Disponible en**: [Français 🇫🇷](README.fr.md)

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/your-username/soroban-contract-templates.git
cd soroban-contract-templates

# Build all contracts
make build

# Run tests
make test

# Re-run tests automatically on file changes (requires cargo-watch)
make watch

# Deploy to testnet
make deploy-testnet

# See all available commands
make help
```

Or use `just` (see [dev-environment.md](docs/dev-environment.md) for installation):

```bash
just build
just test
just deploy-testnet
just --list
```

## 📦 Contract Templates

| Template | Description | Use Cases | Status |
|----------|-------------|-----------|---------|
| **Token** | Custom fungible token with mint/burn/admin controls | DeFi tokens, governance tokens, utility tokens | ✅ Complete |
| **Escrow** | Two-party escrow with timeout and refund mechanism | P2P trading, service payments, milestone payments | ✅ Complete |
| **Vesting** | Token vesting with cliff + linear release schedule | Team allocations, investor lockups, employee grants | ✅ Complete |
| **Staking** | Token staking with proportional reward distribution | DeFi yield, protocol incentives, liquidity mining | ✅ Complete |
| **Multisig** | N-of-M wallet for threshold-approved contract calls | DAO treasuries, team wallets, shared administration | ✅ Complete |
| **Subscription** | Recurring token-pull payment contract | SaaS billing, streaming payments, membership fees | ✅ Complete |
| **Timelock** | Time-locked token release to a beneficiary | Team token locks, delayed payments, governance timelocks | ✅ Complete |
| **NFT** | Non-fungible token with admin minting and optional supply cap | Digital collectibles, on-chain ownership, access tokens | ✅ Complete |
| **DAO** | On-chain governance with token-weighted voting | Protocol upgrades, treasury management, community decisions | ✅ Complete |
| **Swap** | Atomic two-party token swap with deadline | P2P token exchange, OTC trades, trustless DeFi swaps | ✅ Complete |
| **Oracle** | Price oracle consumer with staleness validation | DeFi price feeds, on-chain data consumption, freshness checks | ✅ Complete |
| **Lottery** | Verifiable on-chain lottery with commit-reveal randomness | Raffles, fair draws, decentralised prize distribution | ✅ Complete |

### Token Contract Features
- **Standard Interface**: Full Soroban token compatibility
- **Administrative Controls**: Mint, burn, and admin management
- **Metadata Support**: Name, symbol, and decimals
- **Allowance System**: Approve and transfer_from functionality
- **Event Emission**: All operations emit events for tracking
- **Error Handling**: Custom error types for better debugging

### Escrow Contract Features
- **Two-Party Security**: Secure buyer-seller transactions
- **Deadline Protection**: Automatic refunds after deadline
- **Arbiter Support**: Third-party dispute resolution
- **State Management**: Clear transaction lifecycle
- **Token Agnostic**: Works with any Soroban token
- **Event Emission**: All operations emit events for tracking

### Vesting Contract Features
- **Cliff + Linear Schedule**: Tokens unlock linearly between `cliff_ledger` and `end_ledger`
- **Admin Revocation**: Admin can cancel unvested tokens at any time; vested tokens remain claimable
- **Incremental Claims**: Beneficiary claims accrued tokens on demand
- **Token Agnostic**: Works with any Soroban-compatible token
- **Event Emission**: `initialized`, `claimed`, and `revoked` events for off-chain tracking
- **TTL Management**: Instance storage TTL is extended on every interaction

### Staking Contract Features
- **Proportional Rewards**: Rewards distributed pro-rata to each staker's share of the pool
- **Reward-Per-Token Accumulator**: Gas-efficient global accumulator pattern; no per-staker loops
- **Separate Stake / Reward Tokens**: Stake token and reward token can be the same or different
- **Admin Reward Deposits**: Admin calls `add_rewards` to top up the reward pool at any time
- **Incremental Claims**: Stakers call `claim_rewards` independently; rewards accrue continuously
- **Token Agnostic**: Works with any Soroban-compatible token
- **Event Emission**: `staked`, `unstaked`, `rewards_claimed`, and `rewards_added` events
- **TTL Management**: Instance storage TTL is extended on every interaction
### Subscription Contract Features
- **Provider-Initiated Charges**: Service provider pulls payments on a configurable ledger interval
- **Subscriber-Controlled Plans**: Subscribers set their own amount and interval; cancel at any time
- **Allowance-Based Pulls**: Uses token `approve` + `transfer_from` — no funds are locked up-front
- **Re-subscribe Support**: Cancelled subscribers can create a new plan without re-deploying
- **State Tracking**: Subscription state (active, last charged ledger) stored per subscriber
- **Event Emission**: `subscribed`, `charged`, and `cancelled` events for off-chain tracking
- **TTL Management**: Both instance and persistent storage entries are extended on each interaction

### Multisig Contract Features
- **N-of-M Authorization**: Configure any valid threshold across unique signers
- **Signer Management**: Add or remove signers with threshold-approved changes
- **Transaction Proposals**: Store target contract, function, and arguments
- **Signature Tracking**: Prevent duplicate signatures and non-signer approvals
- **Threshold Execution**: Execute proposed calls only after enough signatures
- **Event Emission**: Initialization, signer changes, signatures, and execution emit events

### Timelock Contract Features
- **Time-Locked Release**: Tokens held until a specified ledger sequence number, then released to the beneficiary
- **Admin Cancellation**: Admin can cancel and reclaim tokens at any time before release
- **Open Release**: Once the release ledger is reached, `release` is callable by anyone
- **Token Agnostic**: Works with any Soroban-compatible token
- **Event Emission**: `initialized`, `released`, and `cancelled` events for off-chain tracking
- **TTL Management**: Instance storage TTL is extended on every interaction

### NFT Contract Features
- **Unique Token Ownership**: Each token ID maps to exactly one owner tracked in persistent storage
- **Admin-Controlled Minting**: Only the admin may mint new tokens; optional supply cap enforced at mint time
- **Standard Operations**: `mint`, `transfer`, `burn`, `approve`, `transfer_from` matching ERC-721 semantics
- **Atomic Batch Transfer**: `batch_transfer` moves multiple tokens in one call with a single auth check; ownership of every token is validated before any transfer is applied
- **Per-Token Metadata**: Each token has an associated URI stored on-chain; collection has name and symbol
- **Approval System**: Single-token approvals cleared automatically on transfer or burn
- **Property Tests**: Proptest suite verifies supply invariants and ownership correctness
- **Event Emission**: `minted`, `transferred`, `burned`, and `approved` events

### DAO Contract Features
- **Token-Weighted Voting**: Voting power equals the voter's token balance at vote time
- **Configurable Parameters**: Voting period (in ledgers) and quorum threshold set at initialization
- **Proposal Lifecycle**: `Active → Executed` (passes) or `Active → Cancelled` (admin)
- **Quorum + Majority**: Proposals execute only when total votes ≥ quorum AND yes > no
- **Double-Vote Prevention**: Each address may vote exactly once per proposal
- **Event Emission**: `proposal_created`, `voted`, `prop_executed`, and `prop_cancelled` events
- **TTL Management**: Persistent proposal and vote records are bumped on every write

### Swap Contract Features
- **Atomic Exchange**: Both token transfers occur in a single transaction — no partial fills
- **Deadline-Based Expiry**: Swaps expire after a configurable ledger; anyone may cancel to recover party A's tokens
- **Party A Control**: Party A can cancel any open swap before it is accepted
- **Multi-Swap Support**: Multiple concurrent swaps tracked by auto-incrementing IDs
- **Token Agnostic**: Works with any pair of Soroban-compatible tokens
- **Event Emission**: `swap_proposed`, `swap_accepted`, and `swap_cancelled` events

### Oracle Contract Features
- **Price Feed Consumer**: Admin pushes price updates; consumers read via `get_price`
- **Staleness Validation**: `get_price` rejects prices older than the configured ledger threshold
- **Admin-Controlled Updates**: Only the admin may push new prices
- **Configurable Threshold**: Staleness threshold set at initialization
- **Event Emission**: `initialized` and `price_updated` events
- **TTL Management**: Instance storage TTL is extended on every interaction

### Lottery Contract Features
- **Commit-Reveal Randomness**: Admin commits `hash(secret ++ salt)` before the draw, then reveals to prove fairness
- **Ticket Purchase**: Any address buys tickets before the admin commits
- **Verifiable Winner Selection**: Winner index derived from SHA-256 of revealed secret, salt, and ledger sequence
- **Prize Pool Distribution**: Full ticket pool transferred to winner atomically
- **State Machine**: Open → Committed → Drawn — each transition is irreversible
- **Event Emission**: `initialized`, `ticket_purchased`, `committed`, and `winner_drawn` events

Each template includes:
- ✅ Complete contract implementation
- ✅ Comprehensive unit tests (8+ test cases each)
- ✅ Deployment scripts with examples
- ✅ Usage examples and documentation

## 🛠 Prerequisites

- [Rust](https://rustup.rs/) **1.82.0** (pinned via `rust-toolchain.toml` — `rustup` picks this up automatically)
- [Soroban CLI](https://soroban.stellar.org/docs/getting-started/setup#install-the-soroban-cli)
- [Docker](https://www.docker.com/) (for local Stellar node)

> **Zero-install option:** Open this repo in a pre-configured environment with all tools ready — see the [Dev Container & Codespaces Guide](docs/devcontainer.md).

### VS Code Setup

A [.vscode/extensions.json](.vscode/extensions.json) file is included with recommended extensions for a consistent developer experience. VS Code will prompt you to install them when you open the project, or you can install them manually:

| Extension | Purpose |
|-----------|---------|
| `rust-lang.rust-analyzer` | Rust language server (completions, go-to-def, inline errors) |
| `tamasfe.even-better-toml` | Syntax highlighting and validation for `Cargo.toml` |
| `serayuzgur.crates` | Inline crate version hints and outdated-dependency warnings |
| `usernamehw.errorlens` | Inline diagnostic messages directly in the editor |

## 🔄 Compatibility Matrix

This repository is pinned to `soroban-sdk = "=21.7.7"`. Each major SDK version is tightly coupled to a Stellar network protocol version. Use the table below to pick the right SDK for your target network.

> ⚠️ **Always verify compatibility before deploying to Mainnet.** Contracts compiled against SDK v21 will not work on a node running Protocol 22 or later without recompilation against the matching SDK version.

| soroban-sdk version | Stellar Protocol | Network status | Notes |
|---------------------|-----------------|----------------|-------|
| `21.x` (this repo: `21.7.7`) | Protocol 21 | Mainnet (Jun 2024) | secp256r1, separate TTL extension for instance/code |
| `22.x` | Protocol 22 | Mainnet (Dec 2024) | Constructor support, BLS12-381 host functions |
| `23.x` | Protocol 23 "Whisk" | Mainnet (Sep 2025) | Unified events (CAP-67), state archival (CAP-62/66) |
| `25.x` | Protocol 25 "X-Ray" | Mainnet (Jan 2026) | BN254 elliptic curve ops, Poseidon hash functions |
| `26.x` | Protocol 26 "Yardstick" | Mainnet (May 2026) | Freeze ledger entries, muxed address conversions, ZK BN254 |

**To upgrade this repository to a newer SDK:**
1. Update `soroban-sdk = "=<new-version>"` in `Cargo.toml`.
2. Update `stellar-cli` to the matching version (`cargo install stellar-cli --version <new-version>`).
3. Rebuild all contracts and run the full test suite.
4. Update this matrix and `docs/gas-costs.md` with the new protocol version and fee schedule.

For the authoritative version table, see [Stellar Software Versions](https://developers.stellar.org/docs/networks/software-versions).

## 📖 Usage

### Building Contracts

```bash
cd contracts/[template-name]
stellar contract build
```

### Running Tests

```bash
cd contracts/[template-name]
cargo test
```

### Deploying to Testnet

```bash
cd contracts/[template-name]
./scripts/deploy.sh testnet
```

### Local Development

Start a local Stellar node with Soroban RPC:

```bash
docker compose up stellar-node
```

### Creating a New Contract

Scaffold a fresh contract from the `contracts/common` skeleton with a single command:

```bash
./scripts/new-contract.sh <contract-name>
```

**Example** — create a `price-feed` contract:

```bash
./scripts/new-contract.sh price-feed
```

This creates `contracts/price-feed/` (with `Cargo.toml` and `src/lib.rs`) and registers it in the workspace. Verify it compiles and its test passes immediately:

```bash
cargo check -p soroban-price-feed
cargo test  -p soroban-price-feed
```

Then replace the stub `hello()` method in `contracts/price-feed/src/lib.rs` with your contract logic.

## ⚠️ Error Reference

> For full details — causes, triggers, and resolution steps — see [docs/error-reference.md](docs/error-reference.md).

### Token Contract Errors (`TokenError`)

| Code | Name | Description |
|------|------|-------------|
| 1 | `InsufficientBalance` | Caller's balance is too low to complete the transfer or burn |
| 2 | `InsufficientAllowance` | Approved allowance is too low for the requested `transfer_from` amount |
| 3 | `Unauthorized` | Caller is not the admin or does not have permission for this operation |
| 4 | `AlreadyInitialized` | `initialize` was called on a contract that has already been set up |
| 5 | `NotInitialized` | An operation was attempted before the contract was initialized |
| 6 | `InvalidAmount` | Amount is zero, negative, or exceeds the configured max supply |
| 7 | `Overflow` | Arithmetic overflow occurred during a balance or supply calculation |

### Escrow Contract Errors (`EscrowError`)

| Code | Name | Description |
|------|------|-------------|
| 1 | `NotAuthorized` | Caller is not permitted to invoke this function (wrong party or arbiter) |
| 2 | `InvalidState` | The escrow is not in the required state for this operation |
| 3 | `DeadlinePassed` | The escrow deadline has already elapsed; the operation is no longer valid |
| 4 | `DeadlineNotReached` | The deadline has not yet passed; premature refund or timeout claim attempted |
| 5 | `AlreadyInitialized` | `initialize` was called on an escrow that is already set up |
| 6 | `NotInitialized` | An operation was attempted before the escrow was initialized |
| 7 | `InsufficientFunds` | The buyer's token balance is too low to cover the escrowed amount |
| 8 | `InvalidAmount` | The specified amount is zero or otherwise invalid |
| 9 | `InvalidParties` | Buyer, seller, or arbiter addresses are invalid or conflict with each other |

### Vesting Contract Errors (`VestingError`)

| Code | Name | Description |
|------|------|-------------|
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up |
| 2 | `NotInitialized` | An operation was attempted before the contract was initialized |
| 3 | `Unauthorized` | Caller is not the admin |
| 4 | `InvalidAmount` | The vesting amount is zero or negative |
| 5 | `InvalidSchedule` | `cliff_ledger` >= `end_ledger`, or `end_ledger` is in the past |
| 6 | `NothingToClaim` | No tokens have vested since the last claim (or vested amount is zero) |
| 7 | `AlreadyRevoked` | `revoke` was called on a schedule that has already been revoked |

### Staking Contract Errors (`StakingError`)

| Code | Name | Description |
|------|------|-------------|
| 1 | `AlreadyInitialized` | `initialize` was called on a contract that is already set up |
| 2 | `NotInitialized` | An operation was attempted before the contract was initialized |
| 3 | `Unauthorized` | Caller is not the admin |
| 4 | `InvalidAmount` | Amount is zero or negative |
| 5 | `NoStake` | Staker has no stake to unstake or claim from |
| 6 | `InsufficientStake` | Requested unstake amount exceeds the staker's current stake |
| 7 | `NoRewards` | No rewards are available to claim |
### Multisig Contract Errors (`MultisigError`)

| Code | Name | Description |
|------|------|-------------|
| 1 | `AlreadyInitialized` | `initialize` was called after the signer set was already configured |
| 2 | `NotInitialized` | An operation was attempted before the multisig was initialized |
| 3 | `InvalidThreshold` | Threshold is zero or greater than the number of signers |
| 4 | `InvalidSigners` | Signer or approval lists are empty or contain duplicates |
| 5 | `NotSigner` | Caller, approver, or signer is not part of the wallet signer set |
| 6 | `TransactionNotFound` | Requested transaction ID does not exist |
| 7 | `AlreadyExecuted` | Transaction has already been executed |
| 8 | `AlreadySigned` | Signer already approved the transaction |
| 9 | `ThresholdNotMet` | Transaction does not have enough signatures to execute |
| 10 | `InsufficientApprovals` | Signer-management change lacks enough threshold approvals |
## 📂 Examples

End-to-end working examples are provided in the `examples/` directory:

| Example | Description |
|---------|-------------|
| [`examples/typescript/index.js`](examples/typescript/index.js) | Node.js script — deploys token, mints to buyer, runs full escrow lifecycle |
| [`examples/shell/run.sh`](examples/shell/run.sh) | Equivalent shell script using the Stellar CLI |

Both examples target a local Stellar node. Start one with `./scripts/local-net.sh start` before running.

### TypeScript

```bash
npm install @stellar/stellar-sdk
TOKEN_CONTRACT_ID=<id> ESCROW_CONTRACT_ID=<id> node examples/typescript/index.js
```

### Shell

```bash
./examples/shell/run.sh
```

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, test commands, code style, and the PR process.

A special thanks to all our [contributors](CONTRIBUTORS.md) who have helped make this project better!

## 📚 Resources

- [Directory Tree](docs/directory-tree.md) — Purpose of every directory in the repository
- [Glossary](docs/glossary.md) — Soroban/Stellar terms used across the repo
- [Threat Model](docs/threat-model.md) — Trust assumptions and attack surface per contract
- [Known Issues](docs/known-issues.md) — Accepted design tradeoffs per contract
- [Translations](docs/translations.md) — Source-commit checklist for `README.es.md` / `README.fr.md`
- [FAQ](docs/faq.md) — Common developer questions: setup, testing, deployment, feature flags, token customization
- [System Architecture](docs/architecture.md) — High-level design, contract relationships, storage tiers, event model, and admin framework
- [Contract API Reference](docs/contract-api.md) — Full public API for all contracts (parameters, return types, errors)
- [Upgrade Guide](docs/upgrade-guide.md) — Step-by-step on-chain WASM upgrade with timelock and key rotation
- [Upgrade Compatibility Matrix](docs/upgrade-compatibility-matrix.md) — Storage layouts and storage-breaking change categories per contract
- [Access-Control Reference](docs/access-control-reference.md) — Entry point -> role `require_auth` table for every contract
- [Security Best Practices](docs/security.md)
- [Integration Guide](docs/integration-guide.md)
- [Deployment Guide](docs/deployment-guide.md)
- [Testing Strategy](docs/testing-strategy.md) — Coverage matrix (unit/property/fuzz/integration) by contract
- [Gas / Compute Unit Costs](docs/gas-costs.md) — Measured and estimated CU costs per contract function
- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Stellar Developer Discord](https://discord.gg/stellardev)
- [Soroban Examples](https://github.com/stellar/soroban-examples)
- [Freighter Wallet](https://freighter.app/)
- [Stellar Laboratory](https://laboratory.stellar.org/)
- [Security Best Practices](docs/security.md)
- [Token Interface Compliance ADR](docs/adr/0007-token-interface-compliance.md)
- [Batch Mint Design ADR](docs/adr/0009-batch-mint-design.md)
- [Architecture Decision Records](docs/adr/README.md)

## 📄 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

---

**Ready to build on Soroban?** Start with any template and customize it for your use case! 🚀

## Handsoff notes

<!-- handsoff-issue-1025 -->
- #1025: Root Cargo.toml lists "contracts/ballot" as a workspace member twice
<!-- handsoff-issue-1030 -->
- #1030: CONTRIBUTING.md contains two entire, inconsistent contributing guides concatenated together — the first ~220 lines are an orphaned pre-19-contract-era version
<!-- handsoff-issue-1119 -->
- #1119: contracts/multisig: Off-chain EIP-712 / SEP-format typed signature batch verification

<!-- handsoff-issue-1132 -->
- #1132: contracts/staking: Time-weighted lockup boosting for staking rewards
