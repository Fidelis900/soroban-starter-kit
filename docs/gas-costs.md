# Compute Unit (CU) Cost Reference

_Last verified: 2026-05-30 · Protocol 22_

> **Ledger timing:** The Stellar network targets **5 seconds per ledger**. All
> time estimates in this document use 5 seconds per ledger (e.g. 12 ledgers ≈ 60
> seconds, 1 minute ≈ 12 ledgers, 1 hour ≈ 720 ledgers).

Soroban charges **compute units (CUs)** per transaction. Each contract
invocation consumes CUs based on CPU instructions, memory, ledger I/O, and
host-function calls. The figures below are measured using the Soroban test
environment's CPU-instruction counter, which is the primary driver of on-chain
CU cost.

> **How to read this table**
> - *Measured CUs* – approximate instruction count recorded by the Soroban
>   host during unit/benchmark tests. Treat these as order-of-magnitude guides;
>   exact values vary with SDK version and ledger state.
> - *Storage ops* – number of instance/persistent/temporary reads + writes
>   performed by the function (each carries a fixed host-function overhead).
> - *Token transfers* – cross-contract calls to the token contract; each adds
>   roughly **500 000 – 700 000 CUs** on top of the base cost.
> - The Soroban network resource fee is calculated from protocol-defined rates
>   for CPU instructions, ledger reads/writes, bandwidth, history, events, and
>   rent. Under Protocol 22, CPU instructions cost **7 stroops per 10,000
>   instructions**; the total transaction fee depends on all resource components.

---

## Protocol 22 Fee Schedule
The following resource fee rates are current for Stellar Protocol 22. Verify
these values anytime the SDK or protocol version changes.

| Resource | Cost (stroops) |
|---|---|
| 10,000 CPU instructions | 7 |
| Read 1 ledger entry from disk | 1,563 |
| Read 1 KB from disk | 447 |
| Write 1 ledger entry | 2,500 |
| Write 1 KB to disk | 875 |
| 1 KB transaction size (bandwidth) | 406 |
| 1 KB transaction size (history) | 4,059 |
| 1 KB events/return value | 5,000 |
| 30 days of rent for 1 KB persistent storage | ~426,667 |
| 30 days of rent for 1 KB temporary storage | ~213,334 |

The values above are the protocol-defined rates used to compute Soroban resource
fees. Actual transaction fees will vary depending on the declared resource
usage and the validator-set limits in effect.

---

## Token Contract (`soroban-token-template`)

| Function | Measured CUs (approx.) | Storage ops | Notes |
|---|---|---|---|
| `initialize` | ~500 000 | 6 writes (instance) | One-time cost; sets admin, metadata, supply |
| `mint` | ~350 000 | 2 reads + 2 writes (persistent + instance) | Admin auth required |
| `burn_admin` | ~350 000 | 2 reads + 2 writes (persistent + instance) | Admin auth required |
| `set_admin` | ~150 000 | 1 read + 1 write (instance) | Admin auth required |
| `transfer` | ~400 000 | 2 reads + 2 writes (persistent) | Auth + balance update for both parties |
| `approve` | ~200 000 | 1 write (temporary) | TTL extended if expiration is in the future |
| `transfer_from` | ~500 000 | 1 read + 1 write (temporary) + 2 reads + 2 writes (persistent) | Allowance check + transfer |
| `burn` | ~300 000 | 1 read + 1 write (persistent + instance) | Self-auth required |
| `burn_from` | ~450 000 | 1 read + 1 write (temporary) + 1 read + 1 write (persistent) | Allowance check + burn |
| `allowance` | ~80 000 | 1 read (temporary) | Read-only; cheap |
| `balance` | ~60 000 | 1 read (persistent) | Read-only; cheapest call |
| `decimals` / `name` / `symbol` | ~50 000 | 1 read (instance) | Read-only metadata |
| `total_supply` | ~50 000 | 1 read (instance) | Read-only |
| `admin` | ~50 000 | 1 read (instance) | Read-only |

---

## Escrow Contract (`soroban-escrow-template`)

| Function | Measured CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~600 000 | 9 writes (instance) | 0 | One-time cost; stores all parties + state |
| `fund` | ~900 000 | 2 reads + 1 write (instance) | 1 (buyer → contract) | Cross-contract token transfer dominates |
| `mark_delivered` | ~200 000 | 2 reads + 2 writes (instance) | 0 | Seller auth only |
| `approve_delivery` | ~950 000 | 2 reads + 2 writes (instance) | 1 (contract → seller) | Triggers `release_to_seller` internally |
| `request_refund` | ~950 000 | 3 reads + 1 write (instance) | 1 (contract → buyer) | Only callable after deadline |
| `resolve_dispute` | ~950 000 | 2 reads + 1 write (instance) | 1 (contract → seller or buyer) | Arbiter auth; calls release or refund |
| `release_partial` | ~900 000 | 3 reads + 1 write (instance) | 1 (contract → seller) | Buyer auth; reduces stored amount |
| `cancel` | ~200 000 | 2 reads + 1 write (instance) | 0 | Only valid in `Created` state |
| `bump` | ~80 000 | 1 read + TTL extend (instance) | 0 | Anyone can call; no auth |
| `get_escrow_info` | ~120 000 | 7 reads (instance) | 0 | Read-only; returns full struct |
| `get_state` | ~50 000 | 1 read (instance) | 0 | Read-only |
| `is_deadline_passed` | ~50 000 | 1 read (instance) | 0 | Read-only |

---

## Cost Breakdown by Operation Type

| Operation | Approximate CU cost |
|---|---|
| Instance storage read | ~5 000 |
| Instance storage write | ~10 000 |
| Persistent storage read | ~8 000 |
| Persistent storage write | ~15 000 |
| Temporary storage read | ~5 000 |
| Temporary storage write | ~8 000 |
| TTL extension (instance) | ~5 000 |
| Cross-contract call (token transfer) | ~500 000 – 700 000 |
| `require_auth` check | ~50 000 – 100 000 |
| Event emission | ~20 000 |

---

## Reproducing the Measurements

Run the Criterion benchmarks locally to get instruction counts for your
specific SDK version:

```bash
# Token contract benchmarks
cd benches
cargo bench --bench token_ops

# Escrow contract benchmarks
cargo bench --bench escrow_ops
```

The CI pipeline (`.github/workflows/bench.yml`) runs these on every PR and
fails if any function regresses by more than **10%** versus the baseline.

---

## Tips for Minimising Fees

1. **Batch reads** – read all instance keys you need at the start of a
   function rather than interleaving reads and writes.
2. **Avoid unnecessary bumps** – `bump_instance` is called on every write;
   avoid calling it redundantly in read-only paths.
3. **Use `get_state` before heavier calls** – a cheap `get_state` read
   (~50 000 CUs) can confirm the escrow is in the right state before
   committing to a full `fund` or `approve_delivery` invocation.
4. **Token choice matters** – using a Stellar Asset Contract (SAC) for the
   token is slightly cheaper than a custom token contract because SAC
   host-functions are built into the Soroban host.
