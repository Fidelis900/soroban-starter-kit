# Storage Layout Documentation

This document details the storage keys, types, and TTL management policy for each contract in the Soroban Starter Kit. Each contract uses a mix of **instance** and **persistent** storage to optimize gas costs and TTL extension overhead.

## Storage Tier Policies

| Tier | Lifetime | Use Case | TTL Extension |
|------|----------|----------|----------------|
| **Instance** | Tied to contract instance | Immutable or rarely-changed config; contract state | Extended once per transaction |
| **Persistent** | Independent; outlives contract upgrades | Per-user balances, allowances, proposals | Extended per operation or batch |
| **Temporary** | Ledger sequence + TTL window | Time-limited access tokens (allowances) | Not extended; expires naturally |

---

## Token Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Current contract administrator |
| `PendingAdmin` | Instance | `Address` | Extended on proposal/acceptance | Pending admin for two-step transfer |
| `Balance(Address)` | Persistent | `i128` | Extended on every balance change | Token balance per address |
| `Allowance(AllowanceDataKey)` | Temporary | `AllowanceValue` | Expires naturally | Approve(spender) with expiration ledger |
| `Metadata(MetadataKey)` | Instance | `String` / `u32` | Extended on init | Token name, symbol, decimals |
| `TotalSupply` | Persistent | `i128` | Extended on mint/burn | Total circulating supply |
| `Paused` | Instance | `bool` | Extended on pause/unpause | Whether contract is paused |
| `MaxSupply` | Instance | `i128` | Extended on init | Hard cap on supply (if capped-supply feature enabled) |
| `PendingUpgrade` | Instance | `(BytesN<32>, u32)` | Extended on upgrade proposal | Pending WASM hash + ready ledger |
| `Version` | Instance | `u32` | Extended on init | Contract version number |
| `Frozen(Address)` | Persistent | `bool` | Extended per freeze/unfreeze | Whether an address is frozen |

---

## Escrow Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Buyer` | Instance | `Address` | Extended on init | Buyer address |
| `Seller` | Instance | `Address` | Extended on init | Seller address |
| `Arbiter` | Instance | `Address` | Extended on init | Single arbiter (deprecated; see `Arbiters`) |
| `Arbiters` | Instance | `Vec<Address>` | Extended on init | Multi-sig arbiter group for resolution |
| `TokenContract` | Instance | `Address` | Extended on init | Soroban token contract address |
| `Amount` | Instance | `i128` | Extended on init | Escrowed amount in token units |
| `Deadline` | Instance | `u32` | Extended on init | Refund-eligible ledger sequence |
| `State` | Instance | `EscrowState` | Extended on state change | Current escrow lifecycle state |
| `Paused` | Instance | `bool` | Extended on pause/unpause | Whether contract is paused |
| `Version` | Instance | `u32` | Extended on init | Contract version number |
| `PendingUpgrade` | Instance | `(BytesN<32>, u32)` | Extended on upgrade proposal | Pending WASM hash + ready ledger |
| `RequiredSignatures` | Instance | `u32` | Extended on init | Threshold for multi-sig resolution |
| `ArbiterVotes` | Instance | `Vec<Address>` | Extended per vote | Arbiters who have voted to release/refund |

**Escrow States:** `Created → Funded → Delivered → Completed`

---

## Vesting Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Token` | Instance | `Address` | Extended on init | Token contract address |
| `Version` | Instance | `u32` | Extended on init | Contract version number |
| `AdminReleased` | Instance | `i128` | Extended on `admin_release` | Running total of tokens released early by admin (audit log) |
| `Schedule(Address)` | Persistent | `BeneficiarySchedule` | Extended per schedule change | Per-beneficiary vesting schedule |

**`BeneficiarySchedule` fields:** `amount`, `cliff_ledger`, `end_ledger`, `claimed`, `revoked`

**Vesting Release:** Tokens accrue linearly from `cliff_ledger` to `end_ledger` within each `Schedule(Address)`. Admin can revoke unvested tokens at any time; vested tokens remain claimable.

---

## Staking Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `StakeToken` | Instance | `Address` | Extended on init | Token that users stake |
| `RewardToken` | Instance | `Address` | Extended on init | Token distributed as rewards (may equal StakeToken) |
| `TotalStaked` | Instance | `i128` | Extended on every stake/unstake | Total staked across all stakers |
| `TotalRewards` | Instance | `i128` | Extended on reward deposit/claim | Unclaimed reward pool |
| `RewardPerTokenStored` | Instance | `i128` | Extended on reward snapshot | Global reward accumulator (scaled by 1e12) |
| `UnbondingPeriod` | Instance | `u32` | Extended on init | Ledgers between `unstake` and `withdraw` (0 = immediate) |
| `SlashDestination` | Instance | `Address` | Extended on init | Address that receives slashed tokens |
| `Version` | Instance | `u32` | Extended on init | Contract version number |
| `Stake(Address)` | Persistent | `i128` | Extended on stake/unstake | Per-staker staked amount |
| `RewardPerTokenPaid(Address)` | Persistent | `i128` | Extended on reward claim | Per-staker reward accumulator snapshot |
| `Rewards(Address)` | Persistent | `i128` | Extended on reward accrual/claim | Per-staker accrued rewards |
| `UnbondRequest(Address)` | Persistent | `UnbondRequest` | Extended on unbond/withdraw | Pending unbond request (`amount`, `available_at`) |
| `Compounding(Address)` | Persistent | `bool` | Extended on `set_compounding` | Whether auto-compounding is enabled for a staker |

---

## Multisig Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Signers` | Instance | `Vec<Address>` | Extended on signer change | List of authorized signers |
| `Weights` | Instance | `Map<Address, u32>` | Extended on init / signer change | Per-signer vote weight for weighted voting; signers not listed default to 1 |
| `Threshold` | Instance | `u32` | Extended on threshold change | Required number of signatures (N-of-M) |
| `NextTransactionId` | Instance | `u64` | Extended on proposal | Auto-incrementing transaction ID counter |
| `Transaction(u64)` | Persistent | `Transaction` | Extended on propose/sign/execute | Proposed transaction details and signatures |
| `Paused` | Instance | `bool` | Extended on pause/unpause | Whether contract is paused |
| `Version` | Instance | `u32` | Extended on init | Contract version number |

**Transaction Struct:** Contains proposer, target, function name, arguments, signatures, and execution status.

---

## DAO Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Token` | Instance | `Address` | Extended on init | Token used for voting power |
| `VotingPeriod` | Instance | `u32` | Extended on init | Proposal duration in ledgers |
| `Quorum` | Instance | `i128` | Extended on init | Minimum votes required for quorum |
| `QuorumBps` | Instance | `u32` | Extended on init | Minimum participation as basis points of total supply (0–10 000; 0 = disabled) |
| `ProposalCount` | Instance | `u32` | Extended on proposal creation | Total proposals created |
| `Initialized` | Instance | `bool` | Extended on init | Whether contract has been initialized |
| `Proposal(u32)` | Persistent | `Proposal` | Extended on proposal change | Proposal state, vote counts, deadline |
| `VoteKey { proposal_id, voter }` | Persistent | `i128` | Extended on vote | Per-voter vote amount (prevents double-voting) |

**Proposal States:** `Active → Executed` (if quorum + majority reached) or `Active → Cancelled` via `cancel_proposal` (admin, any active proposal) or `proposer_cancel_proposal` (original proposer, only before any vote is cast)

---

## Airdrop Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Admin address |
| `Token` | Instance | `Address` | Extended on init | Payment token address |
| `MerkleRoot` | Instance | `Bytes` | Extended on `set_root` | Merkle root of the distribution tree |
| `ClaimDeadline` | Instance | `u32` | Extended on init | Ledger sequence after which claims are rejected |
| `Claimed(Address)` | Persistent | `bool` | Extended on claim | Whether a given address has already claimed |

Uses per-address persistent storage (`Claimed(Address)`) to prevent duplicate claims, alongside instance storage for the (rarely-changed) root and deadline.

---

## Auction Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Seller` | Instance | `Address` | Extended on init | Seller address |
| `Token` | Instance | `Address` | Extended on init | Bid token address |
| `StartPrice` | Instance | `i128` | Extended on init | Starting bid price |
| `MinIncrement` | Instance | `i128` | Extended on init | Minimum bid increment |
| `Deadline` | Instance | `u32` | Extended on bid (anti-sniping) | Auction end ledger; may be pushed back by a late bid |
| `HighestBidder` | Instance | `Address` | Extended on bid | Current highest bidder |
| `HighestBid` | Instance | `i128` | Extended on bid | Current highest bid amount |
| `Settled` | Instance | `bool` | Extended on `end` | Whether the auction has been settled |
| `ReservePrice` | Instance | `i128` | Extended on init | Optional reserve price |
| `Pending(Address)` | Persistent | `i128` | Extended on bid | Refund owed to an outbid bidder |
| `ExtensionWindow` | Instance | `u32` | Extended on init | Anti-sniping extension window in ledgers (0 = disabled) |
| `Cancelled` | Instance | `bool` | Extended on `cancel` | Whether the seller cancelled before any bid |

Per-address persistent storage (`Pending(Address)`) tracks outbid-bidder refunds independently of the single instance-storage auction state.

---

## Ballot Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `VotingActive` | Instance | `bool` | Extended on `tally`/`tally_all` | Whether voting is still open |
| `RegisteredVoter(Address)` | Persistent | `bool` | Extended on register/vote | Whether an address is a registered voter |
| `Voter(Address)` | Persistent | `bool` | Extended on vote | Whether an address has already voted |
| `YesVotes` | Instance | `i128` | Extended on vote | Backward-compat yes counter (choice index 1) |
| `NoVotes` | Instance | `i128` | Extended on vote | Backward-compat no counter (choice index 0) |
| `VotingStart` | Instance | `u32` | Extended on init | First ledger at which voting opens |
| `VotingEnd` | Instance | `u32` | Extended on init | Last ledger at which voting is open |
| `TotalVotes` | Instance | `i128` | Extended on vote | Running total votes cast; gates `deregister_voter` |
| `Choices` | Instance | `Vec<String>` | Extended on init | Ordered list of choice labels |
| `ChoiceVotes(u32)` | Instance | `i128` | Extended on vote | Per-choice vote tally |

Per-address persistent storage (`RegisteredVoter`, `Voter`) tracks voter eligibility and participation; all tallies live in instance storage since they're bounded by the fixed `choices` list.

---

## Bonding Curve Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Token` | Instance | `Address` | Extended on init | Reserve token address |
| `Reserve` | Instance | `i128` | Extended on buy/sell | Token reserve held by the contract |
| `Supply` | Instance | `i128` | Extended on buy/sell | Current curve-token supply |
| `Price` | Instance | `i128` | Extended on buy/sell | Current price per token (scaled by `PRICE_SCALE = 1_000_000`) |

Instance-only storage — no per-address state — since the curve's price is a pure function of the shared `reserve`/`supply` pair.

---

## Crowdfund Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Creator` | Instance | `Address` | Extended on init | Campaign creator |
| `Token` | Instance | `Address` | Extended on init | Pledge token address |
| `Goal` | Instance | `i128` | Extended on init | Funding goal |
| `Deadline` | Instance | `u32` | Extended on `extend_deadline` | Campaign deadline ledger |
| `TotalPledged` | Instance | `i128` | Extended on pledge/withdraw | Running total pledged |
| `Claimed` | Instance | `bool` | Extended on `claim` | Whether the creator has claimed the funds |
| `Pledge(Address)` | Persistent | `i128` | Extended on pledge | Per-pledger cumulative pledge amount |
| `Tiers` | Instance | `Vec<FundingTier>` | Extended on init | Optional stretch-goal reward tiers |
| `MaxPledgePerAddress` | Instance | `i128` | Extended on init | Optional per-address pledge cap |
| `DeadlineExtended` | Instance | `bool` | Extended on `extend_deadline` | Whether the one-time deadline extension has been used |

Per-address persistent storage (`Pledge(Address)`) tracks individual contributions, gated against `MaxPledgePerAddress` when configured.

---

## Lottery Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Token` | Instance | `Address` | Extended on init | Ticket payment token |
| `TicketPrice` | Instance | `i128` | Extended on init | Price per ticket |
| `Participants` | Instance | `Vec<Address>` | Extended on ticket purchase/refund | List of ticket-holding addresses (one entry per ticket) |
| `State` | Instance | `LotteryState` | Extended on state change | `Open` \| `Committed` \| `Drawn` |
| `Commit` | Instance | `Commit` | Extended on `commit` | Stored `hash(secret \|\| salt)` commitment |
| `Winner` | Instance | `Address` | Extended on `draw` | First (or only) winner address |
| `Winners` | Instance | `Vec<Address>` | Extended on `draw` | All winner addresses |
| `RevealDeadline` | Instance | `u32` | Extended on `commit` | Ledger by which `draw` must be called |
| `WinnerCount` | Instance | `u32` | Extended on init | Number of distinct winners to select |
| `PrizeSplits` | Instance | `Vec<u32>` | Extended on init | Per-winner prize share, in basis points (sums to 10 000) |
| `MaxTicketsPerAddress` | Instance | `u32` | Extended on init | Optional per-address ticket cap |
| `TicketCount(Address)` | Persistent | `u32` | Extended on ticket purchase | Per-address ticket count, gated against the cap |

Uses per-address persistent storage (`TicketCount`) for the ticket cap, while the participant list itself (used for weighted draw and refunds) lives in instance storage.

---

## Marketplace Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Admin address |
| `PaymentToken` | Instance | `Address` | Extended on init | Token used to pay for listings |
| `RoyaltyBps` | Instance | `u32` | Extended on init | Royalty in basis points (e.g. `250` = 2.5%) |
| `RoyaltyRecipient` | Instance | `Address` | Extended on init | Royalty recipient address |
| `NextListingId` | Instance | `u64` | Extended on `list` | Auto-incrementing listing ID counter |
| `Listing(u64)` | Persistent | `Listing` | Extended on list/buy/cancel | Per-listing seller, price, and active state |
| `Offer(u64, Address)` | Persistent | `i128` | Extended on offer activity | Escrowed offer amount for a `(listing_id, buyer)` pair |

Per-listing (`Listing(u64)`) and per-`(listing, buyer)` (`Offer`) persistent keys let each listing/offer expire independently of the shared instance config. See `contract-api.md` and `error-reference.md` for a note on `contracts/marketplace/src/lib.rs` currently containing corrupted/duplicated code — this storage layout reflects the authoritative `storage.rs` definitions.

---

## NFT Contract

### Instance-Storage Keys (`DataKey`)

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Collection administrator |
| `Name` | Instance | `String` | Extended on init | Collection name |
| `Symbol` | Instance | `String` | Extended on init | Collection symbol |
| `TotalSupply` | Instance | `u32` | Extended on mint | Number of tokens minted so far |
| `MaxSupply` | Instance | `u32` | Extended on init | Optional supply cap |
| `Initialized` | Instance | `bool` | Extended on init | Whether the contract has been initialized |
| `RoyaltyBps` | Instance | `u32` | Extended on init | Collection-level default royalty in basis points |
| `RoyaltyRecipient` | Instance | `Address` | Extended on init | Collection-level default royalty recipient |

### Persistent-Storage Keys (`TokenKey`)

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Owner(u32)` | Persistent | `Address` | Extended on mint/transfer | Current owner of a token ID |
| `Approval(u32)` | Persistent | `Address` | Extended on approve | Approved spender for a token ID |
| `Uri(u32)` | Persistent | `String` | Extended on mint | Metadata URI for a token ID |
| `TokenRoyaltyBps(u32)` | Persistent | `u32` | Extended on mint | Optional per-token royalty override (basis points) |
| `TokenRoyaltyRecipient(u32)` | Persistent | `Address` | Extended on mint | Optional per-token royalty recipient override |

Despite the doc comment on `TokenKey` describing it as persistent-tier, per-token owner/URI/royalty data is stored under `env.storage().instance()` in the current `lib.rs` rather than `env.storage().persistent()` — see `error-reference.md`'s NFT section for related `lib.rs`/`errors.rs` naming drift (e.g. `Unauthorized` vs. `NotAuthorized`, `MaxSupplyReached` vs. `SupplyCapReached`) that a future fix should reconcile alongside this.

---

## Oracle Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Admin authorized to push prices |
| `Price` | Instance | `i128` | Extended on `update_price` | Most recently published price |
| `UpdatedAt` | Instance | `u32` | Extended on `update_price` | Ledger sequence of the last price update |
| `StalenessThreshold` | Instance | `u32` | Extended on init | Max age (in ledgers) before `get_price` rejects the price |
| `UpdatedAtTimestamp` | Instance | `u64` | Extended on `update_price` | Unix timestamp of the last price update |
| `Publishers` | Instance | `Vec<Address>` | Extended on `set_publishers` | Admin-configured set of authorized publishers |
| `Submission(Address)` | Persistent | `PublisherSubmission` | Extended on `submit_price` | Latest submission from a given publisher |
| `History` | Instance | `Vec<PriceObservation>` | Extended on price update | Ring buffer of the last `MAX_HISTORY` (30) observations, used for `get_twap` |

Per-publisher persistent storage (`Submission(Address)`) supports the multi-publisher median-price feature independently of the single admin-pushed `Price`.

---

## Subscription Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Provider` | Instance | `Address` | Extended on init | Service provider address |
| `Token` | Instance | `Address` | Extended on init | Payment token address |
| `Subscription(Address)` | Persistent | `SubscriptionInfo` | Extended on subscribe/charge/cancel | Per-subscriber plan, trial, and billing state |
| `Plan(Symbol)` | Persistent | `Plan` | Extended on register/update | Named plan configuration (amount, interval, active) |

Per-subscriber (`Subscription(Address)`) and per-plan (`Plan(Symbol)`) persistent keys let individual subscriptions and plans expire independently of the shared `Provider`/`Token` instance config.

---

## Swap Contract

### Instance-Storage Keys (`DataKey`)

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `SwapCount` | Instance | `u32` | Extended on `propose_swap` | Auto-incrementing swap ID counter |
| `Initialized` | Instance | `bool` | Extended on init | Whether the contract has been initialized |
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Treasury` | Instance | `Address` | Extended on `set_treasury` | Fee-collection treasury address |
| `FeeBps` | Instance | `u32` | Extended on init/`set_fee_bps` | Swap fee in basis points |

### Persistent-Storage Keys (`DataKey`)

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Swap(u32)` | Persistent | `SwapInfo` | Extended on propose/accept/cancel | Per-swap parties, tokens, amounts, expiry, and state |

Per-swap persistent storage (`DataKey::Swap(u32)`) lets each proposed swap expire independently of the shared instance configuration. The contract bumps the record TTL whenever a swap is proposed, read, accepted, or cancelled, while the instance tier contains only configuration and the monotonic counter.

---

## Timelock Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `Token` | Instance | `Address` | Extended on init | Locked token address |
| `Beneficiary` | Instance | `Address` | Extended on init/`reassign_beneficiary` | Recipient of released tokens |
| `ReleaseLedger` | Instance | `u32` | Extended on init | Deprecated: single-tranche release ledger (backward compat) |
| `Amount` | Instance | `i128` | Extended on init | Deprecated: single-tranche amount (backward compat) |
| `State` | Instance | `TimelockState` | Extended on release/cancel | `Active` \| `Released` \| `Cancelled` |
| `Tranches` | Instance | `Vec<ReleaseTranche>` | Extended on `initialize_with_tranches` | Multi-tranche release schedule |
| `ReleasedTranches` | Instance | `Vec<bool>` | Extended on `release` | Per-tranche release flags, indexed by tranche position |

Instance-only storage — a single timelock instance has one admin/beneficiary/schedule, so there's no per-address fan-out the way there is in per-user contracts like Token or Staking.

---

## Wrapped-Token Contract

### Storage Keys

| Key | Tier | Type | TTL Policy | Description |
|-----|------|------|------------|-------------|
| `Admin` | Instance | `Address` | Extended on init | Contract administrator |
| `WrappedToken` | Instance | `Address` | Extended on init | Wrapped (minted) token contract address |
| `UnderlyingToken` | Instance | `Address` | Extended on init | Underlying (reserve) token contract address |
| `TotalWrapped` | Instance | `i128` | Extended on wrap/unwrap | Contract-tracked total wrapped supply |
| `Paused` | Instance | `bool` | Extended on pause/unpause | Whether the contract is paused (`pausable` feature only) |
| `MaxWrapPerAddress` | Instance | `i128` | Extended on init | Optional cap on cumulative wrap per address |
| `WrappedByAddress(Address)` | Persistent | `i128` | Extended on wrap | Cumulative amount wrapped so far by an address (only tracked when a cap is set) |

**Storage-key stability:** the `DataKey` enum's variant order is covered by an exhaustive discriminant test (`discriminant_tests::data_key_discriminants_are_stable`) — new keys must be appended at the end, never inserted or reordered, since Soroban's `#[contracttype]` encoding uses the variant name/position as the on-chain storage discriminant.

---

## TTL Extension Strategy

### Instance vs. Persistent

- **Instance storage** is extended once per transaction involving the contract. All instance keys share a single TTL bump.
- **Persistent storage** is extended individually per key on each read or write operation.

### Ledger Sequence and Renewal

On every operation that reads or writes a key:

1. The current ledger sequence is obtained via `env.ledger().sequence()`.
2. If TTL is below a threshold (typically 50 days), the entry is extended to `current_ledger + EXTENSION_LIFETIME`.
3. EXTENSION_LIFETIME is typically 31,536,000 ledgers (~1 year).

### Example: Token Transfer

```
User calls transfer(to, amount)
├─ Read Balance(from) → TTL bumped
├─ Write Balance(from) → TTL bumped
├─ Read Balance(to) → TTL bumped
├─ Write Balance(to) → TTL bumped
└─ Emit transferred event
```

### Known Gap: Vesting Read-Side TTL ([#969](https://github.com/Fidelis900/soroban-starter-kit/issues/969))

`vesting`'s per-beneficiary `Schedule(beneficiary)` persistent entry is bumped
via `bump_schedule()` (`extend_ttl_persistent`) after every write —
`create_schedule`, `claim`, `revoke`, and `admin_release` all call it, so the
write-side gap this issue originally reported is fixed. What's still open:
`get_info` only calls `bump()` (instance TTL, not the `Schedule` entry
itself) and `claimable` doesn't extend any TTL at all. A beneficiary who is
only ever read (via `get_info`/`claimable`) and never writes — no `claim`,
no `revoke`, no `admin_release` — can still have their `Schedule` entry
archived out from under them between long-cliff writes. The fix is to have
both read paths call `bump_schedule()` too, matching the pattern
`multisig::get_transaction` already uses to keep actively-read entries alive
between writes.

---

## See Also

- [Architecture Decision Record 0001: Storage Tier Choices](adr/0001-storage-tier-choices.md) — rationale for instance vs. persistent tier selection
- [Integration Guide](integration-guide.md) — how to interpret storage in your integration
- [Soroban Storage Docs](https://soroban.stellar.org/docs/learn/storing-data)
