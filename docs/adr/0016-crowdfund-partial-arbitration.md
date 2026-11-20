# ADR 0016: Crowdfund Partial-Deliverable Arbitration

- Status: Proposed
- Date: 2026-11-19
- Issue: [#1362](https://github.com/Fidelis900/soroban-starter-kit/issues/1362)

## Context

A funded campaign today has only all-or-nothing outcomes. The creator claims everything, or
releases milestones one at a time, or, if they never claim, backers eventually refund everything.
If a creator delivers part of the project and then stalls or abandons it, neither side has a fair
option. Remaining milestone funds stay locked, or the creator gets nothing for real work done.

The escrow contract already uses a single designated arbiter
([ADR 0006](0006-escrow-arbiter-model.md)). This ADR applies the same trust model to crowdfund.

## Decision

### 1. Arbiter configuration

- `initialize` gets `arbiter: Option<Address>`, stored as `DataKey::Arbiter`. With `None`,
  arbitration is off and every function below returns `NotAuthorized`.
- The arbiter must not be the creator (`InvalidArbiter`).
- The arbiter is fixed once the campaign is initialised. There is no rotation in this version, so
  the creator cannot replace an arbiter who rules against them.

### 2. Which campaigns can be disputed

A dispute can be opened only on a campaign that is **funded but not fully settled**:

- `total_pledged >= goal` and `ledger > deadline`;
- not cancelled;
- some funds are still held: `escrowed = total_pledged - released_milestone_sum - claimed_amount > 0`.

In practice this means campaigns that use milestones, because a full `claim()` leaves nothing
in escrow to dispute.

### 3. Lifecycle

```
Funded ──open_dispute()──▶ Disputed ──resolve_dispute(bps)──▶ Resolved
                               │
                               └── milestone release/vote and claim are blocked
```

| Function | Caller | Effect |
|----------|--------|--------|
| `open_dispute(caller)` | creator, or any backer with pledge > 0 | sets `DataKey::Disputed = true` and emits `dispute_opened` |
| `resolve_dispute(creator_bps: u32)` | arbiter (`require_auth`) | splits `escrowed`, sets `Resolved`, emits `dispute_resolved` |
| `claim_dispute_share(backer)` | anyone (permissionless, like ADR 0015) | pays a backer their share |

While `Disputed`, these functions return `CampaignDisputed`: `release_milestone`, `vote_milestone`,
`request_milestone_release`, `claim`, `refund` and `refund_batch`. Funds cannot leave through any
other path until the arbiter rules.

### 4. Proportional split

`creator_bps` is in basis points, `0..=10_000`. Anything above 10 000 returns `InvalidAmount`.

```
creator_share = escrowed * creator_bps / 10_000            // floor
backer_pool   = escrowed - creator_share
backer_i      = pledge_i * backer_pool / total_pledged       // floor
```

- `creator_share` is transferred to the creator immediately inside `resolve_dispute`.
- `backer_pool` and the `total_pledged` snapshot are stored, so each backer's share is fixed at
  resolution time. Backers are then paid through `claim_dispute_share`, one by one or in a keeper
  batch, because looping over every backer in one transaction would not fit Soroban's limits.
- **Rounding:** each backer payout rounds down. Leftover dust (fewer than `backer_count` stroops)
  stays in the contract. It is not swept to the creator, so the creator can never receive more
  than `creator_bps` of the escrow.
- Use `checked_mul` and `checked_div` throughout (see
  [docs/integer-overflow-audit-checklist.md](../integer-overflow-audit-checklist.md)).
  `pledge_i * backer_pool` stays far below `i128::MAX` for realistic token supplies.
- Edge cases: `creator_bps = 10_000` gives the creator the whole escrow (backers get 0).
  `creator_bps = 0` is a full refund of the remaining escrow.

### 5. Events

- `dispute_opened(opener)`
- `dispute_resolved(arbiter, creator_bps, creator_share, backer_pool)`
- `dispute_share_claimed(backer, amount)`

### 6. New errors

New errors go at the end of the enum: `CampaignDisputed`, `NotDisputed`, `DisputeAlreadyResolved`,
`InvalidArbiter`, `ArbitrationNotEnabled`.

## Required tests

| Test | Expectation |
|------|-------------|
| `test_open_dispute_by_backer` | state becomes `Disputed` |
| `test_open_dispute_by_stranger_rejected` | `NotAuthorized` |
| `test_open_dispute_before_deadline_rejected` | `DeadlineNotReached` |
| `test_dispute_blocks_milestone_release_and_claim` | `CampaignDisputed` |
| `test_resolve_60_40_split` | creator gets 60% of the escrow, and backers' shares together equal 40% (minus dust) |
| `test_resolve_after_partial_milestone_release` | only the unreleased escrow is split |
| `test_resolve_0_and_10000_bps` | full refund and full creator payout |
| `test_resolve_bps_over_10000_rejected` | `InvalidAmount` |
| `test_resolve_by_non_arbiter_rejected` | auth failure |
| `test_resolve_twice_rejected` | `DisputeAlreadyResolved` |
| `test_backer_share_proportional_and_once_only` | the second claim pays 0 |
| `test_no_arbiter_configured` | `ArbitrationNotEnabled` |

Proptest: for random pledges and a random `creator_bps`,
`creator_share + Σ backer_i <= escrowed` and `escrowed - (creator_share + Σ backer_i) < backer_count`.

## Consequences

- Positive: partial delivery has a fair, auditable on-chain outcome.
- Negative: it relies on a trusted arbiter. The centralisation and key-compromise risks in
  ADR 0006 apply here too, and [docs/threat-model.md](../threat-model.md) needs a crowdfund-arbiter
  entry.
- Negative: an unresponsive arbiter freezes a disputed campaign. A follow-up should add a dispute
  timeout that falls back to a pro-rata refund (`creator_bps = 0`).
- Depends on [ADR 0015](0015-crowdfund-batch-refund-keeper.md) for keeper-style share payouts, and
  on the crate compiling again (see ADR 0014, "Known gaps").
