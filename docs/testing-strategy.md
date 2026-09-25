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
