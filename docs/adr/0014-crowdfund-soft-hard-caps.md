# ADR 0014: Crowdfund Soft Cap and Hard Cap

- Status: Proposed
- Date: 2026-11-19
- Issue: [#1360](https://github.com/Fidelis900/soroban-starter-kit/issues/1360)

## Context

A crowdfund campaign today has one campaign-wide target, `goal`. `claim()` succeeds once
`total_pledged >= goal`, and nothing stops pledges after that point. A campaign can therefore
raise far more than the creator asked for or can responsibly use.

The contract already has `max_pledge_per_address`, but that limits each backer's cumulative
pledge. It does not limit the campaign as a whole.

Creators want two numbers:

- **Soft cap**: the minimum the campaign must raise to succeed. Below this, backers get refunds.
- **Hard cap**: the most the campaign will accept. Pledges that would go over it are rejected.

## Decision

### 1. `goal` is the soft cap

The existing `goal` already has soft-cap semantics: it is the threshold checked by `claim()`,
`withdraw()`, `refund()` and `refund_batch()`. We do **not** add a separate `soft_cap` value or
rename `goal`, because that would change the contract spec (the CLI flag is `--goal`) and every
existing caller. The docs and rustdoc will call it "goal (soft cap)".

### 2. Add an optional `hard_cap`

- `initialize` gets a new trailing parameter, `hard_cap: Option<i128>`. `None` keeps today's
  uncapped behaviour.
- New storage key: `DataKey::HardCap` (instance storage, written only when `Some`).
- `CrowdfundInfo` gets `hard_cap: Option<i128>` so clients can show remaining capacity.
- New view: `get_remaining_capacity() -> Option<i128>`, returning `hard_cap - total_pledged`,
  or `None` when the campaign is uncapped.

### 3. Validation at `initialize`

| Condition | Error |
|-----------|-------|
| `hard_cap <= 0` | `InvalidHardCap` (new) |
| `hard_cap < goal` | `InvalidHardCap`, because the campaign could never succeed |
| any `tier.threshold > hard_cap` | `InvalidTier`, because the tier could never be reached |

`max_pledge_per_address > hard_cap` is allowed. The cap is redundant in that case, but harmless.

### 4. Enforcement on pledges

In `pledge` and `pledge_with_token`, after the per-address cap check and **before** any storage
write or token transfer:

```rust
let new_total = total.checked_add(value).ok_or(CrowdfundError::Overflow)?;
if let Some(cap) = hard_cap {
    if new_total > cap {
        return Err(CrowdfundError::HardCapExceeded);
    }
}
```

- `pledge_with_token` compares `value_in_common_unit`, not the raw token amount, so the cap is
  in the campaign's unit of account.
- A pledge that lands exactly on the cap is accepted. The campaign is then full.
- **No partial fills.** If a pledge would go over the cap, the whole pledge is rejected rather
  than trimmed to fit. The backer authorised a specific amount, and silently transferring less
  would surprise them. Clients should call `get_remaining_capacity()` to size the pledge.
- `withdraw()` already decrements `TotalPledged`, so withdrawing frees capacity for other
  backers. No change is needed there.

### 5. Events

New event `hard_cap_reached(total)`, emitted once, when a pledge brings `total_pledged` exactly
to `hard_cap`. Indexers and frontends can use it to mark the campaign as sold out.

### 6. Error codes

New variants go at the end of `CrowdfundError`: `HardCapExceeded` and `InvalidHardCap`.
`snapshots/error_codes.snap` and [docs/error-reference.md](../error-reference.md) must be
updated in the same PR.

> **Prerequisite:** `errors.rs` currently reuses discriminants 17, 18 and 19
> (`ClaimDeadlineExpired`, `Overflow` and `MilestoneNotFound` clash with `CampaignCancelled`,
> `TierCapacityExceeded` and `InvalidTierId`). The crate also does not compile (see "Known
> gaps" below). Renumber the error codes so they are unique before assigning the new ones.

## Required tests

| Test | Expectation |
|------|-------------|
| `test_init_rejects_hard_cap_below_goal` | `InvalidHardCap` |
| `test_init_rejects_non_positive_hard_cap` | `InvalidHardCap` |
| `test_init_rejects_tier_above_hard_cap` | `InvalidTier` |
| `test_pledge_up_to_hard_cap_succeeds` | total == cap, `hard_cap_reached` emitted |
| `test_pledge_exceeding_hard_cap_rejected` | `HardCapExceeded`; balances and total unchanged |
| `test_pledge_with_token_respects_hard_cap` | `HardCapExceeded` using the common-unit value |
| `test_withdraw_frees_hard_cap_capacity` | a pledge that was blocked succeeds after another backer withdraws |
| `test_no_hard_cap_is_unbounded` | `None` keeps current behaviour |
| `test_soft_cap_not_met_refunds` | regression: goal still gates `claim` and `refund` |

Proptest (`prop_test.rs`): for any pledge sequence, `total_pledged <= hard_cap` always holds.

## Consequences

- Positive: creators can bound their raise, and backers are protected from oversubscription.
- Positive: backwards compatible for logic. `hard_cap = None` behaves the same as today.
- Negative: adding a parameter to `initialize` is a breaking ABI change for deployment scripts
  and tests. Every `client.initialize(...)` call site must pass an extra `&None`.
- Follow-up (out of scope): allow `claim()` before the deadline once the hard cap is reached.

## Known gaps in the current contract (fix first)

- `lib.rs` has an unclosed delimiter (`cargo check -p soroban-crowdfund-template` fails), and
  `events::campaign_cancelled` is missing its closing `);` and `}`.
- `refund_batch` references an undefined `milestones` variable.
- `test.rs` calls `initialize` with 6 arguments, but the function takes 7.
- `pledge_with_token` does not check `Cancelled`.
