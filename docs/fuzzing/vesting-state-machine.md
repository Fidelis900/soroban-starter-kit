# Fuzz Target: `vesting_state_machine`

> Tracks issue **#1365**: libFuzzer state machine harness for vesting schedule revocation and cliff releases.

| | |
|---|---|
| **Target file** | `fuzz/fuzz_targets/vesting_state_machine.rs` |
| **Contract under test** | `contracts/vesting` (`soroban-vesting-template`) |
| **Harness style** | Stateful: an `arbitrary`-generated sequence of actions checked against a shadow model |
| **Run** | `cargo +nightly fuzz run vesting_state_machine -- -max_total_time=300` |

---

## 1. Why this target exists

A vesting schedule moves through states that depend on the ledger sequence: before the cliff, in the linear region, fully vested, and revoked. Milestone schedules add a second, approval-driven unlock path. Bugs here usually come from **ordering**, for example claim → revoke → claim, revoking exactly at the cliff, or approving a milestone after revocation. Fixed-sequence unit tests cover only a few such orderings. A state machine fuzzer generates arbitrary interleavings and checks token conservation after every step.

---

## 2. Contract API used by the harness

From `contracts/vesting/src/lib.rs`:

```rust
initialize(admin, token)
create_schedule(beneficiary, start_ledger, cliff_ledger, end_ledger, amount: i128, is_revocable: bool)
create_schedules_batch(admin, schedules: Vec<ScheduleParams>)          // all-or-nothing
create_milestone_schedule(beneficiary, verifier, milestones: Vec<(u32, u32)>, amount, is_revocable)
approve_milestone(beneficiary, milestone_id)                           // verifier only
claim(beneficiary) -> i128
revoke(beneficiary) -> (paid_to_beneficiary, clawed_back_to_admin)
get_info(beneficiary) -> Option<BeneficiarySchedule>
vested(beneficiary) -> i128
```

Relevant rules the model must mirror:

- **Bounds.** `validate_bounds` enforces `now ≤ start ≤ cliff < end`.
- **Linear vesting.** `vested_linear` is `0` before the cliff, `amount` at or after the end, and `floor(amount × (now − start) / (end − start))` in between.
- **Milestone vesting.** `floor(amount × Σ approved_bps / 10_000)`. Weights must be non-zero, the ids must be unique, and the weights must sum to `10_000`.
- **Revocation.** `revoke` pays `vested − claimed` to the beneficiary and claws back `amount − vested` to the admin. It then sets `amount = claimed = vested` and `revoked = true`.

> **On "transfer".** The issue mentions *transfer* actions. The vesting contract has no schedule-transfer entry point. The harness models "transfer" as an **external token transfer out of the beneficiary's wallet** after a claim. This checks that the contract's accounting depends only on its own storage, never on the beneficiary's live token balance. If a schedule-transfer function is added later, add it to the action enum.

---

## 3. Input model

```rust
#[derive(Arbitrary, Debug)]
struct Input {
    actions: Vec<Action>,          // capped at 128 per run
}

#[derive(Arbitrary, Debug)]
enum Action {
    CreateLinear { who: u8, start_off: u16, cliff_off: u16, end_off: u16, amount: u32, revocable: bool },
    CreateBatch  { entries: Vec<(u8, u16, u16, u16, u32, bool)> },   // ≤ 8 entries
    CreateMilestone { who: u8, weights: Vec<u16>, amount: u32, revocable: bool },
    Approve      { who: u8, milestone_idx: u8 },
    Claim        { who: u8 },
    Revoke       { who: u8 },
    TransferOut  { who: u8, amount: u32 },        // beneficiary moves tokens away
    AdvanceLedger { by: u16 },                    // 0 is allowed, to land exactly on a cliff or end
}
```

**Mapping rules**

- **Beneficiaries.** Use a fixed pool of ≤ 8 generated addresses, so creating a second schedule for the same beneficiary (`ScheduleAlreadyExists`) is reachable.
- **Offsets.** `start/cliff/end` are offsets from the current ledger. Do not sort them. Unsorted triples must be rejected with `InvalidScheduleBounds`, which is part of the test.
- **Amounts.** Map `amount` to `0..=u32::MAX as i128`. A zero amount must be rejected. Large amounts combined with long durations exercise the `mul_div` overflow path, which must return `ArithmeticError`, never panic.
- **Funding.** Mint the admin enough to cover every successful create.
- **Calls.** Use `try_*` client methods. Apply each action to the shadow model only if the contract accepted it, then compare.

---

## 4. Shadow model

For each beneficiary, keep:

```rust
struct Shadow {
    initial_grant: i128,   // amount at creation, never mutated
    claimed: i128,         // cumulative tokens paid to the beneficiary (claims + revoke payout)
    clawed_back: i128,     // tokens returned to the admin by revoke
    revoked: bool,
    kind: Linear { start, cliff, end } | Milestone { weights, approved },
}
```

The model computes `expected_vested(now)` independently of the contract and must agree with `vested(beneficiary)` for every live schedule after every action.

---

## 5. Invariants

Checked after **every** action, not only at the end of the run.

| # | Invariant |
|---|---|
| **V1** | **Per-schedule conservation.** `claimed + clawed_back + locked == initial_grant`, where `locked = schedule.amount − schedule.claimed` read from `get_info`. This is the issue's `claimed + unvested_remaining == initial_grant`, extended to cover revocation. |
| **V2** | **Contract solvency.** `token.balance(contract) == Σ_live (amount − claimed)` over all schedules. The contract never holds less than it owes. Note that it also never holds more. |
| **V3** | **Global conservation.** `admin_spent_on_grants == Σ claimed + Σ clawed_back + token.balance(contract)`. |
| **V4** | **Monotonic vesting.** For a non-revoked schedule, `vested(t₂) ≥ vested(t₁)` whenever `t₂ ≥ t₁` (milestones: whenever the approved set only grows). |
| **V5** | **Cliff gate.** For linear schedules, `vested == 0` while `now < cliff`, and `claim` returns `NothingToClaim`. |
| **V6** | **Full release.** For linear schedules, `vested == amount` once `now ≥ end`. |
| **V7** | **Claim bound.** `0 ≤ claimed ≤ vested ≤ amount` at all times. |
| **V8** | **Revoke finality.** After `revoke`, `vested == amount == claimed`. A further `claim` returns `NothingToClaim`, and further `revoke` or `approve_milestone` calls return `AlreadyRevoked`. |
| **V9** | **Irrevocable schedules.** `revoke` on an `is_revocable == false` schedule returns `ScheduleIrrevocable` and changes nothing. |
| **V10** | **Revoke split.** `revoke` returns `(pay, clawback)` with `pay == vested − claimed_before` and `clawback == initial_amount − vested`, both `≥ 0`. |
| **V11** | **Batch atomicity.** If `create_schedules_batch` fails, no entry from that batch exists and the admin balance is unchanged. |
| **V12** | **Wallet independence.** `TransferOut` never changes any contract-side value (`vested`, `get_info`, contract balance). |
| **V13** | **Model agreement.** The contract's `vested()` equals the shadow model's `expected_vested()`. |
| **V14** | **No host panic.** Every action ends in `Ok` or a typed `VestingError`. |

---

## 6. Registering the target

```toml
[dependencies.soroban-vesting-template]
path = "../contracts/vesting"

[[bin]]
name = "vesting_state_machine"
path = "fuzz_targets/vesting_state_machine.rs"
test = false
doc = false
bench = false
```

---

## 7. Running

```bash
cd fuzz
cargo +nightly fuzz run vesting_state_machine -- -max_total_time=300 -max_len=8192
```

**Seed corpus.** Commit a handful of hand-written inputs under `fuzz/corpus/vesting_state_machine/` that reach the interesting regions quickly:

- create → advance to the cliff exactly → claim
- create → claim mid-way → revoke → claim
- milestone create → approve all → revoke
- a batch with one invalid entry

---

## 8. Acceptance checklist

- [ ] `fuzz/fuzz_targets/vesting_state_machine.rs` implemented with an `arbitrary`-derived `Vec<Action>`
- [ ] Shadow model with `expected_vested` that is independent of the contract
- [ ] Conservation invariants V1–V3 checked after every action, across linear and milestone schedules and revocations
- [ ] `soroban-vesting-template` dependency and `[[bin]] vesting_state_machine` added to `fuzz/Cargo.toml`
- [ ] Five-minute run completes with no crashes
