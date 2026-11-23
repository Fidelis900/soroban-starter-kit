# ADR-0013: Wrapped-Token State Machine Design

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The wrapped-token contract issues a 1:1 synthetic token ("wToken") backed by an
underlying asset held in the contract's reserve. Users deposit the underlying asset
via `wrap` (minting wTokens) and redeem via `unwrap` (burning wTokens). A 1:1 peg
is the core security guarantee: every outstanding wToken must be redeemable for
exactly one unit of the underlying asset.

The contract supports optional features (`pausable` feature flag) and optional
constraints (per-address wrap cap, aggregate unwrap rate limit, proof-of-reserve
oracle). Without a formal state specification it is difficult to verify that peg
invariants hold in all reachable states, including the paused and rate-limited
paths.

## Decision

### States

```
Uninitialized ──initialize()──► Active ──pause()──► Paused
                                   │                   │
                               wrap()               (no wrap/unwrap)
                               unwrap()             unpause()──► Active
                               (self-loop)
```

| State | `Paused` flag | Meaning |
|-------|:---:|---------|
| `Uninitialized` | n/a | `initialize` not yet called |
| `Active` | `false` | Normal operation; `wrap` and `unwrap` are open |
| `Paused` | `true` | Admin has paused the contract; all minting/burning blocked |

`Paused` is only available when compiled with the `pausable` feature flag. Without
that feature the contract is always `Active` after initialization and there is no
pause transition.

There is no terminal state: `wrap` and `unwrap` are indefinitely available in `Active`.

### Transition Rules

| Current State | Entry Point | Auth Required | Feature Flag | Next State |
|---|---|:---:|:---:|---|
| `Uninitialized` | `initialize` | Admin | — | `Active` |
| `Active` | `wrap` | Caller | — | `Active` (self-loop) |
| `Active` | `unwrap` | Caller | — | `Active` (self-loop) |
| `Active` | `pause` | Admin | `pausable` | `Paused` |
| `Paused` | `unpause` | Admin | `pausable` | `Active` |
| `Paused` | `wrap` | — | `pausable` | ❌ `Unauthorized` |
| `Paused` | `unwrap` | — | `pausable` | ❌ `Unauthorized` |
| Any | `set_unwrap_rate_limit` | Admin | — | (unchanged) |
| Any | `set_unwrap_rate_limit_paused` | Admin | — | (unchanged) |

### Mathematical Invariants

#### I-1 Peg Invariant (Core Guarantee)

```
total_wrapped_supply == reserve_balance(underlying_token, contract)
```

At any point after initialization, the number of wTokens in circulation equals the
number of underlying tokens held by the contract. Formally:

```
∀ ledger L :  total_supply(wToken, L)  =  balance(underlying, contract, L)
```

This holds because:
- `wrap(amount)`:  deposits `amount` underlying  →  mints `amount` wTokens  (Δ = 0)
- `unwrap(amount)`: burns `amount` wTokens  →  releases `amount` underlying (Δ = 0)

Both operations follow CEI: the balance update is committed before the token
transfer, ensuring the invariant is maintained even if the transfer reverts.

#### I-2 Per-Address Wrap Cap

```
wrap(amount)  ⟹  cumulative_wrapped(address) + amount ≤ max_wrap_per_address
```

When configured at `initialize`, the contract accumulates wraps per address in
persistent storage (`DataKey::WrappedBy(Address)`). Exceeding the cap returns
`MaxWrapExceeded` without mutating state. The cap is checked before the transfer.

#### I-3 Aggregate Unwrap Rate Limit

```
unwrap_volume_in_window + amount ≤ unwrap_rate_limit
```

When the rate limit is active, the contract tracks cumulative unwrap volume in a
sliding ledger window (`DataKey::UnwrapRateUsage`). Exceeding the limit returns
`RateLimitExceeded`. The window resets when `current_ledger ≥ window_start + window`.

This invariant has a temporary exception: when `DataKey::UnwrapRateLimitPaused = true`
(set by the admin during migrations), the check is bypassed entirely, allowing the
admin to drain or migrate the reserve without being blocked by their own rate limit.

#### I-4 Reserve Non-Negativity

```
unwrap(amount)  ⟹  reserve_balance(underlying, contract) ≥ amount
```

`unwrap` calls `token::Client::balance` to read the current reserve before
attempting the transfer. If the balance is below `amount` it returns
`InsufficientReserve`. This guards against reserve discrepancies introduced by
external slashing, lost funds, or (hypothetically) proof-of-reserve oracle
deviations.

#### I-5 Proof-of-Reserve Consistency (optional)

When a `PorOracle` address is configured:

```
get_reserve() ≥ total_wrapped_supply
```

`unwrap` can optionally query the proof-of-reserve oracle's `get_reserve()` and
reject the transaction if the oracle reports undercollateralization. This is a
defense-in-depth check; I-1 already enforces on-chain peg correctness, but I-5
protects against off-chain discrepancies in cross-chain wrapped assets.

### Checks-Effects-Interactions (CEI) Pattern

`wrap`:
```rust
// Effects
env.storage().instance().set(&DataKey::WrappedBy(caller.clone()), &new_cumulative);
bump(&env);
// Interactions
underlying_token.transfer_from(&caller, &contract, &amount);
wrapped_token.mint(&caller, &amount);
```

`unwrap`:
```rust
// Effects
bump(&env);
// Interactions
wrapped_token.burn(&caller, &amount);
underlying_token.transfer(&caller, &amount);
```

In `unwrap` the wToken burn happens before the underlying transfer, ensuring the
peg invariant (I-1) cannot be violated even if the underlying transfer fails — if
the transfer reverts, the burn also reverts in the same Soroban invocation frame.

### Threat Model

| Threat | Mitigation |
|--------|-----------|
| Reserve drain (unwrap more than reserve) | `InsufficientReserve` guard (I-4) |
| Flash-loan unwrap attack | Aggregate rate limit (I-3); per-address cap (I-2) |
| Admin minting without reserve | Admin cannot call `wrap` directly; `wrap` always requires underlying transfer |
| Pause bypass | `require_not_paused` checked at the top of `wrap` and `unwrap` before auth |
| Oracle manipulation (PoR) | PoR oracle is advisory; I-1 enforces on-chain peg regardless |
| Per-address cap bypass via multiple addresses | Cap is per-address; Sybil resistance is out of scope for this contract |
| Overflow in cumulative wrap tracker | `InvalidAmount` returned for non-positive amounts; `checked_add` for accumulator |

## Consequences

- The 1:1 peg invariant (I-1) is the strongest and most critical guarantee; any future
  extension (e.g., yield-bearing wTokens) that breaks strict 1:1 redemption must
  supersede this ADR.
- The `pausable` feature flag introduces a compile-time branch that must be explicitly
  tested in both configurations. CI should run tests with `--features pausable` as well
  as without.
- The aggregate unwrap rate limit (I-3) is a contract-level circuit breaker; it does
  not replace monitoring-level alerting. See [docs/monitoring.md](../monitoring.md)
  for alert thresholds.
- Proof-of-reserve oracle integration (I-5) is optional and off by default; enabling
  it introduces an external dependency whose availability could block legitimate
  unwraps if the oracle is unavailable.

## References

- `contracts/wrapped-token/src/lib.rs` — `initialize`, `wrap`, `unwrap`, `enforce_unwrap_rate_limit`
- `contracts/wrapped-token/src/storage.rs` — `DataKey`, per-address and aggregate storage keys
- `contracts/wrapped-token/src/errors.rs` — `WrappedTokenError`
- `contracts/wrapped-token/src/prop_test.rs` — property tests verifying peg invariant
- `contracts/wrapped-token/src/test.rs` — unit tests for all state transitions
- [ADR-0003](0003-admin-model.md) — Admin model and pause/unpause authorization
- [ADR-0007](0007-token-interface-compliance.md) — Token interface used by `wrap`/`unwrap`
