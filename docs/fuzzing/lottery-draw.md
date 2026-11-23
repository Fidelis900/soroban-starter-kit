# Fuzz Target: `lottery_draw`

> Tracks issue **#1364**: libFuzzer invariant target for lottery commit-reveal fairness and entropy.

| | |
|---|---|
| **Target file** | `fuzz/fuzz_targets/lottery_draw.rs` |
| **Contract under test** | `contracts/lottery` (`soroban-lottery-template`) |
| **Harness style** | Structured input via `arbitrary`, single lottery round per run |
| **Run** | `cargo +nightly fuzz run lottery_draw -- -max_total_time=300` |

---

## 1. Why this target exists

The lottery contract picks winners with a commit-reveal scheme and a modulo reduction:

1. The admin calls `commit(hash, reveal_deadline)` with `hash = SHA-256(secret ‖ salt)`.
2. The admin later calls `draw(secret, salt)`. The contract checks the preimage against the stored commitment.
3. For each winner round `r`, the contract computes
   `entropy = SHA-256(secret ‖ salt ‖ ledger_seq_be ‖ r_be)` and
   `idx = u64_be(entropy[24..32]) % pool.len()`. See `entropy_hash` and `entropy_index` in `contracts/common/src/lib.rs`.
4. It removes every ticket held by the chosen winner, so each address can win at most once.
5. It splits the pot (`ticket_price × total_tickets`, minus an optional `fee_bps` rake) between winners using `prize_splits` (in basis points).
6. If the admin never reveals, buyers can call `claim_refund` once `ledger > reveal_deadline`.

Each step has edge cases that unit tests rarely reach: the modulo reduction at awkward pool sizes, ticket pools dominated by one address, basis-point rounding dust, fee rakes, and refunds racing the reveal deadline. A coverage-guided fuzzer can explore these.

---

## 2. Contract API used by the harness

These are the real signatures in `contracts/lottery/src/lib.rs`. The harness must use them as written.

```rust
initialize(admin, token, ticket_price: i128, winner_count: u32,
           prize_splits: Vec<u32>, max_tickets_per_address: Option<u32>,
           fee_bps: Option<u32>, fee_recipient: Option<Address>)
buy_ticket(buyer)
buy_tickets_batch(buyer, count: u32)
commit(hash: BytesN<32>, reveal_deadline: u32)      // admin, Open → Committed
draw(secret: BytesN<32>, salt: BytesN<32>) -> Vec<Address>   // admin, Committed → Drawn
rollover()                                           // Committed → Open
claim_refund(buyer) -> i128                          // Committed, ledger > reveal_deadline
get_info() -> LotteryInfo { total_tickets, state, ticket_price, .. }
get_winners() -> Vec<Address>
get_ticket_count(buyer) -> u32
```

> **Note:** The current `fuzz/fuzz_targets/lottery_draw.rs` was written against an older API (`submit_commit`, a seven-argument `initialize` with ticket caps and deadlines). It does not compile against the current contract, and it is not registered in `fuzz/Cargo.toml`. This issue replaces it.

---

## 3. Input model

Use `arbitrary::Arbitrary` to derive a structured input instead of slicing raw bytes by hand. libFuzzer then mutates meaningful fields rather than offsets.

```rust
#[derive(Arbitrary, Debug)]
struct Input {
    ticket_price: u16,             // mapped to 1..=10_000
    winner_count: u8,              // mapped to 1..=8
    split_seed: Vec<u16>,          // normalised so the splits sum to 10_000 bps
    max_per_address: Option<u8>,   // None, or 1..=50
    fee_bps: Option<u16>,          // None, or 1..=9_999
    buyers: Vec<BuyerOp>,          // capped at 64 entries
    secret: [u8; 32],
    salt: [u8; 32],
    reveal_offset: u8,             // reveal_deadline = now + 1 + offset
    reveal_correctly: bool,        // false → draw with a mutated secret
    advance_past_deadline: bool,   // exercises the refund path
    draw_ledger: u32,              // ledger sequence at draw time (feeds entropy)
}

#[derive(Arbitrary, Debug)]
struct BuyerOp {
    who: u8,                       // index into a pool of ≤ 16 generated addresses
    count: u8,                     // 1 → buy_ticket, >1 → buy_tickets_batch
}
```

**Mapping rules**

- **Ticket price.** Clamp into `1..=10_000` so `price × tickets` never overflows `i128`, and mint each buyer exactly what they spend. That makes balance accounting exact.
- **Prize splits.** Build `winner_count` non-zero weights from `split_seed`, then normalise them to sum to `10_000`. Put any remainder on the first split. Keep a small share of inputs (for example when `split_seed` is empty) where the splits are deliberately invalid. Those runs must be rejected with `InvalidWinnerConfig`, which is an invariant in its own right.
- **Addresses.** Draw from a fixed pool of ≤ 16 `Address::generate` values. Repeated `who` indices create the skewed, many-tickets-one-address pools that stress winner uniqueness.
- **Rejected calls.** Use `try_*` client methods throughout. A rejected call is a valid outcome. A host panic is a bug.

---

## 4. Run sequence

```text
setup env, SAC token, fee recipient
initialize(...)                     → if Err, assert the error is expected for the input, then return
for op in buyers: buy_ticket / buy_tickets_batch
snapshot: total_tickets, per-buyer spend, contract balance
commit(sha256(secret‖salt), now + 1 + reveal_offset)
set ledger = draw_ledger (bounded to stay ≤ reveal_deadline unless advance_past_deadline)
if advance_past_deadline:
    ledger = reveal_deadline + 1
    every buyer calls claim_refund
else:
    draw(secret or mutated secret, salt)
check invariants
```

---

## 5. Invariants

Let `P = ticket_price`, `T = total_tickets`, `W = winner_count`, `pot = P × T`.

| # | Invariant | How to check |
|---|---|---|
| **L1** | **Exact winner count.** A successful draw returns exactly `W` winners. | `draw(..).len() == W` and `get_winners().len() == W` |
| **L2** | **Winner uniqueness.** No address appears twice in the winners list. | Collect into a `BTreeSet`, compare lengths |
| **L3** | **Winners are ticket holders.** Every winner holds at least one ticket. | `get_ticket_count(w) > 0`; never the contract address (the refund sentinel) |
| **L4** | **Pot conservation.** `fee + Σ payouts + dust == pot`, where `dust` is what remains in the contract after the draw. | Token balance deltas for fee recipient, winners, contract |
| **L5** | **Bounded dust.** Floor division in `apply_bps_fee` loses at most 1 stroop per split, plus 1 for the fee, so `0 ≤ dust ≤ W + 1`. | From L4 |
| **L6** | **Pool sizing.** If distinct holders `< W`, `draw` fails with `InsufficientParticipants` and moves no tokens. | Balances unchanged |
| **L7** | **Reveal binding.** A wrong secret or salt fails with `RevealMismatch`. State stays `Committed` and no tokens move. | `reveal_correctly == false` path |
| **L8** | **Refund timing.** `claim_refund` fails with `RefundNotAvailable` while `ledger ≤ reveal_deadline` and succeeds after it. | Both sides of the deadline |
| **L9** | **Refund conservation.** Each buyer's refund equals `P × their_ticket_count`. A second claim returns nothing or an error. The sum of all refunds equals `pot`. | Track per-buyer spend from step 4 |
| **L10** | **No double settlement.** After `Drawn`, `draw`, `claim_refund` and `rollover` all fail. | Call each again |
| **L11** | **No host panic.** Every path ends in `Ok` or a typed `LotteryError`. | Implicit, because any panic is a libFuzzer crash |

> **About the issue's wording.** The issue asks for "total prize equals tickets × price". That holds for the *pot*, but not for the sum of payouts. Because of bps flooring and the optional fee, payouts alone are slightly smaller. L4 and L5 state the exact form of the invariant. If you assert `Σ payouts == pot` directly, the target will report false crashes.

### Modulo bias (observability, not an assertion)

`entropy_index` reduces a `u64` modulo `pool.len()`. Because `pool.len() ≤ u32::MAX ≪ 2⁶⁴`, the bias is below `2⁻³²` and cannot be measured by fuzzing. Do **not** assert distributional fairness inside the fuzz target. Instead:

- Assert `idx < pool.len()` for every round, through L1–L3.
- Optionally, behind a `cfg(feature = "stats")` flag, count how often each pool slot wins across runs. Use this for manual inspection or a proptest-based χ² check in `tests/`, not as a crash condition.

---

## 6. Registering the target

Add the contract dependency and the binary to `fuzz/Cargo.toml`:

```toml
[dependencies.soroban-lottery-template]
path = "../contracts/lottery"

[[bin]]
name = "lottery_draw"
path = "fuzz_targets/lottery_draw.rs"
test = false
doc = false
bench = false
```

---

## 7. Running and triaging

```bash
cd fuzz
cargo +nightly fuzz build lottery_draw
cargo +nightly fuzz run lottery_draw -- -max_total_time=300 -max_len=4096

# Reproduce and minimise a crash
cargo +nightly fuzz run lottery_draw artifacts/lottery_draw/crash-<hash>
cargo +nightly fuzz tmin lottery_draw artifacts/lottery_draw/crash-<hash>
```

Turn every confirmed crash into a regression unit test in `contracts/lottery/src/test.rs` before fixing it.

---

## 8. Acceptance checklist

- [ ] `fuzz/fuzz_targets/lottery_draw.rs` rewritten against the current lottery API, using an `arbitrary`-derived input
- [ ] Invariants L1–L11 asserted, including pot conservation (L4/L5) and winner uniqueness (L2)
- [ ] `soroban-lottery-template` dependency and `[[bin]] lottery_draw` added to `fuzz/Cargo.toml`
- [ ] `cargo +nightly fuzz run lottery_draw -- -max_total_time=300` completes with no crashes
