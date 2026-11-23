# Testing Strategy

This document describes how the Soroban contracts in this workspace are tested,
what each layer of the test pyramid covers, and where the remaining gaps are.
It is generated against the contracts and test files present as of 2026-07-27.

## Test layers

1. **Unit tests** (`contracts/*/src/test.rs`) — exercise individual entry points
   and helper functions with example-based assertions.
2. **Property tests** (`contracts/*/src/prop_test.rs`, plus inline `proptest`
   usage in some `test.rs` files) — assert invariants over randomized inputs.
3. **Fuzz targets** (`fuzz/fuzz_targets/*.rs`) — drive entry points with
   arbitrary byte streams to catch panics and unexpected state transitions.
4. **Integration tests** (`tests/`) — exercise cross-contract flows end to end.

## Coverage Matrix

| Contract       | Unit | Property | Fuzz | Integration |
| -------------- | ---- | -------- | ---- | ----------- |
| airdrop        | ✅   | ❌       | ✅   | ❌          |
| bonding-curve  | ✅   | ✅       | ❌   | ❌          |
| escrow         | ✅   | ❌       | ✅   | ✅          |
| multisig       | ✅   | ❌       | ❌   | ❌          |
| oracle         | ✅   | ✅       | ❌   | ❌          |
| staking        | ✅   | ✅       | ❌   | ❌          |
| swap           | ✅   | ❌       | ✅   | ❌          |
| token          | ✅   | ❌       | ✅   | ✅          |
| wrapped-token  | ✅   | ❌       | ❌   | ❌          |

Fuzz targets currently present in `fuzz/fuzz_targets/`:

```
airdrop_merkle_proof.rs
escrow_initialize.rs
swap_state_machine.rs
token_fuzz.rs
token_mint_burn.rs
```

Property tests currently present:

```
contracts/bonding-curve/src/prop_test.rs
contracts/oracle/src/prop_test.rs
contracts/staking/src/test.rs   (inline proptest usage)
```

## Gaps

- **Fuzz coverage** is limited to `token`, `escrow`, `airdrop`, and `swap`.
  `bonding-curve`, `oracle`, `staking`, `multisig`, and `wrapped-token` have no
  fuzz targets yet.
- **Property coverage** is limited to `bonding-curve`, `oracle`, and `staking`.
  The remaining contracts — including `airdrop`, `escrow`, `multisig`, `swap`,
  `token`, and `wrapped-token` — currently rely on example-based unit tests
  only for their invariants.
- **Integration coverage** exists for `escrow` and `token` only; the other
  contracts are exercised solely through their own unit and property suites.
- **Thinnest unit suites** by `#[test]` function count are `bonding-curve` (3)
  and `multisig` (4). `wrapped-token` has 16 `#[test]` functions and is no
  longer among the thinnest suites in the workspace.

## Adding tests

When you add a new test file or a new fuzz target, add a row update here in the
same PR as the new tests so this matrix stays accurate.

## Mutation Testing

Mutation testing systematically introduces small code changes ("mutants") and
checks whether the test suite catches them. A mutant that is not caught by any
test is called a **survivor** — evidence of a gap in test coverage.

### Configuration

Mutation testing is configured in [`mutants.toml`](../mutants.toml) at the
workspace root. The file selects which source files to mutate and which to
exclude (test helpers, build scripts, property test files).

As of issue #1213, `mutants.toml` covers **all 19 contract packages**. Earlier
versions (tracked as #972) only covered `token` and `escrow`.

### Running Locally

```bash
# Install cargo-mutants (pin to a tested version)
cargo install cargo-mutants --locked

# Run against a single contract (fastest for iteration)
cargo mutants -p soroban-token-template --timeout 120

# Run against all contracts (slow — use CI shards for routine checks)
cargo mutants --workspace --timeout 120 --jobs 4

# Run against a specific file
cargo mutants --file contracts/bonding-curve/src/lib.rs --timeout 120
```

Results are written to `mutants.out/` (or `--output <dir>`):

```
mutants.out/
  caught/       # mutants killed by at least one test
  missed/       # survivors — review these
  timeout/      # mutants that caused test timeouts
  cargo-mutants.log
```

### CI Integration

The `mutation` job in `.github/workflows/ci.yml` runs mutation tests in four
parallel shards:

| Shard | Contracts |
|-------|-----------|
| `financial` | token, escrow, bonding-curve, subscription, wrapped-token |
| `defi` | staking, vesting, swap, timelock |
| `governance` | multisig, dao, ballot, crowdfund |
| `utility` | marketplace, auction, nft, lottery, oracle, airdrop |

The job is scheduled weekly (Sunday midnight UTC) and runs `continue-on-error: true`
on PRs so that survivors are visible without blocking CI. Artifacts are retained for
14 days.

### Triaging Survivors

A survived mutant means the test suite does not distinguish between the original code
and a modified version. Common causes:

1. **Dead code path** — the mutated branch is never exercised by any test. Add a test
   that reaches it.
2. **Equivalent mutant** — the mutation does not change observable behaviour (e.g.,
   changing `>` to `>=` in a case where the boundary value is never tested). Document
   and suppress with `// mutants::skip` if genuinely equivalent.
3. **Test asserts only on happy path** — add a negative test that verifies the error
   case the mutation removes.

To suppress a known-equivalent mutant, add `#[cfg_attr(mutants, allow(unused))]` or
a `// mutants::skip` comment on the specific line.

### Baseline Mutation Scores

These scores were established at the time of issue #1213 and represent the
**minimum acceptable** mutation score per contract. A PR that introduces new
logic should not reduce the score for the affected contract.

| Contract | Baseline Score | Tracked in |
|----------|:--------------:|------------|
| token | ≥ 85% | #972 |
| escrow | ≥ 80% | #972 |
| bonding-curve | TBD (first run) | #1213 |
| subscription | TBD (first run) | #1213 |
| wrapped-token | TBD (first run) | #1213 |
| staking | TBD (first run) | #1213 |
| vesting | TBD (first run) | #1213 |
| multisig | TBD (first run) | #1213 |
| dao | TBD (first run) | #1213 |
| all others | TBD (first run) | #1213 |

Update the table after the first CI run completes and survivors have been triaged.
