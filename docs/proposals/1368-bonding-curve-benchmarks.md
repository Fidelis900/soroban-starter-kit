# #1368: Bonding Curve Criterion Benchmarks

_Status: Proposed · Tracking issue: #1368_

## Goal

Measure the Soroban CPU-instruction cost of the bonding curve's price math and
its `buy` / `sell` entry points, and record the results in
[gas-costs.md](../gas-costs.md).

## Current state (verify before implementing)

The issue assumes the contract has iterative fixed-point helpers
(`fixed_pow`, `fixed_root`) and three curve shapes (power, linear, root). **The
code in `contracts/bonding-curve/src/lib.rs` does not have these today:**

| Issue assumes | What the code has |
|---|---|
| `fixed_pow`, `fixed_root` | Not present |
| Power, linear and root curves | Linear only: `price = reserve * PRICE_SCALE / (supply + 1)` |
| Iterative math | Closed form: `buy_cost` / `sell_proceeds` average the price at the old and new supply |

The private helpers are `calculate_price`, `buy_cost` and `sell_proceeds`. None
of them are `pub`, so a benchmark crate cannot call them directly.

That leaves two ways to scope the work:

1. **Benchmark what exists (recommended first PR).** Bench `buy`, `sell`,
   `get_price` and the price math through the public contract API, using the
   linear curve only.
2. **Add the curves first, then bench them.** Add a `CurveType`
   (`Linear | Power { exp } | Root { n }`) plus `fixed_pow` / `fixed_root`
   helpers in a separate PR, then extend this benchmark file to cover all three.
   Doing this is a contract feature, not benchmark work, and needs its own
   review for overflow and precision.

The rest of this document covers option 1. Option 2 adds rows to the same tables.

## Measuring CPU instructions, not wall time

The existing benches (`escrow_ops.rs`, `token_ops.rs`) use
`c.bench_function`, which measures **wall-clock time**. Wall time is noisy on CI
runners and is not what the network charges for. For this file, also read the
host budget, which is deterministic:

```rust
env.cost_estimate().budget().reset_default();
client.buy(&buyer, &amount, &i128::MAX);
let cpu = env.cost_estimate().budget().cpu_instruction_cost();
let mem = env.cost_estimate().budget().memory_bytes_cost();
```

Use Criterion for the timing view and print the budget numbers (or emit them
as JSON; see [#1369](1369-bench-regression-comparison.md)) for the
instruction view.

## Implementation

### 1. Register the bench

`benches/Cargo.toml`:

```toml
[[bench]]
name = "bonding_curve_ops"
harness = false

[dev-dependencies]
soroban-bonding-curve-template = { path = "../contracts/bonding-curve" }
```

(Check the package name in `contracts/bonding-curve/Cargo.toml`.)

Add `"bonding_curve_ops contract-benchmarks"` to the `BENCHES` array in
`scripts/bench-all.sh`.

### 2. `benches/benches/bonding_curve_ops.rs`

Follow the structure of `escrow_ops.rs`: a `setup()` helper that leaks an
`Env`, calls `mock_all_auths()`, deploys a token plus `BondingCurveContract`,
and seeds the reserve and supply to a given state.

| Benchmark id | What it measures | Parameters |
|---|---|---|
| `bonding_curve::get_price/{supply}` | `calculate_price` via `get_price` | supply ∈ {0, 1e3, 1e6, 1e9} |
| `bonding_curve::buy/{amount}` | `buy_cost` plus token transfer and storage | amount ∈ {1, 100, 10 000} at a fixed supply |
| `bonding_curve::sell/{amount}` | `sell_proceeds` plus token transfer and storage | same as `buy` |
| `bonding_curve::buy_sell_roundtrip` | buy then sell the same amount | n/a |

Use `BenchmarkGroup` with `bench_with_input` so supply and amount appear as
separate series in the HTML report.

The supply sweep matters even for the linear curve: it shows whether cost is
flat across supply levels (as expected for closed-form math), which is the
baseline the iterative curves in option 2 must be compared against.

### 3. Document the results

Add a `## Bonding Curve Contract` section to [gas-costs.md](../gas-costs.md) in
the same format as the Token and Escrow tables:

| Function | Measured CPU instructions | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `get_price` | _measured_ | 2 reads (instance) | 0 | Read-only |
| `buy` | _measured_ | reads + writes (instance) | 1 (buyer → contract) | |
| `sell` | _measured_ | reads + writes (instance) | 1 (contract → seller) | |

Record the SDK version and the date measured, as the page header already does.

## Running

```bash
cargo bench -p contract-benchmarks --bench bonding_curve_ops
# or everything
./scripts/bench-all.sh
```

## Acceptance criteria

- [ ] `benches/benches/bonding_curve_ops.rs` exists and is registered in `benches/Cargo.toml` and `scripts/bench-all.sh`
- [ ] `buy`, `sell` and price calculation are profiled (`fixed_pow` / `fixed_root` too, once option 2 lands)
- [ ] CPU instruction counts are recorded in [gas-costs.md](../gas-costs.md)
