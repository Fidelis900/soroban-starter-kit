# ADR-0012: Subscription State Machine Design

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The subscription contract implements recurring token-pull payments. A provider
periodically charges subscribers by pulling from a pre-approved token allowance.
Multiple plan configurations (plan IDs) can co-exist for the same subscriber.
The contract must handle trial periods, delinquency tracking, grace periods, and
cancellation, while preventing double-charges and protecting subscribers from
malicious providers. Without a formal state specification, edge cases in the
`active → suspended` transition and the interaction between grace period expiry
and re-subscription are difficult to reason about.

## Decision

### States (per `(subscriber, plan_id)` pair)

```
                      subscribe()
                          │
                          ▼
                    ┌──────────┐
                    │  Trial   │◄──────────────────────────────────────────┐
                    └──────────┘                                           │
                         │                                                 │
               charge() after trial_ledgers                                │
                          │                                                │
                          ▼                                                │
                    ┌──────────┐                                           │
   ┌────────────────►  Active  │◄──────────────────────────────────────┐  │
   │                └──────────┘                                       │  │
   │  resubscribe()      │                                             │  │
   │  (new plan)    charge()──► payment ok ───────────────────────────┘  │
   │                     │                                                │
   │               charge()──► payment fails                             │
   │                     │                                                │
   │                     ▼                                                │
   │              ┌────────────┐                                          │
   │              │ Delinquent │  (within grace period)                   │
   │              └────────────┘                                          │
   │                     │                                                │
   │         payment ok ─┼─ payment fails beyond grace period            │
   │                     │            │                                   │
   │                     │            ▼                                   │
   │                     │     ┌───────────┐                              │
   │                     │     │ Suspended │                              │
   │                     │     └───────────┘                              │
   │                     │            │                                   │
   │                     │     reactivate()                               │
   │                     │            │                                   │
   │    cancel()         ▼            └──────────────────────────────────┘
   │                ┌──────────┐
   └────────────────┤Cancelled │ (terminal per plan)
                    └──────────┘
```

| State | `active` flag | `suspended` flag | `trial_completed` flag | Meaning |
|-------|:---:|:---:|:---:|---------|
| `Trial` | `true` | `false` | `false` | Subscription created, trial period running; billing not yet charged |
| `Active` | `true` | `false` | `true` | Normal billing; `charge` pulls payment on each elapsed interval |
| `Delinquent` | `true` | `false` | `true` | Payment failed; retry is allowed during the grace period |
| `Suspended` | `false` | `true` | `true` | Grace period expired; plan is frozen until `reactivate` is called |
| `Cancelled` | `false` | `false` | `true` | Subscriber or provider cancelled; no further charges |

`Delinquent` is not a separate enum variant — it is the composite state
`active=true && failed_charges_count > 0 && delinquent_since_ledger.is_some()`,
inferred from the `SubscriptionInfo` struct fields. This avoids an additional storage
key while still being fully auditable on-chain.

### Transition Rules

| From State | Entry Point | Condition | To State |
|---|---|---|---|
| (none) | `subscribe` | plan not previously cancelled | `Trial` |
| `Trial` | `charge` | `current_ledger ≥ last_charged + trial_ledgers` | `Active` |
| `Active` | `charge` | `current_ledger ≥ last_charged + interval_ledgers`, payment ok | `Active` |
| `Active` | `charge` | payment fails, within grace period | `Delinquent` |
| `Delinquent` | `charge` | payment ok | `Active` |
| `Delinquent` | `charge` | grace period expired | `Suspended` |
| `Active`/`Delinquent` | `cancel` | subscriber auth | `Cancelled` |
| `Suspended` | `reactivate` | subscriber auth + fresh allowance | `Active` |
| `Cancelled` | `resubscribe` | subscriber auth + new plan params | `Trial` |

`subscribe` on an already-active or suspended plan returns `AlreadySubscribed` or
`PlanAlreadyExists`. `charge` on a cancelled plan returns `SubscriptionInactive`.

### Mathematical Invariants

#### I-1 Billing Monotonicity

```
last_charged_ledger(t+1) ≥ last_charged_ledger(t)   for successful charges
```

`last_charged_ledger` is only updated on a successful payment or trial completion;
it is never reset to an earlier value. This ensures a subscriber cannot be billed
twice for the same interval.

#### I-2 Interval Enforcement

```
charge succeeds  ⟹  current_ledger ≥ last_charged_ledger + interval_ledgers
```

Checked via `IntervalNotElapsed` guard at the top of `charge_one`. Because Soroban
ledger sequence numbers are strictly increasing, this guard is monotone — once the
interval has elapsed it stays elapsed.

#### I-3 Allowance Sufficiency (provider-pull model)

```
transfer_from(subscriber → provider, amount) succeeds  ⟹
    allowance(subscriber → contract, token) ≥ amount
```

The contract does not escrow funds; it relies on the subscriber having granted a
token allowance ≥ `amount` before each billing cycle. A failed `try_transfer_from`
call increments `failed_charges_count` without reverting state (CEI pattern: state
written, then transfer attempted with `try_`), maintaining the delinquency record.

#### I-4 Grace Period Bound

```
charge_fails  ∧  (current_ledger - delinquent_since_ledger ≥ grace_period_ledgers)
  ⟹  active = false  ∧  suspended = true
```

Once `delinquent_since_ledger + grace_period_ledgers ≤ current_ledger`, any
subsequent `charge` call atomically transitions the plan to `Suspended`. The grace
period ledger count is configurable at contract initialization and defaults to
`DEFAULT_GRACE_PERIOD_LEDGERS`.

#### I-5 Cancelled Is a Permanent Sink (per plan ID)

```
state == Cancelled  ⟹  ¬∃ operation that sets active = true for same (subscriber, plan_id)
```

After cancellation the `(subscriber, plan_id)` persistent key is cleared. A
re-subscription via `resubscribe` creates a fresh `SubscriptionInfo` under the same
key with zeroed delinquency counters, but this is semantically a new subscription —
not a revival of the old one.

#### I-6 Prepay Discount Invariant

```
prepay_cost(amount, intervals, discount_bps) =
    amount × intervals × (BPS_DENOMINATOR − discount_bps) / BPS_DENOMINATOR
```

`discount_bps` must be in `[0, BPS_DENOMINATOR]`; `InvalidDiscount` is returned
otherwise. The arithmetic is checked via `checked_mul` / `checked_div` to prevent
overflow.

### Checks-Effects-Interactions (CEI) Pattern

State is written before `try_transfer_from` is called:

```rust
// Effects: persist delinquency record
info.failed_charges_count = info.failed_charges_count.saturating_add(1);
save_subscription(env, subscriber, &info);
// Interactions: attempt transfer (failure is tolerated)
let transfer = token_client.try_transfer_from(...);
```

Soroban's single-invocation model prevents true re-entrancy, but the CEI ordering
is maintained for consistency with the broader codebase and for clarity during audits.

### Threat Model

| Threat | Mitigation |
|--------|-----------|
| Provider double-charges in same interval | `IntervalNotElapsed` guard (I-2) |
| Subscriber drains allowance to avoid payment | Recorded as delinquent; plan suspended after grace period (I-4) |
| Provider charges after cancellation | `SubscriptionInactive` guard rejects `charge` on inactive plan |
| Integer overflow in prepay calculation | `checked_mul` / `checked_div`; `ArithmeticOverflow` returned (I-6) |
| Provider front-runs subscriber cancellation | `cancel` requires subscriber `require_auth()`; provider cannot prevent it |
| Re-subscription after suspension resets delinquency | Fresh `SubscriptionInfo` starts from zero; new trial / interval begins |

## Consequences

- The composite-state model (flags rather than an explicit `Delinquent` enum variant)
  reduces storage cost but requires readers to interpret multiple fields together.
  Future versions may introduce an explicit `SubscriptionState` enum for clarity
  at the cost of one extra storage byte.
- Batch `charge` across many subscribers in one transaction (`charge_batch`) inherits
  all per-plan state guarantees; a failure on one plan does not roll back charges
  already processed in the same batch (each plan is an independent `Ok`/`Err`).
- The `trial_ledgers = 0` case (no trial) is fully supported: the trial completion
  check is a no-op when `trial_ledgers == 0` because the condition
  `current_ledger ≥ last_charged_ledger + 0` is always true, immediately advancing
  the plan to `Active`.

## References

- `contracts/subscription/src/lib.rs` — `subscribe`, `charge`, `charge_one`, `cancel`, `resubscribe`
- `contracts/subscription/src/storage.rs` — `SubscriptionInfo`, `ChargeOutcome`, `DataKey`
- `contracts/subscription/src/errors.rs` — `SubscriptionError`
- `contracts/subscription/src/test.rs` — unit and property tests for billing invariants
- [ADR-0003](0003-admin-model.md) — Admin model used by contract initialization
