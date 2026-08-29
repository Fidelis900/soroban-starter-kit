# ADR-0004: Escrow State Machine Design

- **Status**: Accepted
- **Date**: 2024-04-24

## Context

An escrow contract must enforce a strict lifecycle so that funds can never be double-spent, released to the wrong party, or locked forever. The design must be auditable and easy to test.

## Decision

### States

```
Created ──fund()──► Funded ──mark_delivered()──► Delivered
   │                  │                              │
cancel()        raise_dispute()             raise_dispute()
   │                  │                              │
   ▼                  └──────────────┬───────────────┘
Cancelled                            ▼
                                Disputed
                                   │
                 resolve_dispute() (arbiter / multisig-arbiter, ADR-0008)
                 claim_dispute_timeout() (buyer, if timeout configured)
                      │                        │
                      ▼                        ▼
                  Completed               Refunded
               (release to seller)     (return to buyer)

Also reachable without entering Disputed:

  Funded/Delivered ──request_refund() (after deadline)──► Refunded
  Delivered ──approve_delivery()──► Completed
```

| State | Meaning |
|-------|---------|
| `Created` | Escrow initialised; no funds held yet |
| `Funded` | Buyer has transferred tokens to the contract |
| `Delivered` | Seller has marked goods/services as delivered |
| `Disputed` | Either party has raised a dispute via `raise_dispute()`; contract awaits arbiter resolution |
| `Completed` | Funds released to seller — terminal |
| `Refunded` | Funds returned to buyer — terminal |
| `Cancelled` | Buyer cancelled before funding — terminal |

### Transition rules

Every entry point reads the current state first and rejects calls that are invalid for that state, returning `EscrowError::InvalidState`. This is enforced at the top of each function before any auth check or storage mutation.

### Checks-Effects-Interactions (CEI) pattern

All token transfers happen **after** the state is updated in storage:

```rust
// Effects first
env.storage().instance().set(&State, &EscrowState::Completed);
bump_instance(&env);
// Interactions last
token::Client::new(&env, &token_contract).transfer(…);
```

This prevents re-entrancy: if the token transfer somehow triggers a re-entrant call back into the escrow, the state is already terminal and all state-guarded functions will return `InvalidState`.

### Deadline enforcement

The `deadline_ledger` is a Soroban ledger sequence number set at initialisation. It must be at least `MIN_DEADLINE_BUFFER` (100) ledgers in the future.

- `request_refund` is only valid when `env.ledger().sequence() > deadline` **and** the state is `Funded` or `Delivered`.
- There is no automatic expiry; the buyer must explicitly call `request_refund`.

### Partial release

`release_partial(amount)` allows the buyer to release a portion of funds to the seller while the escrow remains in `Funded` or `Delivered` state. The stored `Amount` is decremented; the state does not change. This enables milestone-based payments without requiring a new escrow deployment.

### Dispute flow

Dispute resolution is a **two-step** transition, not a direct edge from `Funded`/`Delivered` to a terminal state:

1. **Enter `Disputed`**: Either party (buyer or seller) calls `raise_dispute()` from `Funded` or `Delivered`. This records the dispute timestamp and transitions to `Disputed`.
2. **Exit `Disputed`**: The arbiter (or a quorum of multisig arbiters, see ADR-0008) calls `resolve_dispute(release_to_seller: bool)`. If `true`, the state transitions through `Delivered` to `Completed`; if `false`, it transitions through `Funded` to `Refunded`. Alternatively, if a `DisputeTimeoutLedgers` was configured and the timeout has elapsed, the buyer may call `claim_dispute_timeout()` to auto-resolve in their favour.

The arbiter has no power in `Created`, `Completed`, `Refunded`, or `Cancelled` states — only in `Disputed`. Both the single-arbiter path and the multisig-arbiter path (ADR-0008) share this same `Disputed` state as their common entry point.

## Consequences

- Terminal states (`Completed`, `Refunded`, `Cancelled`) are irreversible; a new contract instance must be deployed for a new escrow.
- The `Disputed` intermediate state ensures that arbiter resolution is always explicit and traceable on-chain — there is no way for an arbiter to resolve a dispute that was never raised.
- The CEI pattern makes the contract safe against re-entrant token callbacks.
- The explicit state enum makes the lifecycle easy to audit and test exhaustively.
- Partial releases reduce the need for multiple escrow deployments in milestone scenarios.
