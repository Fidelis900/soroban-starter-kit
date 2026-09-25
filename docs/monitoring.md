# Contract Monitoring Guide

How to index events, detect anomalies, and set up alerting for deployed Soroban contracts.

---

## 1. Overview

Soroban contracts emit structured events that can be consumed by off-chain indexers, dashboards, and alerting systems. This guide covers the full monitoring stack:

- Subscribing to events via Stellar Horizon
- Understanding the event schema for token and escrow contracts
- Detecting anomalies in on-chain activity
- Recommended alerting thresholds
- Tooling: Stellar Expert, Horizon, and custom indexers

---

## 2. Subscribing to Contract Events via Stellar Horizon

Horizon exposes a streaming endpoint for contract events using Server-Sent Events (SSE).

### REST (paginated)

```bash
curl "https://horizon-testnet.stellar.org/contracts/<CONTRACT_ID>/events?limit=200&order=asc&cursor=now"
```

### SSE (streaming)

```bash
curl -N "https://horizon-testnet.stellar.org/contracts/<CONTRACT_ID>/events?cursor=now" \
  -H "Accept: text/event-stream"
```

### JavaScript (SDK)

```ts
import { Horizon } from '@stellar/stellar-sdk';

const server = new Horizon.Server('https://horizon-testnet.stellar.org');

server.contractEvents(CONTRACT_ID)
  .cursor('now')
  .stream({
    onmessage: (event) => {
      console.log('Contract event:', event);
    },
    onerror: (err) => {
      console.error('Stream error:', err);
    },
  });
```

### Filtering by event topic

Horizon supports `topic` query parameters to narrow the event stream. Topics are XDR-encoded; use the Stellar SDK to construct filters:

```ts
import { xdr, nativeToScVal } from '@stellar/stellar-sdk';

// Only stream 'transfer' events
const topicFilter = xdr.ScVal.scvSymbol('transfer').toXDR('base64');

const url = `https://horizon-testnet.stellar.org/contracts/${CONTRACT_ID}/events`
          + `?topic1=${encodeURIComponent(topicFilter)}&cursor=now`;
```

---

## 3. Event Schema

### Token Contract Events

All token events are emitted by `contracts/token` and follow this structure:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | — | `(String name, String symbol, u32 decimals)` |
| `mint` | `Symbol("mint")` | `Address` (recipient) | — | `i128` (amount) |
| `burn` | `Symbol("burn")` | `Address` (account) | — | `i128` (amount) |
| `transfer` | `Symbol("transfer")` | `Address` (from) | `Address` (to) | `i128` (amount) |
| `account_frozen` | `Symbol("account_frozen")` | `Address` (account) | — | `()` |
| `account_unfrozen` | `Symbol("account_unfrozen")` | `Address` (account) | — | `()` |
| `approve` | `Symbol("approve")` | `Address` (owner) | `Address` (spender) | `i128` (amount) |
| `revoke` | `Symbol("revoke")` | `Address` (owner) | `Address` (spender) | `()` |
| `admin_changed` | `Symbol("admin_changed")` | `Address` (old admin) | — | `Address` (new admin) |
| `admin_proposed` | `Symbol("admin_proposed")` | `Address` (current admin) | — | `Address` (pending admin) |
| `admin_accepted` | `Symbol("admin_accepted")` | `Address` (new admin) | — | `()` |
| `admin_proposal_cancelled` | `Symbol("admin_proposal_cancelled")` | `Address` (admin) | — | `()` |
| `paused` | `Symbol("paused")` | `Address` (admin) | — | `()` |
| `unpaused` | `Symbol("unpaused")` | `Address` (admin) | — | `()` |
| `upgraded` | `Symbol("upgraded")` | `Address` (admin) | — | `BytesN<32>` (wasm hash) |
| `hook_set` | `Symbol("hook_set")` | `Address` (admin) | — | `Option<Address>` (hook; `None` to clear) |
| `snapshot` | `Symbol("snapshot")` | `Address` (account) | `u32` (ledger) | `i128` (balance) |
| `permit_signer_set` | `Symbol("permit_signer_set")` | `Address` (owner) | — | `()` |
| `permit_used` | `Symbol("permit_used")` | `Address` (owner) | `Address` (spender) | `(i128 amount, u32 nonce)` |

`permit_used` is emitted in addition to (not instead of) `approve`/`revoke` whenever
`approve_with_signature` succeeds, so any dashboard already watching allowance
changes via `approve`/`revoke` needs no special-casing for signature-granted
allowances. Watch `permit_signer_set` for unexpected signer rotations — a
rotation invalidates all outstanding, unsubmitted permits signed with the
previous key.

`account_frozen` / `account_unfrozen` are the compliance freeze signals — index
both if you need to know which accounts currently cannot transfer. `hook_set`
fires from `set_transfer_hook` (data is `None` when the hook is cleared).
`snapshot` is the governance balance snapshot from `snapshot()` (issue #717);
topic[2] is the ledger the snapshot is recorded against. `admin_proposal_cancelled`
fires when the current admin aborts a pending two-step admin transfer via
`cancel_admin_proposal`.

### Escrow Contract Events

All escrow events are emitted by `contracts/escrow`:

| Event | Topic[0] | Topic[1] | Topic[2] | Topic[3] | Data |
|-------|----------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (buyer) | `Address` (seller) | `Address` (arbiter) | `i128` (amount) |
| `escrow_created` | `Symbol("escrow_created")` | `Address` (buyer) | `Address` (seller) | — | `i128` (amount) |
| `escrow_funded` | `Symbol("escrow_funded")` | `Address` (buyer) | — | — | `i128` (amount) |
| `delivery_marked` | `Symbol("delivery_marked")` | `Address` (seller) | — | — | `()` |
| `funds_released` | `Symbol("funds_released")` | `Address` (seller) | — | — | `i128` (amount) |
| `partial_release` | `Symbol("partial_release")` | `Address` (seller) | — | — | `i128` (amount) |
| `funds_refunded` | `Symbol("funds_refunded")` | `Address` (buyer) | — | — | `i128` (amount) |
| `dispute_raised` | `Symbol("dispute_raised")` | `Address` (caller) | — | — | `()` |
| `amount_updated` | `Symbol("amount_updated")` | `Address` (buyer) | — | — | `i128` (new amount) |
| `deadline_extended` | `Symbol("deadline_extended")` | `Address` (buyer) | — | — | `u32` (new deadline) |
| `paused` | `Symbol("paused")` | `Address` (admin) | — | — | `()` |
| `unpaused` | `Symbol("unpaused")` | `Address` (admin) | — | — | `()` |
| `upgraded` | `Symbol("upgraded")` | `Address` (admin) | — | — | `BytesN<32>` (wasm hash) |

### Subscription Contract Events

Events emitted by `contracts/subscription`:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (provider) | — | `Address` (token) |
| `plan_registered` | `Symbol("plan_registered")` | `Symbol` (plan_id) | — | `(i128 amount, u32 interval_ledgers)` |
| `plan_updated` | `Symbol("plan_updated")` | `Symbol` (plan_id) | — | `bool` (active) |
| `subscribed` | `Symbol("subscribed")` | `Address` (subscriber) | — | `(Symbol plan_id, i128 amount, u32 interval_ledgers)` |
| `charged` | `Symbol("charged")` | `Address` (subscriber) | `Address` (provider) | `i128` (amount) |
| `cancelled` | `Symbol("cancelled")` | `Address` (subscriber) | — | `()` |
| `trial_completed` | `Symbol("trial_completed")` | `Address` (subscriber) | — | `()` |

`plan_registered` fires from `register_plan`; `plan_updated` from `set_plan_active`.
`subscribed` data includes `plan_id` so an indexer can join a subscription to its plan
without a separate lookup. `trial_completed` is emitted from `charge()` when a
subscriber's trial period ends (no separate `complete_trial` entry point).

### Wrapped-Token Contract Events

Events emitted by `contracts/wrapped-token`:

| Event | Topic[0] | Data |
|-------|----------|------|
| `initialized` | `Symbol("initialized")` | `(Address admin, Address wrapped_token)` |
| `wrapped` | `Symbol("wrapped")` | `(Address user, i128 amount, i128 new_total_wrapped)` |
| `unwrapped` | `Symbol("unwrapped")` | `(Address user, i128 amount, i128 new_total_wrapped)` |
| `paused` | `Symbol("paused")` | `Address` (admin) — only emitted when built with the `pausable` feature |
| `unpaused` | `Symbol("unpaused")` | `Address` (admin) — only emitted when built with the `pausable` feature |

#### Reserve-backing invariant

The wrapped-token contract mints wrapped tokens 1:1 against an underlying asset
held in its own balance. The core solvency invariant that must always hold is:

```
get_total_wrapped() <= get_reserve_balance()
```

- `get_total_wrapped()` returns the contract's internally-tracked wrapped supply.
- `get_reserve_balance()` reads the underlying asset's actual `balance()` of the
  contract address — i.e. the real reserve, independent of the contract's own
  bookkeeping.

A monitoring job should poll both view functions after every `wrapped` /
`unwrapped` event (or on a fixed interval) and alert immediately if
`get_total_wrapped() > get_reserve_balance()`, since that would mean wrapped
tokens are no longer fully backed. Under normal operation the two values are
equal; a persistent gap where the reserve exceeds the wrapped total is benign
(e.g. a direct transfer into the contract), but the wrapped total ever
exceeding the reserve indicates a critical accounting bug or an exploit and
should halt further `wrap` calls (see the admin `pause()` entry point, gated
behind the `pausable` feature) pending investigation.

| Signal | Detection | Threshold |
|--------|-----------|-----------|
| Reserve shortfall | `get_total_wrapped() > get_reserve_balance()` | Alert immediately (critical) — invoke `pause()` |
| Cap saturation | `wrapped_by(address)` approaching `max_wrap_per_address()` | Warn at > 90% of cap |

---

### Airdrop Contract Events

Events emitted by `contracts/airdrop`:

| Event | Topic[0] | Data |
|-------|----------|------|
| `root_set` | `Symbol("root_set")` | `Bytes` (new merkle root) |
| `claimed` | `Symbol("claimed")` | `(Address recipient, i128 amount)` |

`claim_batch` emits one `claimed` event per successful entry rather than a single batch event.

### Auction Contract Events

Events emitted by `contracts/auction`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `started` | `Symbol("started")` | `Address` (seller) | `(i128 start_price, u32 deadline)` |
| `bid_placed` | `Symbol("bid_placed")` | `Address` (bidder) | `i128` (amount) |
| `ended` | `Symbol("ended")` | `Address` (winner) | `i128` (amount) |
| `ended_no_bids` | `Symbol("ended_no_bids")` | — | `()` |
| `ended_reserve_not_met` | `Symbol("ended_reserve_not_met")` | `Address` (highest bidder) | `(i128 highest_bid, i128 reserve_price)` |
| `withdrawn` | `Symbol("withdrawn")` | `Address` (bidder) | `i128` (amount) |
| `deadline_extended` | `Symbol("deadline_extended")` | — | `u32` (new deadline) |
| `cancelled` | `Symbol("cancelled")` | `Address` (seller) | `()` |

`deadline_extended` fires whenever a bid lands inside the anti-sniping `extension_window`; a monitoring job tracking auction end times should re-read `get_info().deadline` after each `bid_placed` rather than caching the value from `started`.

### Ballot Contract Events

Events emitted by `contracts/ballot`:

| Event | Topic[0] | Data |
|-------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) |
| `voter_registered` | `Symbol("voter_registered")` | `Address` (voter) |
| `voter_deregistered` | `Symbol("voter_deregistered")` | `Address` (voter) |
| `voted` | `Symbol("voted")` | `(Address voter, u32 choice)` |
| `tally_result` | `Symbol("tally_result")` | `(i128 yes, i128 no)` — legacy two-choice tally |
| `tally_all_result` | `Symbol("tally_all_result")` | `Vec<i128>` (per-choice counts, in `choices` order) |

### Bonding Curve Contract Events

Events emitted by `contracts/bonding-curve`:

| Event | Topic[0] | Data |
|-------|----------|------|
| `initialized` | `Symbol("initialized")` | `(Address admin, Address token)` |
| `bought` | `Symbol("bought")` | `(Address buyer, i128 tokens, i128 cost)` |
| `sold` | `Symbol("sold")` | `(Address seller, i128 tokens, i128 proceeds)` |

### Crowdfund Contract Events

Events emitted by `contracts/crowdfund`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (creator) | `(i128 goal, u32 deadline)` |
| `pledged` | `Symbol("pledged")` | `Address` (pledger) | `(i128 amount, i128 new_total)` |
| `withdrawn` | `Symbol("withdrawn")` | `Address` (pledger) | `i128` (amount) |
| `claimed` | `Symbol("claimed")` | `Address` (creator) | `i128` (amount) |
| `refunded` | `Symbol("refunded")` | `Address` (pledger) | `i128` (amount) |
| `deadline_extended` | `Symbol("deadline_extended")` | `Address` (creator) | `u32` (new deadline) |

### DAO Contract Events

Events emitted by `contracts/dao`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin), `Address` (token) | `i128` (quorum) |
| `created` | `Symbol("created")` | `Address` (proposer) | `u32` (proposal_id) |
| `voted` | `Symbol("voted")` | `Address` (voter) | `(u32 proposal_id, bool support, i128 weight)` |
| `executed` | `Symbol("executed")` | — | `u32` (proposal_id) |
| `cancelled` | `Symbol("cancelled")` | `Address` (admin) | `u32` (proposal_id) |
| `prop_cancelled` | `Symbol("prop_cancelled")` | `Address` (proposer) | `u32` (proposal_id) |

`created` is emitted by `create_proposal`, `executed` by `execute_proposal`, `cancelled` by admin `cancel_proposal`, and `prop_cancelled` by the proposer's own `proposer_cancel_proposal` — distinguish the last two when alerting on cancellations, since they indicate different actors.

### Lottery Contract Events

Events emitted by `contracts/lottery`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | `i128` (ticket_price) |
| `ticket_purchased` | `Symbol("ticket_purchased")` | `Address` (buyer) | `()` |
| `committed` | `Symbol("committed")` | `Address` (admin) | `()` |
| `winner_drawn` | `Symbol("winner_drawn")` | `Address` (winner) | `i128` (prize) |
| `refund_claimed` | `Symbol("refund_claimed")` | `Address` (buyer) | `i128` (amount) |

`winner_drawn` fires once per winner selected by `draw` (up to `winner_count` times in one transaction) — an indexer counting winners per drawing should aggregate by transaction/ledger, not assume exactly one event per draw.

### Marketplace Contract Events

Events emitted by `contracts/marketplace`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `listed` | `Symbol("listed")` | `u64` (listing_id) | `(Address seller, i128 price)` |
| `sold` | `Symbol("sold")` | `u64` (listing_id) | `(Address buyer, i128 price)` |
| `cancel` | `Symbol("cancel")` | `u64` (listing_id) | `Address` (seller) |
| `swept` | `Symbol("swept")` | `u64` (listing_id) | `Address` (seller) |
| `offered` | `Symbol("offered")` | `u64` (listing_id) | `(Address buyer, i128 amount)` |
| `offracc` | `Symbol("offracc")` | `u64` (listing_id) | `(Address buyer, i128 amount)` |
| `offrcncl` | `Symbol("offrcncl")` | `u64` (listing_id) | `Address` (buyer) |

Topic symbols are truncated to fit `symbol_short!`'s 9-character limit: `cancel` = listing cancelled, `offered` = offer made, `offracc` = offer accepted, `offrcncl` = offer cancelled. As noted in `error-reference.md` and `contract-api.md`, `contracts/marketplace/src/lib.rs` currently has no working code path that emits `offered` or `offracc` (offer creation isn't implemented, and the offer-accept logic is bound to a broken duplicate function) — an indexer should not expect to see those two in practice until that's fixed.

### Multisig Contract Events

Events emitted by `contracts/multisig`:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `u32` (threshold) | — | `u32` (signer_count) |
| `added` | `Symbol("added")` | `Address` (signer) | — | `u32` (new threshold) |
| `removed` | `Symbol("removed")` | `Address` (signer) | — | `u32` (new threshold) |
| `proposed` | `Symbol("proposed")` | `Address` (proposer) | — | `u64` (tx_id) |
| `signed` | `Symbol("signed")` | `Address` (signer) | `u64` (tx_id) | `u32` (signature_count) |
| `executed` | `Symbol("executed")` | `u64` (tx_id) | — | `()` |
| `expired` | `Symbol("expired")` | `u64` (tx_id) | — | `()` |
| `batch_executed` | `Symbol("batch_executed")` | — | — | `(Vec<u64> executed_ids, u32 skipped_count)` |

`batch_executed` summarizes an `execute_batch` call; cross-reference `executed_ids` against individual `executed` events if per-transaction detail is needed for skipped entries.

### NFT Contract Events

Events emitted by `contracts/nft`:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | — | `(String name, String symbol)` |
| `minted` | `Symbol("minted")` | `Address` (to) | — | `u32` (token_id) |
| `transferred` | `Symbol("transferred")` | `Address` (from) | `Address` (to) | `u32` (token_id) |
| `burned` | `Symbol("burned")` | `Address` (from) | — | `u32` (token_id) |
| `approved` | `Symbol("approved")` | `Address` (owner) | `Address` (spender) | `u32` (token_id) |

`burned` is defined in `events.rs` but there is currently no `burn` entry point in `lib.rs` that calls it — an indexer should not expect to observe this event until burning is implemented.

### Oracle Contract Events

Events emitted by `contracts/oracle`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | `u32` (staleness_threshold) |
| `price_updated` | `Symbol("price_updated")` | `Address` (admin) | `(i128 price, u32 ledger)` |
| `price_submitted` | `Symbol("price_submitted")` | `Address` (publisher) | `(i128 price, u64 timestamp)` |

`price_updated` comes from the single-admin `update_price` path; `price_submitted` comes from the multi-publisher `submit_price` path used by `get_median_price`/`get_twap`. A monitoring job computing price-change alerts should watch both.

### Staking Contract Events

Events emitted by `contracts/staking`:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | — | `(Address stake_token, Address reward_token)` |
| `staked` | `Symbol("staked")` | `Address` (staker) | — | `(i128 amount, i128 new_total)` |
| `unstaked` | `Symbol("unstaked")` | `Address` (staker) | — | `(i128 amount, i128 remaining)` |
| `claimed_rewards` | `Symbol("claimed_rewards")` | `Address` (staker) | — | `i128` (amount) |
| `added_rewards` | `Symbol("added_rewards")` | `Address` (admin) | — | `(i128 amount, i128 new_total)` |
| `compounded` | `Symbol("compounded")` | `Address` (staker) | — | `(i128 reward, i128 new_stake)` |
| `slashed` | `Symbol("slashed")` | `Address` (admin) | `Address` (staker) | `(i128 amount, Address destination)` |
| `unbond_requested` | `Symbol("unbond_requested")` | `Address` (staker) | — | `(i128 amount, u32 available_at)` |
| `withdrawn` | `Symbol("withdrawn")` | `Address` (staker) | — | `i128` (amount) |

`slashed` is a high-severity signal worth alerting on every occurrence. `unbond_requested`'s `available_at` is the ledger sequence after which the corresponding `withdrawn` becomes callable — useful for building an unbonding-queue dashboard.

### Swap Contract Events

Events emitted by `contracts/swap`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `proposed` | `Symbol("proposed")` | `Address` (party_a) | `(u32 swap_id, Address token_a, i128 amount_a, Address token_b, i128 amount_b, u32 expires_at)` |
| `accepted` | `Symbol("accepted")` | `Address` (party_b) | `u32` (swap_id) |
| `cancelled` | `Symbol("cancelled")` | — | `u32` (swap_id) |

As noted in `error-reference.md` and `contract-api.md`, `contracts/swap/src/lib.rs` currently calls `events::swap_cancelled` with a mismatched argument count relative to the `swap_cancelled(env, swap_id)` signature in `events.rs`; this table reflects the authoritative `events.rs` definition.

### Timelock Contract Events

Events emitted by `contracts/timelock`:

| Event | Topic[0] | Topic[1] | Topic[2] | Data |
|-------|----------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (admin) | `Address` (beneficiary) | `(u32 release_ledger, i128 amount)` |
| `released` | `Symbol("released")` | `Address` (beneficiary) | — | `i128` (amount) |
| `cancelled` | `Symbol("cancelled")` | `Address` (admin) | — | `i128` (amount) |
| `beneficiary_reassigned` | `Symbol("beneficiary_reassigned")` | `Address` (admin) | `Address` (old beneficiary), `Address` (new beneficiary) | `()` |

For a multi-tranche timelock (`initialize_with_tranches`), `initialized` reports the first tranche's ledger/amount and `released` may fire multiple times (once per tranche release) before the timelock's state finally transitions to `Released`.

### Vesting Contract Events

Events emitted by `contracts/vesting`:

| Event | Topic[0] | Topic[1] | Data |
|-------|----------|----------|------|
| `initialized` | `Symbol("initialized")` | `Address` (beneficiary) | `(i128 amount, u32 cliff_ledger, u32 end_ledger)` |
| `claimed` | `Symbol("claimed")` | `Address` (beneficiary) | `i128` (amount) |
| `revoked` | `Symbol("revoked")` | `Address` (beneficiary) | `(Address admin, i128 returned)` |
| `admin_released` | `Symbol("admin_released")` | `Address` (admin) | `i128` (amount) |

---

## 4. Decoding Events

Events are XDR-encoded on-chain. Use the Stellar CLI or SDK to decode them.

### Stellar CLI

```bash
# Decode a raw XDR event value
stellar xdr decode --type ScVal --xdr <BASE64_XDR>

# Fetch and decode the last 20 events for a contract
stellar contract events \
  --id <CONTRACT_ID> \
  --network testnet \
  --output json | jq .
```

### JavaScript SDK

```ts
import { xdr } from '@stellar/stellar-sdk';

function decodeTopics(rawTopics: string[]): any[] {
  return rawTopics.map((t) => xdr.ScVal.fromXDR(t, 'base64'));
}

function decodeData(rawData: string): any {
  return xdr.ScVal.fromXDR(rawData, 'base64');
}
```

---

## 5. Indexing Events with a Custom Indexer

For production systems, pull events in batches and store them in a database for fast queries.

### Polling architecture

```
Horizon → Batch poller (every N seconds) → Database → API / alerts
```

### Example batch poller (TypeScript)

```ts
import { Horizon } from '@stellar/stellar-sdk';
import { xdr } from '@stellar/stellar-sdk';

const server = new Horizon.Server('https://horizon-testnet.stellar.org');
let cursor = 'now';

async function pollEvents(contractId: string) {
  const page = await server
    .contractEvents(contractId)
    .cursor(cursor)
    .limit(200)
    .call();

  for (const record of page.records) {
    const eventName = xdr.ScVal.fromXDR(record.topic[0], 'base64').sym().toString();
    cursor = record.pagingToken;
    await storeEvent({ contractId, eventName, record });
  }
}

setInterval(() => pollEvents('<CONTRACT_ID>'), 5_000);
```

### Recommended database schema

```sql
CREATE TABLE contract_events (
  id            BIGSERIAL PRIMARY KEY,
  contract_id   TEXT NOT NULL,
  ledger        BIGINT NOT NULL,
  event_name    TEXT NOT NULL,
  topic1        TEXT,
  topic2        TEXT,
  topic3        TEXT,
  data          TEXT,
  paging_token  TEXT UNIQUE NOT NULL,
  created_at    TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX ON contract_events (contract_id, event_name);
CREATE INDEX ON contract_events (ledger);
```

---

## 6. Monitoring with Stellar Expert

[Stellar Expert](https://stellar.expert/explorer/testnet) provides a hosted explorer with contract event history, no setup required.

1. Navigate to `https://stellar.expert/explorer/testnet/contract/<CONTRACT_ID>`
2. Click the **Events** tab to view raw event history
3. Use the search bar to filter by event name or involved address
4. Subscribe to email or webhook alerts via the Stellar Expert notification API

---

## 7. Anomaly Detection

Monitor these patterns to detect unexpected contract behavior:

### Token contract anomalies

| Signal | Detection | Threshold |
|--------|-----------|-----------|
| Large mint | `mint` event with data > expected cap | Alert if `amount > MAX_MINT_AMOUNT` |
| Mint to unknown address | `mint` event with unknown `to` | Alert if `to` not in whitelist |
| Admin changed unexpectedly | `admin_changed` event | Alert on every occurrence |
| Abnormal burn rate | Rate of `burn` events | Alert if rate > 3× 7-day average |
| Upgrade proposed | `upgrade_proposed` event | Alert immediately; review wasm hash |

### Escrow contract anomalies

| Signal | Detection | Threshold |
|--------|-----------|-----------|
| Dispute spike | Rate of `dispute_raised` events | Alert if rate > 5% of funded escrows |
| Large refund | `funds_refunded` with high amount | Alert if `amount > HIGH_VALUE_THRESHOLD` |
| Stalled escrow | Escrow in `Funded` state past deadline | Alert 24 h before deadline |
| Upgrade proposed | `upgrade_proposed` event | Alert immediately |
| Repeated cancellations | `escrow_cancelled` from the same buyer | Alert if > 3 in 24 h |

---

## 8. Recommended Alerting Thresholds

Adjust these defaults for your specific deployment:

| Metric | Warning | Critical |
|--------|---------|----------|
| Events processed per minute | < 10 (processing lag) | 0 (indexer down) |
| Dispute rate | > 2% of active escrows | > 10% |
| Failed charges (subscription) | > 5% of subscribers | > 20% |
| Single transaction value | > $10,000 equivalent | > $100,000 equivalent |
| Upgrade timelock remaining | < 12 h | 0 (executing immediately) |
| Consecutive `admin_changed` | ≥ 1 | ≥ 2 in 1 h |

---

## 9. Health Check Script

Use the provided script to quickly poll contract state from the CLI:

```bash
# Check escrow status
./scripts/monitor-escrow.sh testnet <ESCROW_CONTRACT_ID>
```

See [`scripts/monitor-escrow.sh`](../scripts/monitor-escrow.sh) for details.

---

## 10. Prometheus Alert Definitions

The thresholds in §8 translate directly into Prometheus alerting rules. These
assume an exporter (e.g. the custom indexer from §5) publishes the following
metrics, labelled by `contract_id`:

| Metric | Type | Description |
|--------|------|-------------|
| `soroban_events_processed_total` | counter | Events ingested by the indexer |
| `soroban_escrow_disputes_total` | counter | `dispute_raised` events observed |
| `soroban_escrow_funded_total` | counter | `escrow_funded` events observed |
| `soroban_transfer_amount` | histogram/gauge | Per-transfer value (base units) |
| `soroban_upgrade_events_total` | counter | `upgrade_proposed` / `upgrade_executed` events |
| `soroban_subscription_charge_failed_total` | counter | Failed `charged` attempts |
| `soroban_subscription_charge_total` | counter | Total `charged` attempts |

Save as `monitoring/alerts.yml` and load it from your Prometheus config
(`rule_files: [ "alerts.yml" ]`):

```yaml
groups:
  - name: soroban-contracts
    rules:
      # Indexer has stopped ingesting events entirely.
      - alert: SorobanIndexerDown
        expr: rate(soroban_events_processed_total[5m]) == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "No contract events processed for {{ $labels.contract_id }}"
          description: "The indexer has ingested 0 events in the last 5 minutes."

      # Event-processing lag: throughput dropped well below normal.
      - alert: SorobanEventProcessingLag
        expr: rate(soroban_events_processed_total[5m]) * 60 < 10
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Event processing lag for {{ $labels.contract_id }}"
          description: "Fewer than 10 events/min processed over the last 10 minutes."

      # Repeated disputes: dispute rate exceeds 10% of funded escrows.
      - alert: SorobanEscrowDisputeSpike
        expr: |
          (
            increase(soroban_escrow_disputes_total[1h])
            / clamp_min(increase(soroban_escrow_funded_total[1h]), 1)
          ) > 0.10
        for: 15m
        labels:
          severity: critical
        annotations:
          summary: "Escrow dispute spike on {{ $labels.contract_id }}"
          description: ">10% of funded escrows raised a dispute in the last hour."

      # Large escrow / transfer: a single transfer exceeds the high-value cap.
      - alert: SorobanLargeTransfer
        expr: max_over_time(soroban_transfer_amount[5m]) > 1e11
        for: 0m
        labels:
          severity: warning
        annotations:
          summary: "Large transfer on {{ $labels.contract_id }}"
          description: "A transfer above the high-value threshold was observed. Review immediately."

      # Failed upgrades / upgrade activity: alert on every upgrade event.
      - alert: SorobanUpgradeActivity
        expr: increase(soroban_upgrade_events_total[10m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Upgrade activity on {{ $labels.contract_id }}"
          description: "An upgrade was proposed or executed. Verify the WASM hash and timelock."

      # Subscription charge failures above 20% of attempts.
      - alert: SorobanChargeFailureRate
        expr: |
          (
            increase(soroban_subscription_charge_failed_total[1h])
            / clamp_min(increase(soroban_subscription_charge_total[1h]), 1)
          ) > 0.20
        for: 15m
        labels:
          severity: critical
        annotations:
          summary: "High subscription charge-failure rate on {{ $labels.contract_id }}"
          description: ">20% of charge attempts failed in the last hour."
```

Tune the numeric thresholds (`1e11` base units, `0.10`, `0.20`, …) to match the
decimals and risk profile of your specific deployment — see the tables in §8.

---

## 11. Grafana Dashboard

Import the JSON below via **Dashboards → New → Import** in Grafana. It expects a
Prometheus datasource and the metrics described in §10. Replace the
`${DS_PROMETHEUS}` datasource variable when prompted.

```json
{
  "annotations": { "list": [] },
  "editable": true,
  "schemaVersion": 39,
  "title": "Soroban Contract Monitoring",
  "tags": ["soroban", "stellar", "contracts"],
  "time": { "from": "now-6h", "to": "now" },
  "templating": {
    "list": [
      {
        "name": "contract_id",
        "type": "query",
        "datasource": "${DS_PROMETHEUS}",
        "query": "label_values(soroban_events_processed_total, contract_id)",
        "includeAll": true,
        "multi": true
      }
    ]
  },
  "panels": [
    {
      "type": "timeseries",
      "title": "Events processed / min",
      "gridPos": { "h": 8, "w": 12, "x": 0, "y": 0 },
      "datasource": "${DS_PROMETHEUS}",
      "targets": [
        {
          "expr": "rate(soroban_events_processed_total{contract_id=~\"$contract_id\"}[5m]) * 60",
          "legendFormat": "{{contract_id}}"
        }
      ]
    },
    {
      "type": "stat",
      "title": "Dispute rate (1h)",
      "gridPos": { "h": 8, "w": 12, "x": 12, "y": 0 },
      "datasource": "${DS_PROMETHEUS}",
      "fieldConfig": {
        "defaults": {
          "unit": "percentunit",
          "thresholds": {
            "mode": "absolute",
            "steps": [
              { "color": "green", "value": null },
              { "color": "yellow", "value": 0.02 },
              { "color": "red", "value": 0.1 }
            ]
          }
        }
      },
      "targets": [
        {
          "expr": "increase(soroban_escrow_disputes_total{contract_id=~\"$contract_id\"}[1h]) / clamp_min(increase(soroban_escrow_funded_total{contract_id=~\"$contract_id\"}[1h]), 1)",
          "legendFormat": "{{contract_id}}"
        }
      ]
    },
    {
      "type": "timeseries",
      "title": "Max transfer value (5m)",
      "gridPos": { "h": 8, "w": 12, "x": 0, "y": 8 },
      "datasource": "${DS_PROMETHEUS}",
      "targets": [
        {
          "expr": "max_over_time(soroban_transfer_amount{contract_id=~\"$contract_id\"}[5m])",
          "legendFormat": "{{contract_id}}"
        }
      ]
    },
    {
      "type": "timeseries",
      "title": "Upgrade events (10m)",
      "gridPos": { "h": 8, "w": 12, "x": 12, "y": 8 },
      "datasource": "${DS_PROMETHEUS}",
      "targets": [
        {
          "expr": "increase(soroban_upgrade_events_total{contract_id=~\"$contract_id\"}[10m])",
          "legendFormat": "{{contract_id}}"
        }
      ]
    },
    {
      "type": "stat",
      "title": "Subscription charge-failure rate (1h)",
      "gridPos": { "h": 8, "w": 24, "x": 0, "y": 16 },
      "datasource": "${DS_PROMETHEUS}",
      "fieldConfig": {
        "defaults": {
          "unit": "percentunit",
          "thresholds": {
            "mode": "absolute",
            "steps": [
              { "color": "green", "value": null },
              { "color": "yellow", "value": 0.05 },
              { "color": "red", "value": 0.2 }
            ]
          }
        }
      },
      "targets": [
        {
          "expr": "increase(soroban_subscription_charge_failed_total{contract_id=~\"$contract_id\"}[1h]) / clamp_min(increase(soroban_subscription_charge_total{contract_id=~\"$contract_id\"}[1h]), 1)",
          "legendFormat": "{{contract_id}}"
        }
      ]
    }
  ]
}
```

> Tip: keep this JSON under version control (e.g. `monitoring/dashboard.json`)
> and provision it automatically with the Grafana
> [dashboard provisioning](https://grafana.com/docs/grafana/latest/administration/provisioning/#dashboards)
> config so it is recreated on every environment.

---
## 11. Example Grafana Dashboard

An importable starter dashboard is checked in at [`examples/grafana/contract-events.json`](../examples/grafana/contract-events.json). It visualizes event volume, event-processing error rate, and errors observed in the last hour, with a contract selector for operators.

The dashboard expects a Prometheus adapter to expose these counters with a `contract` label:

| Metric | Required label | Meaning |
|---|---|---|
| `soroban_contract_events_total` | `contract` | Counter of decoded events by contract |
| `soroban_contract_events_errors_total` | `contract` | Counter of event decoding or processing errors by contract |

To import it, open **Dashboards → Import** in Grafana, upload the JSON file, select the Prometheus data source, and click **Import**. If the adapter uses different metric names, update the panel PromQL expressions after import. The `clamp_min` expression in the error-rate panel avoids division by zero for contracts with no recent events.

This export is an operator starting point rather than a complete alert policy. Pair it with alerts for sustained ingestion errors, missing event traffic, RPC failures, and contract-specific invariants such as the wrapped-token reserve check documented above.

## 12. Resources

- [Horizon Events API](https://developers.stellar.org/docs/data/horizon/api-reference/resources/get-events-by-contract-id)
- [Stellar CLI contract events](https://developers.stellar.org/docs/tools/stellar-cli)
- [Stellar Expert Explorer](https://stellar.expert)
- [soroban-sdk event docs](https://docs.rs/soroban-sdk/latest/soroban_sdk/struct.Events.html)
- [XDR types reference](https://developers.stellar.org/docs/learn/fundamentals/transactions/list-of-operations)


## 8. Failed-Transaction Alert Rules

The examples below assume an indexer exports monotonically increasing counters named `soroban_transactions_total` and `soroban_transaction_failures_total`, each labeled with `contract_id` and `contract_name`. Adjust the window and thresholds to the normal traffic profile of each deployment; use `for` to avoid paging on a single transient failure.

```yaml
groups:
  - name: soroban-transaction-health
    rules:
      - alert: SorobanContractFailureRateHigh
        expr: |
          (
            sum by (contract_id, contract_name) (
              increase(soroban_transaction_failures_total[10m])
            )
            /
            clamp_min(
              sum by (contract_id, contract_name) (
                increase(soroban_transactions_total[10m])
              ), 1
            )
          ) > 0.10
        for: 15m
        labels:
          severity: warning
        annotations:
          summary: "Elevated failed-transaction rate for {{ $labels.contract_name }}"
          description: "More than 10% of transactions failed for 15 minutes. Check recent deployments, authorization errors, and dependency health."

      - alert: SorobanContractFailureRateCritical
        expr: |
          (
            sum by (contract_id, contract_name) (
              increase(soroban_transaction_failures_total[5m])
            )
            /
            clamp_min(
              sum by (contract_id, contract_name) (
                increase(soroban_transactions_total[5m])
              ), 1
            )
          ) > 0.25
          and
          sum by (contract_id, contract_name) (
            increase(soroban_transactions_total[5m])
          ) >= 20
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Critical failed-transaction rate for {{ $labels.contract_name }}"
          description: "At least 20 transactions were observed and more than 25% failed in each five-minute window. Investigate immediately and consider pausing the affected contract."

      - alert: SorobanContractFailureBurst
        expr: |
          sum by (contract_id, contract_name) (
            increase(soroban_transaction_failures_total[5m])
          ) >= 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Failed-transaction burst for {{ $labels.contract_name }}"
          description: "At least 10 failed transactions occurred in five minutes. This catches incidents even when total traffic is too low for a percentage threshold."
```

The 10% warning threshold is intended to identify degradation before it becomes a widespread outage. The 25% critical threshold requires at least 20 attempts so that a low-volume contract does not page on a single failure. The burst rule is complementary: it detects a sustained absolute spike and is useful for contracts with uneven traffic. Route alerts by `contract_id`, include the deployment environment in the metric labels, and link each alert to the relevant runbook and transaction/event dashboard.
