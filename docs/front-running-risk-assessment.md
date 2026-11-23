# Front-Running Risk Assessment and MEV Mitigation in `contracts/swap`

## Overview

In decentralized atomic token swaps, pending transactions broadcasted to public peer-to-peer networks and mempools are exposed to observation before final ledger inclusion. On Stellar and Soroban, arbitrageurs and automated MEV (Maximal Extractable Value) searchers can observe pending proposals or acceptances and attempt to front-run or sandwich transactions if cross-market price discrepancies occur between transaction broadcast and ledger execution.

This document outlines the front-running and MEV attack vectors identified in the swap contract and details the protocol safeguards implemented to mitigate them.

---

## 1. Identified MEV & Front-Running Vectors

### A. Mempool Arbitrage & Fast-Mover Front-Running
- **Scenario:** Party A creates an open swap offering an exchange rate at favorable or outdated terms relative to external decentralized/centralized exchanges.
- **Risk:** Before the intended recipient can accept, mempool bots observe the transaction, submit `accept_swap` with higher fee priority, and extract riskless arbitrage profits.

### B. Execution Delay & Stale Pricing
- **Scenario:** A swap proposal is configured with an excessively long expiration ledger buffer. The market price of `token_a` vs `token_b` moves sharply while the proposal remains open.
- **Risk:** The counterparty (or a malicious third party) delays acceptance until price movement guarantees an asymmetric profit, effectively executing an unpriced free option against Party A.

### C. Fee Evasion Griefing
- **Scenario:** Integer division truncation in basis-point fee calculation ($amount \times bps / 10000$) causes fee amounts to round down to 0 for small micro-transactions.
- **Risk:** Attackers fragment high-volume order flow into thousands of zero-fee sub-transactions to drain liquidity and avoid protocol treasury fees without economic consequence.

---

## 2. Implemented Protocol Mitigations

### 1. Counterparty Restriction (`allowed_counterparty`)
- Proposers can specify an optional `allowed_counterparty: Option<Address>` when calling `propose_swap`.
- When set, `accept_swap` strictly validates that `caller == allowed_counterparty`, rejecting unauthorized takers with `SwapError::NotAuthorized`.
- **Protection:** Completely eliminates third-party front-running and MEV bot sniping for private and OTC peer-to-peer trades.

### 2. Maximum Execution Delay Windows (`max_execution_delay`)
- Proposers can enforce an execution delay window (`max_execution_delay: Option<u32>`) representing the maximum ledgers allowed between swap creation (`created_at`) and swap execution (`accept_swap`).
- If `current_ledger > created_at + max_execution_delay`, the transaction is rejected with `SwapError::ExecutionDelayExceeded`.
- **Protection:** Prevents counterparties from holding pending proposals as stale options during volatile market conditions.

### 3. Tight Expiration Buffers (`expires_at`)
- Proposers have granular control over `expires_at` ledger limits to specify short execution time horizons.
- Swaps automatically expire if not completed within the designated ledger interval, with assets safely reclaimable via `cancel_swap`.

### Risk: Price Change Before Purchase (#1101)

**Description:** A seller could cancel and re-list at a higher price (or otherwise change the effective price) between the time a buyer signs a `buy` transaction and the time it executes.

**Mitigations Applied:**
- `buy(buyer, listing_id, max_price)` requires the buyer to state the maximum price they will pay
- The purchase is rejected with `PriceExceedsMax` if `listing.price > max_price`; no funds or NFT move

### Risk: Listing Expiry Manipulation

**Description:** A buyer could observe a pending listing creation and front-run with a purchase before the seller can set an expiry.

**Mitigations Applied:**
- Listings are active immediately upon creation
- Expiry is set at listing time and cannot be shortened
- sweep_expired allows sellers to reclaim expired listings

**Recommendations:**
- Consider allowing sellers to set an expiry after listing creation
- Document the expiry behavior clearly
### 4. Non-Zero Fee Floor Policy (`calculate_and_validate_fee`)
- Enforces that whenever protocol `fee_bps > 0`, the computed fee is clamped to a minimum non-zero value ($1$ unit of token) on trades with $amount > 0$.
- Invariant guarantee: `fee <= amount_b` ensures that treasury fees never exceed total traded amounts.
- **Protection:** Prevents micro-transaction splitting attacks that attempt to bypass treasury fees through integer division rounding.

---

## 3. Recommended Integration Practices for Frontends and Integrators

1. **For Private/OTC Trades:**
   - Always supply the designated taker address in `allowed_counterparty`.
   - Never use open taker mode (`None`) for private counterparty negotiations.

2. **For Open Market Swaps:**
   - Set a conservative `max_execution_delay` (e.g. 10–20 ledgers, ~50–100 seconds) to prevent market delay exploitation.
   - Set an explicit `expires_at` horizon aligned with market volatility.

3. **Querying Pending Order Flow:**
   - Use `get_active_swaps(cursor, limit)` with pagination to index open swaps deterministically without unbounded RPC payload overhead.
