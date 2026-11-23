# Benchmark Suite: `lottery_ops`

> Tracks issue **#1367**: Criterion CPU and instruction benchmarks for lottery ticket purchase and draw.

| | |
|---|---|
| **Bench file** | `benches/benches/lottery_ops.rs` |
| **Contract under test** | `contracts/lottery` (imported as `soroban_lottery`) |
| **Registered in** | `benches/Cargo.toml` (`[[bench]] name = "lottery_ops"`, already present) |
| **Run** | `cargo bench -p contract-benchmarks --bench lottery_ops` |

---

## 1. Goal

Measure how the lottery's **Soroban resource cost** (CPU instructions and memory bytes, read from the host budget) and **wall-clock time** (from Criterion) scale with:

1. **The number of participants** buying tickets, from 10 to 500.
2. **The number of winners** selected in one `draw`.

`draw` iterates the whole ticket pool once to count distinct holders, and once more per winner to remove that winner's tickets. That makes it roughly `O(T × W)` in the number of tickets `T` and winners `W`, which is the cost most likely to approach the per-transaction instruction limit. The results feed the Lottery section of [`docs/gas-costs.md`](../gas-costs.md).

> **Current state.** The existing `benches/benches/lottery_ops.rs` imports a crate named `lottery_contract`, which does not exist. The dependency is named `soroban_lottery`. It also calls `initialize(admin, token, price, max_tickets)` and `draw_winner`, neither of which matches the current contract. It does not compile. This issue replaces it.

---

## 2. Contract API used

```rust
initialize(admin, token, ticket_price, winner_count, prize_splits: Vec<u32>,
           max_tickets_per_address: Option<u32>, fee_bps: Option<u32>, fee_recipient: Option<Address>)
buy_ticket(buyer)
buy_tickets_batch(buyer, count)
commit(hash: BytesN<32>, reveal_deadline: u32)
draw(secret: BytesN<32>, salt: BytesN<32>) -> Vec<Address>
claim_refund(buyer) -> i128
```

The token must be a real Stellar Asset Contract (`env.register_stellar_asset_contract_v2`), and buyers must be minted funds. A random `Address::generate()` token makes every `buy_ticket` fail.

---

## 3. Measurement method

Criterion measures wall-clock time for each iteration. Soroban resource cost is deterministic, so read it once per configuration from the budget rather than averaging it.

```rust
env.budget().reset_unlimited();         // reset immediately before the measured call
client.draw(&secret, &salt);
let cpu = env.budget().cpu_instruction_cost();
let mem = env.budget().memory_bytes_cost();
```

- **Exclude setup.** Use `iter_batched(setup, routine, BatchSize::LargeInput)` so that contract deployment, minting and ticket purchases are not timed when they are not the subject of the benchmark.
- **Report resource numbers.** Print the CPU and memory figures for each parameter value, for example with `eprintln!` in a one-off pass before the Criterion loop, so they can be copied into `gas-costs.md`. Criterion's own report covers wall-clock time only.
- **Fix the seeds.** Use a fixed `secret`, `salt` and ledger sequence so instruction counts are reproducible across runs and machines.

---

## 4. Benchmark groups

### 4.1 `lottery_buy_ticket_scaling`

Cost of ticket purchases as the pool grows.

| Parameter | Values |
|---|---|
| participants `N` | 10, 50, 100, 250, 500 |

Cases:

| Case | Measured routine | Answers |
|---|---|---|
| `single_purchase_at_N` | The setup buys `N − 1` tickets. The routine measures the `N`-th `buy_ticket`. | Does one purchase get more expensive as the pool grows? It should be roughly constant. |
| `fill_N` | Buy `N` tickets from `N` distinct addresses. | Total cost of filling a round. |
| `batch_N` | One `buy_tickets_batch(buyer, N)`. | Batched versus individual purchase. |

Use `BenchmarkId::new(case, N)` and `group.throughput(Throughput::Elements(N))`.

### 4.2 `lottery_draw`

Cost of `draw` across pool size and winner count.

| Parameter | Values |
|---|---|
| participants `N` (one ticket each) | 10, 50, 100, 250, 500 |
| winners `W` | 1, 3, 5, 10 (skip combinations where `W > N`) |
| fee | none, `fee_bps = 250` (adds one extra token transfer) |

The setup initialises with `W` equal prize splits, buys `N` tickets, and commits `sha256(secret‖salt)`. The routine measures only `draw(secret, salt)`.

Also add one **skewed pool** case. With `N = 500` tickets held by 25 addresses (20 each) and `W = 10`, the per-winner removal loop removes many tickets per round.

### 4.3 `lottery_refund`

`claim_refund` scans every ticket to count the caller's tickets, so its cost is linear in the total ticket count `T`.

| Parameter | Values |
|---|---|
| total tickets `T` | 10, 100, 500 |

The routine measures one buyer's `claim_refund` after the reveal deadline has passed.

---

## 5. Skeleton

```rust
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use soroban_lottery::{LotteryContract, LotteryContractClient};
use soroban_sdk::{testutils::{Address as _, Ledger}, token::StellarAssetClient, Address, BytesN, Env, Vec};

const PARTICIPANTS: &[u32] = &[10, 50, 100, 250, 500];
const WINNERS: &[u32] = &[1, 3, 5, 10];

fn bench_draw(c: &mut Criterion) {
    let mut g = c.benchmark_group("lottery_draw");
    g.sample_size(20); // the 500-participant setup is slow
    for &n in PARTICIPANTS {
        for &w in WINNERS.iter().filter(|&&w| w <= n) {
            g.bench_with_input(BenchmarkId::new(format!("w{w}"), n), &(n, w), |b, &(n, w)| {
                b.iter_batched(
                    || setup_committed(n, w),          // deploy, fund, buy, commit
                    |(env, client, secret, salt)| {
                        env.budget().reset_unlimited();
                        client.draw(&secret, &salt);
                        env.budget().cpu_instruction_cost()
                    },
                    BatchSize::LargeInput,
                );
            });
        }
    }
    g.finish();
}

criterion_group!(benches, bench_buy_ticket_scaling, bench_draw, bench_refund);
criterion_main!(benches);
```

---

## 6. Interpreting results

- **Ticket purchases.** `single_purchase_at_N` should stay flat. Growth with `N` means a purchase is scanning existing tickets, which is a regression.
- **Draw cost.** Expect `draw` CPU to grow roughly linearly in `N` for fixed `W`, and linearly in `W` for fixed `N`.
- **Instruction limit.** Compare the largest configuration against the network's per-transaction CPU limit (currently 100 M instructions). If any configuration exceeds about 50 % of the limit, record it in [`docs/known-issues.md`](../known-issues.md) as a recommended maximum for `N` and `W`.
- **Updating `gas-costs.md`.** Copy the CPU and memory figures into the Lottery table in [`docs/gas-costs.md`](../gas-costs.md), keyed by `(N, W)`.

---

## 7. Acceptance checklist

- [ ] `benches/benches/lottery_ops.rs` rewritten against the current lottery API, using the `soroban_lottery` crate and a funded SAC token
- [ ] Ticket purchase scaling benchmarked for 10, 50, 100, 250 and 500 participants
- [ ] Multi-winner `draw` benchmarked across `W ∈ {1, 3, 5, 10}`, with and without a fee
- [ ] CPU instructions and memory bytes reported for each configuration, and `docs/gas-costs.md` updated
- [ ] `cargo bench -p contract-benchmarks --bench lottery_ops` runs to completion
