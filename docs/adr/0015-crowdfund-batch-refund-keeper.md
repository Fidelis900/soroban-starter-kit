# ADR 0015: Crowdfund Permissionless Batch Refund Keeper

- Status: Proposed
- Date: 2026-11-19
- Issue: [#1361](https://github.com/Fidelis900/soroban-starter-kit/issues/1361)

## Context

When a campaign expires without reaching its goal, every backer must send their own `refund`
transaction. Backers who don't notice, or who hold too little XLM for fees, leave funds stranded
in the contract until their persistent `Pledge` entry expires.

The contract already has `refund_batch(pledgers: Vec<Address>)`. It is permissionless, but it is
incomplete:

- it is broken: it references an undefined `milestones` variable, so the crate doesn't compile;
- its eligibility rules differ from `refund`: it rejects the "goal met, but the creator missed
  the claim window" case that `refund` allows;
- it never refunds multi-token pledges (`TokenPledge(pledger, token)`);
- it has no batch size limit and emits no summary event, so keepers can't size or track runs.

## Decision

### 1. Public entry point

Expose `distribute_refunds_batch(env: Env, contributors: Vec<Address>) -> Result<u32, CrowdfundError>`,
the name used in the issue. It returns the number of contributors actually refunded. Keep
`refund_batch` as a thin deprecated wrapper for one release so existing callers keep working.

### 2. Permissionless, no contributor auth

Anyone (a keeper bot, the creator, or a backer) may call it. No `require_auth` is needed from the
contributors, because money can only move from the contract **to the address that originally
pledged it**. A caller cannot redirect funds; it only pays the transaction fee.

### 3. Eligibility matches `refund` exactly

Move the eligibility check out of `refund` into one private helper, `refund_open(&env) -> Result<(), CrowdfundError>`,
and call it from both entry points. Refunds are open when **any** of these is true:

- the campaign is cancelled;
- `ledger > deadline` and `total_pledged < goal`;
- `ledger > deadline + claim_window`, the goal was met, and the creator never claimed.

Otherwise the call returns the same errors `refund` does (`DeadlineNotReached` or `GoalAlreadyMet`).
The check runs once per batch, not once per contributor.

### 4. Per-contributor processing: skip, don't fail

For each address:

1. Read `Pledge(addr)`. If it is greater than 0, remove the key, transfer the amount, and emit
   `refunded`.
2. For each whitelisted token, read `TokenPledge(addr, token)`. If it is greater than 0, remove
   the key, transfer the amount in that token, and emit `refunded`.
3. If nothing was owed, **skip silently**. Don't return an error.

Skipping means a single stale or duplicate address can't revert a whole batch that other
contributors depend on. Duplicates are safe: the second time an address comes up, its entry has
already been removed, so it is skipped.

State is removed **before** each transfer (checks-effects-interactions), matching `refund`.

### 5. Batch size limit

Add `const MAX_REFUND_BATCH: u32 = 50;`. Return `InvalidAmount` (or a new `BatchTooLarge`) if
`contributors.len()` is greater than this. That keeps a single call within Soroban's
per-transaction read/write entry and CPU limits. Measure the real limit with
[benches/](../../benches/) and record it in [docs/gas-costs.md](../gas-costs.md).

### 6. Summary event

After the loop, emit `refund_batch_processed(caller_count, refunded_count, total_amount)`. Keepers
can stop once `refunded_count == 0`.

### 7. Discovering contributors

The contract has no on-chain list of backers, and keeping one would make each pledge more
expensive. Keepers build the list off-chain from `pledged` events (topic
`("pledged", pledger)`; see [docs/event-catalogue.md](../event-catalogue.md)), then send it in
chunks of `MAX_REFUND_BATCH`.

## Required tests

| Test | Expectation |
|------|-------------|
| `test_batch_refund_after_failed_campaign` | 3 backers refunded in one call, balances restored, returns 3 |
| `test_batch_refund_before_deadline_rejected` | `DeadlineNotReached` |
| `test_batch_refund_goal_met_rejected` | `GoalAlreadyMet` while the claim window is open |
| `test_batch_refund_after_claim_window_expiry` | succeeds when the creator missed the window |
| `test_batch_refund_cancelled_campaign` | succeeds before the deadline |
| `test_batch_refund_skips_unknown_and_duplicates` | no panic, each backer paid once |
| `test_batch_refund_is_idempotent` | the second call returns 0 and moves no tokens |
| `test_batch_refund_no_contributor_auth_needed` | works under `mock_auths(&[])` for the backers |
| `test_batch_refund_multi_token_pledges` | `TokenPledge` balances are returned in the right token |
| `test_batch_refund_size_limit` | `MAX_REFUND_BATCH + 1` addresses are rejected |

## Consequences

- Positive: one keeper transaction can clear a failed campaign, and backers pay no fees.
- Positive: `refund` and the batch path share one eligibility rule, which removes the current
  divergence.
- Negative: keepers depend on off-chain event indexing to find contributors.
- Negative: no keeper reward. A follow-up could pay a small bounty, but that would take value
  from backers and needs its own decision.
