# #1369: Benchmark Regression Comparison Against `main`

_Status: Proposed · Tracking issue: #1369_

## Goal

Add a script, `scripts/compare-benches.sh`, that benchmarks the current branch
and `origin/main`, prints a markdown table of CPU-instruction deltas, and exits
non-zero if any core operation regresses by more than 10%.

## What already exists

`.github/workflows/bench.yml` already compares against a baseline and fails at
10%, but:

- it compares **wall-clock nanoseconds** parsed from Criterion's `time:` lines,
  which vary between CI runners;
- the baseline is a `bench-baseline` artifact uploaded on pushes to `main`, so
  it is only as fresh as the last `main` run and was built on a different
  machine;
- the logic is inline Python in the workflow, so it can't be run locally.

This script moves the comparison into the repo, runs both sides on the same
machine, and compares **CPU instructions**, which are deterministic for a given
SDK version. The output does not depend on runner speed.

## Prerequisite: emit instruction counts

Criterion does not report Soroban CPU instructions. Each bench file needs to
write them out next to the timing numbers. Add a small helper (for example
`benches/src/cost.rs`, or a module shared by the bench files) that each
benchmark calls once outside the timed loop:

```rust
pub fn record(name: &str, env: &Env) {
    let b = env.cost_estimate().budget();
    let line = format!(
        "{{\"name\":\"{name}\",\"cpu\":{},\"mem\":{}}}\n",
        b.cpu_instruction_cost(),
        b.memory_bytes_cost()
    );
    // append to $BENCH_COST_OUT or target/bench-results/costs.jsonl
}
```

The result is one JSON line per operation, for example
`{"name":"escrow::fund","cpu":912345,"mem":40211}`.

## Script design: `scripts/compare-benches.sh`

```
Usage: scripts/compare-benches.sh [--base <ref>] [--threshold <pct>] [--out <file>]
  --base       git ref to compare against (default: origin/main)
  --threshold  max allowed CPU regression in percent (default: 10)
  --out        write the markdown table here (default: stdout)
```

Steps:

1. `set -euo pipefail`; `git fetch origin main` if the base is `origin/main`.
2. Create a worktree for the base: `git worktree add "$TMP/base" "$BASE"`,
   and remove it in a `trap ... EXIT`. Don't check out over the user's tree.
3. Run the benches in both trees with `BENCH_COST_OUT` pointing at
   `target/bench-results/{base,head}.jsonl`. Pass `-- --quick` or
   `--warm-up-time 1`, since instruction counts don't need many samples.
4. Join the two files on `name` (`jq` or a short Python block) and print:

   | Operation | main (CPU) | branch (CPU) | Δ | |
   |---|---:|---:|---:|---|
   | `escrow::fund` | 912,345 | 931,002 | +2.0% | ✅ |
   | `token::transfer` | 401,220 | 452,118 | +12.7% | ❌ |
   | `bonding_curve::buy/100` | (new) | 1,204,551 | n/a | 🆕 |

   New operations (missing on `main`) and removed ones are listed without a
   verdict.
5. Exit `1` if any **core operation** exceeds the threshold, otherwise `0`.

### Core operations

Keep the list in the script (or `benches/core-ops.txt`) so it is reviewed
like code:

```
token::mint
token::transfer
token::approve_and_transfer_from
token::burn
escrow::fund
escrow::approve_delivery
escrow::resolve_dispute
```

Add `bonding_curve::buy/*` and `bonding_curve::sell/*` once
[#1368](1368-bonding-curve-benchmarks.md) lands. Operations not in the list
are still shown in the table but never fail the run.

## CI wiring

In `bench.yml`, on `pull_request`, replace the "Compare benchmarks against
baseline" Python step with:

```yaml
- uses: actions/checkout@...   # with fetch-depth: 0 so origin/main is available
- name: Compare CPU instructions against main
  run: ./scripts/compare-benches.sh --out bench-delta.md
- name: Post summary
  if: always()
  run: cat bench-delta.md >> "$GITHUB_STEP_SUMMARY"
```

The existing PR-comment step can post `bench-delta.md` instead of its own
table. The wall-time artifacts and HTML reports can stay for information only.

## Local use

```bash
./scripts/compare-benches.sh                  # vs origin/main
./scripts/compare-benches.sh --base HEAD~1    # vs previous commit
```

## Acceptance criteria

- [ ] `scripts/compare-benches.sh` is implemented and runnable locally
- [ ] It produces a markdown table of per-operation CPU-instruction deltas
- [ ] CI fails if a core operation regresses by more than 10% CPU instructions
