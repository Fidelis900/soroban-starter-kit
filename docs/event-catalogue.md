# Event Catalogue

This document lists every event emitted by every contract in the Soroban Starter Kit. For each event, the table shows the event symbol, topic types, data type, and when it is fired.

## Event Publishing Format

In Soroban, events are published as:

```rust
env.events().publish((topic_1, topic_2, ...), data);
```

- **Topics** are indexed fields used for filtering and queries. Each event has 0–3 topics.
- **Data** is the unindexed payload containing details about the event.

---

## Token Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, admin | `(String, String, u32)` → name, symbol, decimals | `initialize()` called |
| Minted | `mint` | `(Symbol, Address)` → event name, recipient | `i128` → amount minted | `mint()` called |
| Burned | `burn` | `(Symbol, Address)` → event name, account | `i128` → amount burned | `burn()` or `burn_from()` called |
| Admin Changed | `admin_changed` | `(Symbol, Address)` → event name, old admin | `Address` → new admin | `set_admin()` called |
| Admin Proposed | `admin_proposed` | `(Symbol, Address)` → event name, current admin | `Address` → pending admin | `propose_admin()` called |
| Admin Accepted | `admin_accepted` | `(Symbol, Address)` → event name, new admin | `()` | `accept_admin()` called |
| Admin Proposal Cancelled | `admin_proposal_cancelled` | `(Symbol, Address)` → event name, admin | `()` | `cancel_admin_transfer()` called |
| Approved | `approve` | `(Symbol, Address, Address)` → event name, owner, spender | `i128` → allowance amount | `approve()` called |
| Revoked | `revoke` | `(Symbol, Address, Address)` → event name, owner, spender | `()` | `approve()` called with amount 0 |
| Transferred | `transfer` | `(Symbol, Address, Address)` → event name, from, to | `i128` → amount transferred | `transfer()` or `transfer_from()` called |
| Account Frozen | `account_frozen` | `(Symbol, Address)` → event name, account | `()` | `freeze_account()` called |
| Account Unfrozen | `account_unfrozen` | `(Symbol, Address)` → event name, account | `()` | `unfreeze_account()` called |
| Paused | `paused` | `(Symbol, Address)` → event name, admin | `()` | `pause()` called (pausable feature) |
| Unpaused | `unpaused` | `(Symbol, Address)` → event name, admin | `()` | `unpause()` called (pausable feature) |
| Upgraded | `upgraded` | `(Symbol, Address)` → event name, admin | `BytesN<32>` → new WASM hash | `execute_upgrade()` called (upgradeable feature) |
| Transfer Hook Set | `hook_set` | `(Symbol, Address)` → event name, admin | `Option<Address>` → hook address (`None` to clear) | `set_transfer_hook()` called |
| Snapshot Taken | `snapshot` | `(Symbol, Address, u32)` → event name, account, ledger | `i128` → recorded balance | `snapshot()` called (governance balance snapshots) |
| Permit Signer Set | `permit_signer_set` | `(Symbol, Address)` → event name, owner | `()` | `set_permit_signer()` called |
| Permit Used | `permit_used` | `(Symbol, Address, Address)` → event name, owner, spender | `(i128, u32)` → amount, nonce | `approve_with_signature()` succeeds |

---

## Escrow Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address, Address, Address)` → event name, buyer, seller, arbiter | `i128` → amount | `initialize()` called |
| Escrow Created | `created` | `(Symbol, Address, Address)` → event name, buyer, seller | `i128` → amount | `create()` called |
| Escrow Funded | `funded` | `(Symbol, Address)` → event name, buyer | `i128` → amount funded | `fund()` called |
| Delivery Marked | `marked_delivered` | `(Symbol, Address)` → event name, seller | `()` | `mark_delivered()` called |
| Funds Released | `released` | `(Symbol, Address)` → event name, seller | `i128` → amount released | `release()` called |
| Partial Release | `released_partial` | `(Symbol, Address)` → event name, seller | `i128` → partial amount | `partial_release()` called |
| Funds Refunded | `refunded` | `(Symbol, Address)` → event name, buyer | `i128` → amount refunded | `refund()` called (deadline passed) |
| Amount Updated | `amount_updated` | `(Symbol, Address)` → event name, buyer | `i128` → new amount | `update_amount()` called |
| Escrow Cancelled | `escrow_cancelled` | `(Symbol, Address)` → event name, buyer | `()` | `cancel()` called |
| Deadline Extended | `deadline_extended` | `(Symbol, Address)` → event name, buyer | `u32` → new deadline ledger | `extend_deadline()` called |
| Dispute Raised | `dispute_raised` | `(Symbol, Address)` → event name, caller | `()` | `raise_dispute()` called |
| Dispute Timeout Claimed | `dispute_timeout` | `(Symbol, Address)` → event name, buyer | `i128` → amount claimed | `claim_dispute_timeout()` called |
| Milestone Released | `milestone_released` | `(Symbol, Address, u32)` → event name, seller, milestone index | `(i128, i128)` → amount, fee | `release_milestone()` called |
| Fee Config Set | `fee_config_set` | `(Symbol, Address)` → event name, admin | `(u32, Address)` → fee bps, treasury | `set_fee_config()` called |
| Paused | `paused` | `(Symbol, Address)` → event name, admin | `()` | `pause()` called |
| Unpaused | `unpaused` | `(Symbol, Address)` → event name, admin | `()` | `unpause()` called |
| Upgraded | `upgraded` | `(Symbol, Address)` → event name, admin | `BytesN<32>` → new WASM hash | `execute_upgrade()` called |

---

## Staking Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, admin | `(Address, Address)` → stake token, reward token | `initialize()` called |
| Staked | `staked` | `(Symbol, Address)` → event name, staker | `(i128, i128)` → amount staked, new total | `stake()` called |
| Unstaked | `unstaked` | `(Symbol, Address)` → event name, staker | `(i128, i128)` → amount unstaked, remaining stake | `unstake()` called |
| Rewards Claimed | `claimed_rewards` | `(Symbol, Address)` → event name, staker | `i128` → reward amount claimed | `claim_rewards()` called |
| Rewards Added | `added_rewards` | `(Symbol, Address)` → event name, admin | `(i128, i128)` → reward amount, new total | `add_rewards()` called |
| Compounded | `compounded` | `(Symbol, Address)` → event name, staker | `(i128, i128)` → reward compounded, new stake | `compound()` called (auto-compounding) |
| Slashed | `slashed` | `(Symbol, Address, Address)` → event name, admin, staker | `(i128, Address)` → amount slashed, destination | `slash()` called by admin |
| Unbond Requested | `unbond_requested` | `(Symbol, Address)` → event name, staker | `(i128, u32)` → amount queued, available-at ledger | `unstake()` called when `unbonding_period > 0` |
| Withdrawn | `withdrawn` | `(Symbol, Address)` → event name, staker | `i128` → amount withdrawn | `withdraw()` called after unbonding period |

---

## Multisig Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, u32)` → event name, threshold | `u32` → signer count | `initialize()` called |
| Signer Added | `added` | `(Symbol, Address)` → event name, signer | `(u32, u32)` → signer weight, new threshold | `add_signer()` or `execute_signer_change()` called |
| Signer Weight Updated | `weight_updated` | `(Symbol, Address)` → event name, signer | `(u32, u32)` → old weight, new weight | `update_signer_weight()` or `execute_signer_change()` called |
| Threshold Changed | `threshold_changed` | `(Symbol,)` → event name | `(u32, u32)` → old threshold, new threshold | `execute_signer_change()` with `ChangeThreshold` |
| Signer Removed | `removed` | `(Symbol, Address)` → event name, signer | `u32` → new threshold | `remove_signer()` or `execute_signer_change()` called |
| Transaction Proposed | `proposed` | `(Symbol, Address)` → event name, proposer | `u64` → transaction ID | `propose()` called |
| Transaction Signed | `signed` | `(Symbol, Address, u64)` → event name, signer, tx ID | `u32` → signature count | `sign()` called |
| Transaction Executed | `executed` | `(Symbol, u64)` → event name, tx ID | `()` | `execute()` called (threshold met) |
| Proposal Expired | `expired` | `(Symbol, u64)` → event name, tx ID | `()` | `cleanup_expired()` called |
| Transaction Cancelled | `cancelled` | `(Symbol, Address)` → event name, proposer | `u64` → transaction ID | `cancel_proposal()` called |
| Signature Revoked | `revoked` | `(Symbol, Address, u64)` → event name, signer, tx ID | `(u32, u32)` → signature count, accumulated weight | `revoke_signature()` called |
| Signer Change Proposed | `signer_change_proposed` | `(Symbol, Address)` → event name, proposer | `u64` → signer proposal ID | `propose_signer_change()` called |
| Signer Change Signed | `signer_change_signed` | `(Symbol, Address, u64)` → event name, signer, proposal ID | `u32` → signature count | `sign_signer_change()` called |
| Signer Change Executed | `signer_change_executed` | `(Symbol, u64)` → event name, proposal ID | `()` | `execute_signer_change()` called |
| Batch Executed | `batch_executed` | `(Symbol,)` → event name | `(Vec<u64>, u32)` → executed IDs, skipped count | `execute_batch()` called |

---

## DAO Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address, Address)` → event name, admin, token | `i128` → quorum | `initialize()` called |
| Proposal Created | `created` | `(Symbol, Address)` → event name, proposer | `u32` → proposal ID | `create_proposal()` called |
| Voted | `voted` | `(Symbol, Address)` → event name, voter | `(u32, bool, i128)` → proposal ID, support, voting weight | `vote()` called |
| Proposal Executed | `executed` | `(Symbol,)` → event name | `u32` → proposal ID | `execute()` called (quorum + majority met) |
| Proposal Cancelled | `cancelled` | `(Symbol, Address)` → event name, admin | `u32` → proposal ID | `cancel_proposal()` called (admin) |
| Proposer Cancelled | `prop_cancelled` | `(Symbol, Address)` → event name, proposer | `u32` → proposal ID | `proposer_cancel_proposal()` called (proposer self-cancellation) |

---

## Timelock Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address, Address)` → event name, admin, beneficiary | `(u32, i128)` → release ledger, amount | `initialize()` called |
| Released | `released` | `(Symbol, Address)` → event name, beneficiary | `i128` → amount released | `release()` called |
| Cancelled | `cancelled` | `(Symbol, Address)` → event name, admin | `i128` → amount returned to admin | `cancel()` called |
| Beneficiary Reassigned | `beneficiary_reassigned` | `(Symbol, Address, Address, Address)` → event name, admin, old beneficiary, new beneficiary | `()` | `reassign_beneficiary()` called |

---

## Airdrop Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Merkle Root Set | `root_set` | `(Symbol,)` → event name | `Bytes` → new Merkle root | `set_root()` called by admin |
| Claimed | `claimed` | `(Symbol,)` → event name | `(Address, i128)` → recipient, amount | `claim()` called with valid Merkle proof |

---

## Auction Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Started | `started` | `(Symbol, Address)` → event name, seller | `(i128, u32)` → start price, deadline ledger | `start()` called |
| Bid Placed | `bid_placed` | `(Symbol, Address)` → event name, bidder | `i128` → bid amount | `bid()` called |
| Outbid | `outbid` | `(Symbol, Address)` → event name, outbid bidder | `(i128, i128)` → outbid amount, new highest bid | `bid()` called when an existing highest bidder is displaced |
| Refund Queued | `refund_queued` | `(Symbol, Address)` → event name, refunded bidder | `i128` → refund amount queued | `bid()` called when previous highest bid is moved to pending refunds |
| Ended (winner) | `ended` | `(Symbol, Address)` → event name, winner | `i128` → winning amount | `end()` called, winner exists |
| Ended (no bids) | `ended_no_bids` | `(Symbol,)` → event name | `()` | `end()` called with no bids |
| Ended (reserve not met) | `ended_reserve_not_met` | `(Symbol, Address)` → event name, highest bidder | `(i128, i128)` → highest bid, reserve price | `end()` called, bid < reserve |
| Bid Withdrawn | `withdrawn` | `(Symbol, Address)` → event name, bidder | `i128` → amount returned | `withdraw()` called by losing bidder |
| Deadline Extended | `deadline_extended` | `(Symbol,)` → event name | `u32` → new deadline ledger | Anti-snipe window triggered during `bid()` |
| Cancelled | `cancelled` | `(Symbol, Address)` → event name, seller | `()` | `cancel()` called by seller (no bids placed) |
| Credit Applied | `credit_applied` | `(Symbol, Address)` → event name, bidder | `(i128, i128)` → credit used, amount transferred | `bid_with_credit()` called |
| Dutch Started | `dutch_started` | `(Symbol, Address)` → event name, seller | `(i128, i128, u32, u32)` → start price, floor price, start ledger, duration ledgers | `start_dutch()` called |
| Dutch Bought | `dutch_bought` | `(Symbol, Address)` → event name, buyer | `i128` → price paid | `buy()` settles a Dutch auction |
| NFT Escrowed | `nft_escrowed` | `(Symbol, Address)` → event name, NFT contract | `u32` → token id | `start()` / `start_dutch()` with a custodial NFT |
| NFT Released | `nft_released` | `(Symbol, Address)` → event name, recipient | `u32` → token id | NFT delivered to the winner/buyer or returned to the seller |

---

## Ballot Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol,)` → event name | `Address` → admin | `initialize()` called |
| Voter Registered | `voter_registered` | `(Symbol,)` → event name | `Address` → voter | `register_voter()` called |
| Voter Deregistered | `voter_deregistered` | `(Symbol,)` → event name | `Address` → voter | `deregister_voter()` called |
| Voted | `voted` | `(Symbol,)` → event name | `(Address, u32)` → voter, choice index | `vote()` called |
| Tally Result (binary) | `tally_result` | `(Symbol,)` → event name | `(i128, i128)` → yes count, no count | `tally()` called |
| Tally All Result (multi-choice) | `tally_all_result` | `(Symbol,)` → event name | `Vec<i128>` → per-choice vote counts | `tally_all()` called |

---

## Bonding Curve Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol,)` → event name | `(Address, Address)` → admin, token | `initialize()` called |
| Bought | `bought` | `(Symbol,)` → event name | `(Address, i128, i128)` → buyer, tokens received, cost paid | `buy()` called |
| Sold | `sold` | `(Symbol,)` → event name | `(Address, i128, i128)` → seller, tokens sold, proceeds received | `sell()` called |

---

## Crowdfund Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, creator | `(i128, u32)` → goal amount, deadline ledger | `initialize()` called |
| Pledged | `pledged` | `(Symbol, Address)` → event name, pledger | `(i128, i128)` → amount pledged, new running total | `pledge()` called |
| Withdrawn | `withdrawn` | `(Symbol, Address)` → event name, pledger | `i128` → amount withdrawn | `withdraw()` called before deadline |
| Claimed | `claimed` | `(Symbol, Address)` → event name, creator | `i128` → total amount claimed | `claim()` called after goal met |
| Refunded | `refunded` | `(Symbol, Address)` → event name, pledger | `i128` → amount refunded | `refund()` called after deadline, goal not met |
| Deadline Extended | `deadline_extended` | `(Symbol, Address)` → event name, creator | `u32` → new deadline ledger | `extend_deadline()` called by creator |

---

## Lottery Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, admin | `i128` → ticket price | `initialize()` called |
| Ticket Purchased | `ticket_purchased` | `(Symbol, Address)` → event name, buyer | `()` | `buy_ticket()` called |
| Committed | `committed` | `(Symbol, Address)` → event name, admin | `()` | `commit()` called with hash and reveal deadline |
| Winner Drawn | `winner_drawn` | `(Symbol, Address)` → event name, winner | `i128` → prize amount | Emitted once per winner during `draw()` |
| Refund Claimed | `refund_claimed` | `(Symbol, Address)` → event name, buyer | `i128` → refund amount | `claim_refund()` called after reveal deadline passed |

---

## Marketplace Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Listed | `listed` | `(Symbol, u64)` → event name, listing ID | `(Address, i128)` → seller, price | `list()` or `list_with_expiry()` called |
| Sold | `sold` | `(Symbol, u64)` → event name, listing ID | `(Address, i128)` → buyer, price | `buy()` called |
| Cancelled | `cancel` | `(Symbol, u64)` → event name, listing ID | `Address` → seller | `cancel()` called by seller |
| Swept | `swept` | `(Symbol, u64)` → event name, listing ID | `Address` → seller | `sweep_expired()` called after listing expiry |
| Offer Made | `offered` | `(Symbol, u64)` → event name, listing ID | `(Address, i128)` → buyer, offer amount | `make_offer()` called |
| Offer Accepted | `offracc` | `(Symbol, u64)` → event name, listing ID | `(Address, i128)` → buyer, accepted amount | `accept_offer()` called by seller |
| Offer Cancelled | `offrcncl` | `(Symbol, u64)` → event name, listing ID | `Address` → buyer | `cancel_offer()` called by buyer |

> **Note:** The `cancel`, `offracc`, and `offrcncl` symbols are abbreviated to fit Soroban's 9-byte `symbol_short!` limit.

---

## Oracle Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, admin | `u32` → staleness threshold (ledgers) | `initialize()` called |
| Price Updated | `price_updated` | `(Symbol, Address)` → event name, admin | `(i128, u32)` → price, ledger sequence | `update_price()` called by admin |
| Price Submitted | `price_submitted` | `(Symbol, Address)` → event name, publisher | `(i128, u64)` → price, timestamp | `submit_price()` called by authorized publisher |

---

## Subscription Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, provider | `Address` → payment token | `initialize()` called |
| Plan Registered | `plan_registered` | `(Symbol, Symbol)` → event name, plan ID | `(i128, u32)` → charge amount, interval in ledgers | `register_plan()` called by provider |
| Plan Updated | `plan_updated` | `(Symbol, Symbol)` → event name, plan ID | `bool` → new active status | `update_plan()` called by provider |
| Subscribed | `subscribed` | `(Symbol, Address)` → event name, subscriber | `(Symbol, i128, u32)` → plan ID, amount, interval ledgers | `subscribe()` called |
| Charged | `charged` | `(Symbol, Address, Address)` → event name, subscriber, provider | `i128` → amount charged | `charge()` called by provider |
| Cancelled | `cancelled` | `(Symbol, Address)` → event name, subscriber | `()` | `cancel()` called by subscriber |
| Trial Completed | `trial_completed` | `(Symbol, Address)` → event name, subscriber | `()` | `complete_trial()` called after trial period ends |

---

## Swap Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Proposed | `proposed` | `(Symbol, Address)` → event name, party A | `(u32, Address, i128, Address, i128, u32)` → swap ID, token A, amount A, token B, amount B, expires-at ledger | `propose()` called by party A |
| Accepted | `accepted` | `(Symbol, Address)` → event name, party B | `u32` → swap ID | `accept()` called by party B |
| Cancelled | `cancelled` | `(Symbol,)` → event name | `u32` → swap ID | `cancel()` called (by party A or after expiry) |

---

## Timelock Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address, Address)` → event name, admin, beneficiary | `(u32, i128)` → release ledger, amount | `initialize()` called |
| Released | `released` | `(Symbol, Address)` → event name, beneficiary | `i128` → amount released | `release()` called after release ledger |
| Cancelled | `cancelled` | `(Symbol, Address)` → event name, admin | `i128` → amount returned to admin | `cancel()` called by admin before release ledger |
| Beneficiary Reassigned | `beneficiary_reassigned` | `(Symbol, Address, Address, Address)` → event name, admin, old beneficiary, new beneficiary | `()` | `reassign_beneficiary()` called |

---

## Vesting Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol, Address)` → event name, beneficiary | `(i128, u32, u32)` → amount, cliff ledger, end ledger | `create_schedule()` called (`initialize()` only configures admin and token) |
| Claimed | `claimed` | `(Symbol, Address)` → event name, beneficiary | `i128` → amount claimed | `claim()` called |
| Revoked | `revoked` | `(Symbol, Address)` → event name, beneficiary | `(Address, i128)` → admin, amount returned to admin | `revoke()` called by admin |
| Admin Released | `admin_released` | `(Symbol, Address)` → event name, admin | `i128` → amount released to admin | `admin_release()` called after vesting end |

---

## Wrapped Token Contract

| Event | Symbol | Topics | Data Type | When Fired |
|-------|--------|--------|-----------|-----------|
| Initialized | `initialized` | `(Symbol,)` → event name | `(Address, Address)` → admin, underlying token | `initialize()` called |
| Wrapped | `wrapped` | `(Symbol,)` → event name | `(Address, i128, i128)` → user, amount wrapped, new total supply | `wrap()` called |
| Unwrapped | `unwrapped` | `(Symbol,)` → event name | `(Address, i128, i128)` → user, amount unwrapped, new total supply | `unwrap()` called |
| Paused | `paused` | `(Symbol, Address)` → event name, admin | `()` | `pause()` called (pausable feature only) |
| Unpaused | `unpaused` | `(Symbol, Address)` → event name, admin | `()` | `unpause()` called (pausable feature only) |

---

## Event Publishing Patterns

### Event Schema Versioning

Off-chain indexers must handle events whose shape may change between contract upgrades. The convention used across this kit is:

**Version tag in the first topic**

The first topic is always an event symbol. A `v` suffix signals the schema version:

```
(Symbol, ...)   →  topic[0] = "transfer"    (version 1, implied)
(Symbol, ...)   →  topic[0] = "transfer_v2" (version 2, explicit)
```

When a breaking change is made to an event's topic list or data type, a new symbol is introduced with an incremented suffix. The old symbol is kept emitting from the old code path for at least one major release, giving indexers time to migrate.

**Recommended indexer pattern**

```javascript
function handleEvent(event) {
  const sym = event.topic[0];
  if (sym === "transfer" || sym === "transfer_v1") {
    // original shape: data = i128
    handleTransferV1(event);
  } else if (sym === "transfer_v2") {
    // new shape: data = { amount: i128, memo: String }
    handleTransferV2(event);
  }
  // unknown future versions: log and skip, do not throw
}
```

**Rules**

1. Non-breaking additions (e.g. adding an optional field to a tuple) do not require a version bump; document them in the changelog.
2. Any change to the number or type of **topics** is always breaking and requires a new symbol.
3. Any change to the **data** type that is not backward-compatible (e.g. `i128` → `(i128, Address)`) requires a new symbol.
4. Deprecated symbols must be documented in this catalogue with a `**(deprecated as of vX.Y)**` note.
5. The `version` field is carried in the symbol name, not in the data payload, to remain filterable via topic queries.

---

### Indexing Strategy

Topics are indexed for efficient querying:

- **First topic (always)**: Event symbol (e.g., `initialized`, `transfer`, `voted`)
- **Second topic**: Primary actor (e.g., address performing action: sender, staker, proposer)
- **Third topic**: Secondary context (e.g., recipient, spender, or secondary party)

### Data Type Conventions

- Use **single values** if one piece of information: `i128`, `Address`, `u32`
- Use **tuples** for multi-value payloads: `(i128, u32)` for amount and ledger
- Use `()` (unit type) if no data needed beyond topics

### TTL Management

Events are broadcast to the Soroban network but are **not** subject to TTL extension. They are archived according to the network's archival policies (typically ~1 year of history).

---

## Querying Events

### Stellar SDK Example (JavaScript)

```javascript
import * as StellarSDK from "@stellar/stellar-sdk";

const server = new StellarSDK.Server(rpcUrl);
const ledgers = await server.getLedgers()
  .eventFilter({
    contractId: "CAAAA...",
    topics: ["transfer", addr],
    type: "contract",
  })
  .call();

ledgers.records.forEach((event) => {
  console.log("Topic:", event.topic);
  console.log("Data:", event.value);
});
```

### Grep Example (Off-Chain Indexer)

```bash
# Search for all "transfer" events in a JSON event log
jq '.[] | select(.topic[0] == "transfer")' events.json
```

---

## Consistency with Source

This catalogue is generated from the event emission calls in each contract's `src/events.rs`:

- Token: `contracts/token/src/events.rs`
- Escrow: `contracts/escrow/src/events.rs`
- Vesting: `contracts/vesting/src/events.rs`
- Staking: `contracts/staking/src/events.rs`
- Multisig: `contracts/multisig/src/events.rs`
- DAO: `contracts/dao/src/events.rs`
- Airdrop: `contracts/airdrop/src/events.rs`
- Auction: `contracts/auction/src/events.rs`
- Ballot: `contracts/ballot/src/events.rs`
- Bonding Curve: `contracts/bonding-curve/src/events.rs`
- Crowdfund: `contracts/crowdfund/src/events.rs`
- Lottery: `contracts/lottery/src/events.rs`
- Marketplace: `contracts/marketplace/src/events.rs`
- Oracle: `contracts/oracle/src/events.rs`
- Subscription: `contracts/subscription/src/events.rs`
- Swap: `contracts/swap/src/events.rs`
- Timelock: `contracts/timelock/src/events.rs`
- Wrapped Token: `contracts/wrapped-token/src/events.rs`

To keep this catalogue in sync, verify against the source before each release. A CI lint check validates that event names and topic signatures match the published code.

---

## See Also

- [Architecture: Event Model](architecture.md#event-model)
- [Soroban Events Documentation](https://soroban.stellar.org/docs/learn/events)
- [Integration Guide: Event Streams](integration-guide.md)