# ADR-0011: Bonding-Curve State Machine Design

- **Status**: Accepted
- **Date**: 2026-10-16

## Context

The bonding-curve contract implements a token sale where price scales deterministically
with token supply. Unlike the escrow or lottery contracts, the curve does not progress
through a simple linear state sequence — it must handle concurrent buy/sell activity
within a single `Active` lifecycle and then irreversibly transition to `Graduated` once
a supply cap is reached. Without a formal state specification, subtle invariant
violations (e.g., reserve drain, sandwich attacks, post-graduation minting) are hard to
audit against the implementation.

## Decision

### States

```
Uninitialized ──initialize()──► Active ──graduate()──► Graduated
                                   │                       │
                               buy() / sell()         migrate_to_amm()
                               (self-loop)                 │
                                                           ▼
                                                     (terminal: external AMM)
```

| State | Meaning |
|-------|---------|
| `Uninitialized` | Contract deployed but `initialize` not yet called |
| `Active` | Normal operation: buying and selling are open |
| `Graduated` | Supply cap reached: minting (buying) is locked; only `migrate_to_amm` is available |

`Graduated` is semi-terminal: the reserve and supply are migrated out via
`migrate_to_amm`, after which the contract holds zero reserve and zero circulating
supply and no further operations are valid.

### Transition Rules

| Current State | Entry Point | Allowed? | Next State |
|---|---|---|---|
| `Uninitialized` | `initialize` | ✅ | `Active` |
| `Active` | `buy` | ✅ | `Active` (self-loop) |
| `Active` | `sell` | ✅ | `Active` (self-loop) |
| `Active` | `graduate` (supply ≥ cap) | ✅ | `Graduated` |
| `Graduated` | `buy` | ❌ `CurveGraduated` | — |
| `Graduated` | `migrate_to_amm` | ✅ | (contract drained) |
| `Graduated` | `sell` | ✅ | `Graduated` (burn tokens, release reserve) |

Every state-guarded entry point reads `DataKey::CurveState` before any auth check or
storage mutation, returning `BondingCurveError::NotInitialized` if absent and
`BondingCurveError::CurveGraduated` / `BondingCurveError::NotGraduated` as appropriate.

### Mathematical Invariants

The following invariants must hold after every state-mutating transaction:

#### I-1 Reserve Solvency

```
reserve_balance(contract) ≥ Σ sell_proceeds(token_i)  for all circulating tokens
```

More concretely, the on-chain XLM / stablecoin reserve held by the contract must
equal or exceed the theoretical redemption value of all outstanding wrapped tokens:

```
R ≥ sell_proceeds(supply, supply, base_price, slope, connector_weight_bps)
```

This invariant ensures no holder can be deprived of their redemption right.

Proof obligation: `buy_cost` and `sell_proceeds` are computed with `FixedPoint`
arithmetic that saturates on overflow; the `InsufficientReserve` guard in `sell`
enforces the invariant at runtime. Property tests in `prop_test.rs` verify this
across randomized supply/amount pairs.

#### I-2 Price Monotonicity (for linear curve segment)

```
spot_price(S + ε) ≥ spot_price(S)  for all ε > 0
```

With `base_price > 0` and `slope ≥ 0`, the linear price function
`base_price + slope * S / PRICE_SCALE` is non-decreasing in `S`. The Bancor
reserve price `R / S` can deviate downward on net sells but is floored at the
linear price, ensuring the curve never prices below the baseline.

#### I-3 Fee Integrity

```
net_reserve_increase_on_buy = buy_cost - fee
fee = buy_cost * fee_bps / BPS_DENOMINATOR
```

The fee is taken from the buyer's payment before it credits the reserve. The treasury
address receives the fee in the same transaction. This keeps `reserve + treasury_balance`
equal to the sum of all historical buyer payments.

#### I-4 Anti-Sandwich Rate Limit

```
buys_this_ledger(actor) ≤ MAX_BUYS_PER_LEDGER   (= 1)
ledger(last_buy(actor)) + BUY_COOLDOWN_LEDGERS ≤ current_ledger
```

These two invariants are checked atomically inside `buy` using persistent
`DataKey::LastBuyLedger` and `DataKey::BuysThisLedger` keys. Violation returns
`RateLimitExceeded` or `CooldownActive` without consuming the buyer's funds.

#### I-5 Graduation Irreversibility

```
state == Graduated  ⟹  state ≠ Active  (forever)
```

Once `CurveState::Graduated` is written to instance storage it can never revert to
`Active`. `migrate_to_amm` is the only valid post-graduation entry point, and it does
not change the state enum — it clears reserve and supply.

### Checks-Effects-Interactions (CEI) Pattern

```rust
// Effects first
env.storage().instance().set(&DataKey::Reserve, &new_reserve);
env.storage().instance().set(&DataKey::Supply, &new_supply);
bump(&env);
// Interactions last
token::Client::new(&env, &token_contract).transfer_from(...)
```

All storage mutations are committed before token transfers, preventing re-entrant
manipulation of the reserve/supply accumulators.

### Threat Model

| Threat | Mitigation |
|--------|-----------|
| Sandwich attack (same-ledger multi-buy) | `MAX_BUYS_PER_LEDGER = 1` per address; `BUY_COOLDOWN_LEDGERS = 1` cooldown |
| Reserve drain (sell more than reserve) | `InsufficientReserve` guard before `transfer`; I-1 enforced at runtime |
| Arithmetic overflow in price computation | `FixedPoint::mul_div` saturates; `Overflow` error propagated |
| Admin rug-pull via `migrate_to_amm` | `migrate_to_amm` requires `CurveGraduated` state; admin `require_auth()` |
| Graduation front-running | Graduation check is deterministic (supply ≥ cap); no secret randomness |
| Bogus fee configuration | `InvalidFee` if `fee_bps > MAX_FEE_BPS`; checked at `initialize` time |

## Consequences

- The two-state (`Active` / `Graduated`) model is simple to audit; there are no
  implicit intermediate states or timed transitions.
- Property tests in `contracts/bonding-curve/src/prop_test.rs` provide machine-checked
  evidence for I-1 and I-4 across wide input ranges.
- Operators who need a softer graduation curve (e.g., partial liquidity migration)
  must deploy a new contract; this ADR does not preclude future extension.
- The cooldown invariant (I-4) introduces a minimum ~5 s delay between consecutive
  buys from the same address, which may frustrate high-frequency automated strategies.
  This is an intentional safety/UX trade-off.

## References

- [Bancor Formula Whitepaper](https://storage.googleapis.com/website-bancor/2018/04/01ba8253-bancor_protocol_whitepaper_en.pdf)
- `contracts/bonding-curve/src/lib.rs` — `buy_cost`, `sell_proceeds`, `graduate`, `migrate_to_amm`
- `contracts/bonding-curve/src/storage.rs` — `CurveState`, `DataKey`, constants
- `contracts/bonding-curve/src/prop_test.rs` — property tests for I-1 and I-4
- [ADR-0003](0003-admin-model.md) — Admin model used by `initialize` and `migrate_to_amm`
