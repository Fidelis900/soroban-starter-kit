# Compute Unit (CU) Cost Reference

_Last verified: 2026-05-30 · Protocol 22_

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

## Newer Contracts

The contracts below were added after the initial Token/Escrow benchmark suite
and do not yet have dedicated Criterion benches under `benches/`. Figures here
are **estimated**, not measured — see [Methodology](#methodology-for-newer-contracts)
for how they were derived. Treat them as order-of-magnitude guides only, and
prefer the measured Token/Escrow figures above when precision matters.

> **Tracked as [#970](https://github.com/Fidelis900/soroban-starter-kit/issues/970).**
> `auction`, `marketplace`, `staking`, `swap`, `dao`, and `lottery` are the
> priority contracts to move from this estimated section into real Criterion
> benches — they're the highest-traffic, money-math-heavy templates, and the
> ones where an undetected compute-unit regression would most directly affect
> minimum fees / mainnet feasibility.

### Airdrop Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~450 000 | 4 writes (instance) | 0 | Stores merkle root, token, admin |
| `set_root` | ~150 000 | 1 write (instance) | 0 | Admin auth required |
| `claim` | ~750 000 | 1 read + 1 write (persistent) | 1 (contract → claimant) | Merkle proof verification adds hashing cost |
| `claim_batch` | ~750 000 × N | N reads + N writes (persistent) | N | Cost scales linearly with batch size |
| `is_claimed` / `get_root` | ~60 000 | 1 read | 0 | Read-only |

### Auction Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `start` | ~500 000 | 6 writes (instance) | 0 | Seller auth; stores auction terms |
| `bid` | ~950 000 | 3 reads + 2 writes (instance) | 1 (bidder → contract) | Refunds prior high bidder to pending balance |
| `cancel` | ~200 000 | 2 reads + 1 write (instance) | 0 | Only valid before any bid |
| `end` | ~950 000 | 2 reads + 1 write (instance) | 1 (contract → seller) | Settles highest bid |
| `withdraw` | ~700 000 | 2 reads + 1 write (instance) | 1 (contract → bidder) | Pulls pending refund balance |
| `get_pending` / `get_info` | ~70 000 | 1-2 reads | 0 | Read-only |

### Ballot Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~400 000 | 3 writes (instance) | 0 | No token dependency |
| `register_voter` / `deregister_voter` | ~200 000 | 1 read + 1 write (persistent) | 0 | Admin auth required |
| `vote` | ~300 000 | 2 reads + 2 writes (persistent + instance) | 0 | Prevents double voting |
| `tally` / `get_yes_votes` / `get_no_votes` | ~55 000 | 1 read (instance) | 0 | Read-only |

### Bonding-Curve Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~450 000 | 4 writes (instance) | 0 | Stores curve slope + token references |
| `buy` | ~900 000 | 2 reads + 2 writes (instance) | 1 (buyer → contract) | Price computed from reserve/supply curve before transfer |
| `sell` | ~900 000 | 2 reads + 2 writes (instance) | 1 (contract → seller) | Same curve math, reverse direction |
| `get_reserve` / `get_supply` / `get_price` | ~55 000 | 1 read (instance) | 0 | Read-only |

### Crowdfund Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~500 000 | 5 writes (instance) | 0 | Stores goal, deadline, token |
| `pledge` | ~900 000 | 2 reads + 2 writes (persistent + instance) | 1 (pledger → contract) | Tracks per-pledger amount |
| `withdraw` (creator, goal met) | ~900 000 | 2 reads + 1 write (instance) | 1 (contract → creator) | Only after deadline + goal reached |
| `extend_deadline` | ~150 000 | 1 read + 1 write (instance) | 0 | Creator auth required |
| `claim` | ~900 000 | 2 reads + 1 write (instance) | 1 (contract → creator) | Alias path for goal-met payout |
| `refund` | ~900 000 | 2 reads + 2 writes (persistent + instance) | 1 (contract → pledger) | Only if goal not met after deadline |
| `get_info` / `get_pledge` | ~65 000 | 1 read | 0 | Read-only |

### Lottery Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~450 000 | 4 writes (instance) | 0 | Sets ticket price, token |
| `buy_ticket` | ~750 000 | 2 reads + 2 writes (persistent + instance) | 1 (buyer → contract) | Increments ticket count |
| `commit` | ~200 000 | 1 write (instance) | 0 | Admin commits `hash(secret ++ salt)` |
| `draw` | ~950 000 | 3 reads + 2 writes (instance) | 1 (contract → winner) | SHA-256 reveal check + winner payout |
| `claim_refund` | ~750 000 | 2 reads + 1 write (persistent) | 1 (contract → buyer) | Refund path if lottery cancelled |
| `get_info` / `get_winner` / `get_ticket_count` | ~60 000 | 1 read | 0 | Read-only |

### Marketplace Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~450 000 | 3 writes (instance) | 0 | Stores payment token, fee config |
| `list` / `list_with_expiry` | ~500 000 | 1 read + 2 writes (persistent) | 0 | Cross-contract check against NFT owner |
| `buy` | ~1 150 000 | 3 reads + 2 writes (persistent) | 1-2 (buyer → seller, optional royalty) + 1 NFT `transfer_from` | Most expensive path: two token transfers plus an NFT transfer |
| `cancel` | ~200 000 | 2 reads + 1 write (persistent) | 0 | Seller auth required |
| `sweep_expired` | ~200 000 × N | N reads + N writes (persistent) | 0 | Cost scales with number of expired listings swept |
| `make_offer` | ~900 000 | 2 reads + 2 writes (persistent) | 1 (offerer → contract, escrowed) | Escrows offer amount until accepted/cancelled |
| `accept_offer` | ~1 150 000 | 3 reads + 2 writes (persistent) | 1-2 + 1 NFT `transfer_from` | Same shape as `buy` |
| `cancel_offer` | ~700 000 | 2 reads + 1 write (persistent) | 1 (contract → offerer) | Returns escrowed offer amount |
| `get_listing` / `get_offer` / `get_active_listings` | ~70 000 | 1 read | 0 | Read-only |

### Oracle Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~350 000 | 3 writes (instance) | 0 | No token dependency |
| `update_price` | ~150 000 | 1 write (instance) | 0 | Admin-pushed single price |
| `set_publishers` | ~200 000 | 1 write (instance) | 0 | Admin-managed publisher set |
| `submit_price` | ~250 000 | 2 reads + 1 write (persistent) | 0 | Per-publisher price submission |
| `get_price` / `get_price_checked` / `get_price_data` | ~55 000 | 1 read (instance) | 0 | Read-only; `_checked` variant adds an age comparison |
| `get_median_price` / `get_twap` | ~120 000 | N reads (persistent) | 0 | Cost scales with number of publisher submissions / window size |

### Subscription Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~350 000 | 3 writes (instance) | 0 | Stores provider + token |
| `subscribe` | ~250 000 | 1 write (persistent) | 0 | Subscriber sets own amount/interval; no funds move yet |
| `charge` | ~750 000 | 2 reads + 1 write (persistent) | 1 (`transfer_from`, subscriber → provider) | Allowance-based pull; provider-initiated |
| `cancel` | ~150 000 | 1 read + 1 write (persistent) | 0 | Subscriber auth required |
| `get_subscription` / `get_provider` / `get_token` | ~55 000 | 1 read | 0 | Read-only |

### Swap Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `propose_swap` | ~950 000 | 1 write (persistent) | 1 (Party A → contract, escrowed) | Escrows Party A's tokens on proposal |
| `accept_swap` | ~1 400 000 | 2 reads + 1 write (persistent) | 2 (Party B → Party A, contract → Party B) | Atomic two-directional settlement |
| `cancel_swap` | ~700 000 | 2 reads + 1 write (persistent) | 1 (contract → Party A) | Only before acceptance or after expiry |
| `get_swap` / `swap_count` | ~55 000 | 1 read | 0 | Read-only |

### Timelock Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~550 000 | 6 writes (instance) | 1 (beneficiary/admin → contract, if funded at init) | Stores beneficiary, release ledger, token |
| `release` | ~700 000 | 2 reads + 1 write (instance) | 1 (contract → beneficiary) | Callable by anyone once release ledger reached |
| `cancel` | ~700 000 | 2 reads + 1 write (instance) | 1 (contract → admin) | Only before release ledger |
| `get_info` / `is_releasable` / `get_remaining_ledgers` | ~55 000 | 1 read (instance) | 0 | Read-only |

### Vesting Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~550 000 | 6 writes (instance) | 0 | Stores cliff/end ledgers, admin, token |
| `claim` | ~700 000 | 2 reads + 1 write (instance) | 1 (contract → beneficiary) | Computes linearly-vested amount since last claim |
| `revoke` | ~750 000 | 2 reads + 2 writes (instance) | 1 (contract → beneficiary, remaining vested) | Admin auth; unvested amount stays with admin |
| `admin_release` | ~700 000 | 2 reads + 1 write (instance) | 1 (contract → admin) | Reclaims unvested tokens post-revoke |
| `get_info` / `claimable` / `contract_version` | ~55 000 | 1 read (instance) | 0 | Read-only |

### Wrapped-Token Contract

| Function | Estimated CUs (approx.) | Storage ops | Token transfers | Notes |
|---|---|---|---|---|
| `initialize` | ~400 000 | 3 writes (instance) | 0 | Stores underlying + wrapped token references |
| `wrap` | ~900 000 | 1 read + 1 write (instance) | 1 (user → contract) + 1 wrapped-token mint | Deposits underlying, mints wrapped 1:1 |
| `unwrap` | ~900 000 | 1 read + 1 write (instance) | 1 wrapped-token burn + 1 (contract → user) | Burns wrapped, releases underlying 1:1 |
| `get_total_wrapped` | ~55 000 | 1 read (instance) | 0 | Read-only |

### Methodology for Newer Contracts

Unlike the Token and Escrow tables above (measured via the Criterion benches in
`benches/`), the tables in this section were **derived, not measured**:

1. Each function's storage reads/writes were counted by inspecting
   `contracts/<name>/src/lib.rs` for `env.storage()` calls.
2. Cross-contract token transfers (`token::Client::...transfer`,
   `transfer_from`, mint/burn on a wrapped/NFT contract) were counted from the
   same source pass.
3. Those counts were multiplied against the flat per-operation costs already
   established in the [Cost Breakdown by Operation Type](#cost-breakdown-by-operation-type)
   table above (e.g. instance write ≈ 10,000 CUs, cross-contract token
   transfer ≈ 500,000–700,000 CUs, `require_auth` ≈ 50,000–100,000 CUs) to
   arrive at a rough total.
4. No Criterion bench currently exists for these contracts under
   `benches/benches/`. Adding one per contract (following the pattern of
   `token_ops.rs` / `escrow_ops.rs`) and wiring it into
   `.github/workflows/bench.yml` would let these figures move from *estimated*
   to *measured* — tracked as [#970](https://github.com/Fidelis900/soroban-starter-kit/issues/970).
   As each contract gains a real bench, move its table out of this section and
   into the measured tables above, and drop it from the priority list in the
   [Newer Contracts](#newer-contracts) intro.

Because these figures are model-derived rather than measured, treat them as
directionally useful for comparing "cheap read" vs. "expensive multi-transfer
call" rather than as precise fee predictions.

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
