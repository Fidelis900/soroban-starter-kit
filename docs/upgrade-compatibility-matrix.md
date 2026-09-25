# Upgrade Compatibility Matrix

Companion to the [Upgrade Guide](upgrade-guide.md): what storage each
contract actually persists, which contracts share a storage layout or data
structure with another contract (so a layout change in one must be applied
consistently to the other), and what kinds of code changes are
storage-breaking for a live deployment of each contract.

## How storage compatibility works here

- **`#[contracttype]` enums used as storage keys encode by variant *name*,
  not declaration order.** Every `storage.rs` in this repo documents this
  rule directly above its `DataKey` enum. Renaming or removing a variant
  changes the on-chain key and orphans existing data; adding a variant is
  safe as long as it's appended and no existing variant is renamed.
- **Struct shape matters.** For `#[contracttype]` structs actually written
  to storage (as opposed to response DTOs assembled at query time, see
  below), adding a field, removing a field, or changing a field's type is
  storage-breaking. Reordering `#[contracttype]` struct fields is also
  unsafe if the struct derives `Ord`/`PartialOrd`, since comparisons then
  depend on declaration order.
- **Not everything with a `#[contracttype]` on it is stored.** Most
  contracts expose a `get_info` / `get_*_data` query that assembles a
  response struct (`EscrowInfo`, `AuctionInfo`, `CrowdfundInfo`,
  `LotteryInfo`, `VestingInfo`, `PriceData`, `ListingEntry`, `ListingPage`,
  ...) from several individually-stored keys. These response DTOs can be
  changed freely between upgrades — they're never written to storage, only
  constructed on read. The matrix below lists only the types that are
  actually persisted.
- **Storage tier matters for TTL, not shape.** Moving a key between
  instance, persistent, and temporary storage doesn't change its XDR
  encoding, but changes its bump/eviction lifecycle — treat it as an
  operational change to test, even though it isn't "storage-breaking" in
  the layout sense.

## Contracts sharing a storage layout or data structure

| Pattern | Contracts | Why it matters |
|---|---|---|
| Admin address key | All 19 deployable contracts store a single admin `Address`. **`escrow`** is the only one that stores it under `soroban_common::AdminKey::Admin` (a shared type from the `common` crate) instead of a local `DataKey::Admin` variant — every other contract defines its own independent `DataKey::Admin`. A rename of `AdminKey::Admin` in `contracts/common` is storage-breaking for `escrow` specifically; it has no effect on the other 18, since their `Admin` variant lives in their own enum. |
| Upgrade timelock (`PendingUpgrade` + `Version`) | `escrow`, `token` | Both store the identical shape `PendingUpgrade: (BytesN<32>, u32)` = `(wasm_hash, ready_after_ledger)` plus a `Version: u32` counter, backing `propose_upgrade`/`execute_upgrade`. If this pattern is extended (e.g. to add a proposer address to the tuple), change both contracts together and update [`upgrade-guide.md`](upgrade-guide.md) — currently the only two contracts with on-chain upgrade timelocks. |
| Address-keyed persistent record | `airdrop` (`Claimed(Address)`), `auction` (`Pending(Address)`), `ballot` (`RegisteredVoter`/`Voter(Address)`), `crowdfund` (`Pledge(Address)`), `lottery` (`TicketCount(Address)`), `oracle` (`Submission(Address)`), `staking` (`Stake`/`RewardPerTokenPaid`/`Rewards`/`Compounding(Address)`), `subscription` (`Subscription(Address)`), `token` (`Balance`/`Frozen(Address)`) | Same *pattern* (an `Address` as part of the storage key) reused independently by each contract. Each contract's value shape is unrelated to the others' — this row is about template consistency, not a shared type. Changing one contract's per-address value shape doesn't affect any other contract. |
| Contract version counter | `escrow`, `multisig`, `staking`, `token`, `vesting` | All expose `contract_version()` backed by a `Version: u32` instance key. Only `escrow` and `token` pair it with `PendingUpgrade` (see above); in `multisig`/`staking`/`vesting` it is a plain counter with no upgrade-timelock storage attached. |

## Per-contract storage matrix

For each contract: the `DataKey` variants it defines (dynamic/persistent
keys marked), any `#[contracttype]` structs actually written to storage,
and change categories that would break storage for that contract
specifically, beyond the general rules above.

| Contract | Storage keys (`DataKey`) | Stored structs | Contract-specific storage-breaking risks |
|---|---|---|---|
| `airdrop` | `Admin`, `Token`, `MerkleRoot`, `ClaimDeadline`; dynamic: `Claimed(Address)` | — | None beyond the general rules. |
| `auction` | `Seller`, `Token`, `StartPrice`, `MinIncrement`, `Deadline`, `HighestBidder`, `HighestBid`, `Settled`, `ReservePrice`, `ExtensionWindow`, `Cancelled`, `StartLedger`, `CancellationGraceLedgers`, `CancellationFee`; dynamic: `Pending(Address)` | — | None beyond the general rules. `AuctionInfo` is a query-only DTO, not stored. |
| `ballot` | `Admin`, `VotingActive`, `YesVotes`, `NoVotes`, `VotingStart`, `VotingEnd`, `TotalVotes`, `Choices`; dynamic: `RegisteredVoter(Address)`, `Voter(Address)`, `ChoiceVotes(u32)` | — | `Choices` is a stored `Vec` of choice labels, indexed positionally; `ChoiceVotes(u32)` keys reference an entry by that index. Reordering or removing entries from `Choices` after voting has started silently remaps which tally belongs to which choice label — treat it as append-only, like a `DataKey` enum. |
| `bonding-curve` | `Admin`, `Token`, `Reserve`, `Supply`, `Price` | — | None beyond the general rules. |
| `crowdfund` | `Creator`, `Token`, `Goal`, `Deadline`, `TotalPledged`, `Claimed`, `Tiers`, `MaxPledgePerAddress`, `DeadlineExtended`; dynamic: `Pledge(Address)` | `FundingTier` (stored inside the `Tiers: Vec<FundingTier>` list) | `FundingTier`'s field shape is storage-breaking since it's nested inside a stored `Vec`. `TierStatus` is a query-only DTO, not stored. |
| `dao` | `Admin`, `Token`, `VotingPeriod`, `Quorum`, `QuorumBps`, `ProposalCount`, `Initialized` | `Proposal` (per-proposal, keyed by a separate `ProposalKey`), `VoteKey` | `Proposal`'s field shape and `ProposalState`'s variant names are storage-breaking. |
| `escrow` | `Buyer`, `Seller`, `Arbiter`, `TokenContract`, `Amount`, `Deadline`, `State`, `Paused`, `Version`, `PendingUpgrade`, `Arbiters`, `RequiredSignatures`, `ArbiterVotesRelease`, `ArbiterVotesRefund`, `MetadataHash`, `DisputeTimeoutLedgers`, `DisputeRaisedAt`, `Milestones`, `FeeBps`, `Treasury`; admin via shared `soroban_common::AdminKey` (see table above) | `EscrowState` (stored under `State`), `Milestone` (stored inside the `Milestones: Vec<Milestone>` list) | `EscrowState` derives `Ord`/`PartialOrd` and lifecycle logic compares states with `<`/`>=`; reordering its variants changes comparison results even though the XDR key name doesn't change. Keep new states appended in lifecycle order, not just enum-declaration order. `Milestone`'s field shape is storage-breaking since it's nested inside a stored `Vec`; it's only populated for escrows initialized with milestones. Dispute resolution is split into two separate vote keys (`ArbiterVotesRelease`/`ArbiterVotesRefund`), not the single `ArbiterVotes` key this doc previously described. |
| `lottery` | `Admin`, `Token`, `TicketPrice`, `Participants`, `State`, `Commit`, `Winner`, `Winners`, `RevealDeadline`, `WinnerCount`, `PrizeSplits`, `MaxTicketsPerAddress`; dynamic: `TicketCount(Address)` | `Commit` (stored under `DataKey::Commit`) | None beyond the general rules. |
| `marketplace` | `Admin`, `PaymentToken`, `RoyaltyBps`, `RoyaltyRecipient`, `NextListingId`; dynamic: `Listing(u64)`, `Offer(u64, Address)` | `Listing` (stored per `Listing(u64)` key) | `Listing`'s field shape is storage-breaking; `ListingEntry`/`ListingPage` are pagination DTOs assembled at query time and not stored. |
| `multisig` | `Signers`, `Weights`, `Threshold`, `NextTransactionId`, `Paused`, `Version`; dynamic: `Transaction(u64)` | `Transaction` (stored per `Transaction(u64)` key), `Weights` (stored `Map<Address, u32>` of per-signer vote weights, added for weighted-voting support, #825) | `Transaction`'s field shape (including the `Vec<Val>` args it carries) is storage-breaking. `Weights`' shape is storage-breaking the same way the other stored maps/structs in this table are flagged: changing what it maps from/to, or how absence is interpreted (currently: a signer absent from the map defaults to weight 1), changes vote-weight calculations for every existing wallet. |
| `nft` | `Admin`, `Name`, `Symbol`, `TotalSupply`, `MaxSupply`, `Initialized`, `RoyaltyBps`, `RoyaltyRecipient`; dynamic: per-token owner/approval/metadata/royalty-override keys (see `TokenKey`) | `TokenMetadata` | `TokenMetadata`'s field shape is storage-breaking. Collection-level `RoyaltyBps`/`RoyaltyRecipient` are separate instance keys from the per-token `TokenRoyaltyBps(u32)`/`TokenRoyaltyRecipient(u32)` overrides in `TokenKey`. |
| `oracle` | `Admin`, `Price`, `UpdatedAt`, `StalenessThreshold`, `UpdatedAtTimestamp`, `Publishers`, `History`; dynamic: `Submission(Address)` | `PublisherSubmission` (stored per `Submission(Address)`), `PriceObservation` (stored inside the `History` ring buffer) | `PriceObservation`'s shape is storage-breaking since it's nested in the stored `History` vector. `PriceData` is a query-only DTO, not stored. |
| `staking` | `Admin`, `StakeToken`, `RewardToken`, `TotalStaked`, `TotalRewards`, `RewardPerTokenStored`, `Version`, `UnbondingPeriod`, `SlashDestination`; dynamic: `Stake(Address)`, `RewardPerTokenPaid(Address)`, `Rewards(Address)`, `Compounding(Address)`, `UnbondRequest(Address)` | `UnbondRequest` (stored per `UnbondRequest(Address)` key) | The `REWARD_SCALE` fixed-point scale used to interpret `RewardPerTokenStored`/`RewardPerTokenPaid` is a code constant, not stored — changing it changes how existing stored values are interpreted without any storage layout change, so treat it as storage-breaking in effect even though no key/type changes. `UnbondRequest`'s field shape is storage-breaking the same way other stored structs in this table are flagged. It also holds at most one pending request per staker under a single `UnbondRequest(Address)` key — a second `unstake()` call before `withdraw()` overwrites the existing entry rather than queuing a second one, so this is a behavioral constraint as well as a layout one. |
| `subscription` | `Provider`, `Token`; dynamic: `Subscription(Address)`, `Plan(Symbol)` | `SubscriptionInfo` (stored per `Subscription(Address)`), `Plan` (stored per `Plan(Symbol)`) | `SubscriptionInfo`'s and `Plan`'s field shapes are storage-breaking. |
| `swap` | `SwapCount`, `Initialized`, `Admin`, `Treasury`, `FeeBps`; dynamic (via `SwapKey`, see storage.rs) | `SwapInfo` | `SwapInfo`'s field shape and `SwapState`'s variant names are storage-breaking. |
| `timelock` | `Admin`, `Token`, `Beneficiary`, `ReleaseLedger`, `Amount`, `State`, `Tranches`, `ReleasedTranches` | `TimelockState` (stored under `State`), `ReleaseTranche` (stored inside the `Tranches: Vec<ReleaseTranche>` list) | `TimelockState`'s variant names are storage-breaking. `ReleaseTranche`'s field shape is storage-breaking since it's nested inside a stored `Vec`. `ReleaseLedger`/`Amount` are deprecated single-tranche fields kept only for backward compatibility with schedules created before multi-tranche support existed; `TimelockInfo` is a query-only DTO, not stored. |
| `token` | `Admin`, `PendingAdmin`, `TotalSupply`, `Paused`, `MaxSupply`, `PendingUpgrade`, `Version`, `TransferHook`; dynamic: `Balance(Address)`, `Allowance(AllowanceDataKey)`, `Metadata(MetadataKey)`, `Frozen(Address)`, `Snapshot(Address, u32)`, `PermitSigner(Address)`, `PermitNonce(Address)` | `AllowanceDataKey`/`AllowanceValue`, snapshot balances | `AllowanceDataKey`/`AllowanceValue`'s field shape is storage-breaking; it is also the type most likely to be read by external integrators (wallets, DEXs), so a shape change needs coordinated off-chain updates, not just an on-chain migration. Shares the `PendingUpgrade`/`Version` pattern with `escrow` (see table above). `PermitSigner(Address)`/`PermitNonce(Address)` back signature-based permits; the registered ed25519 key and nonce are per-owner state and must stay in sync — a migration that drops or resets `PermitNonce` without also handling `PermitSigner` re-enables replay of old signed permits. A migrated `PermitNonce(Address)` value must never decrease from its pre-migration value; lowering (or clearing) it re-validates previously-consumed signed permits, allowing them to be replayed. |
| `vesting` | `Admin`, `Token`, `AdminReleased`, `Version`; dynamic: `Schedule(Address)` | `BeneficiarySchedule` (stored per `Schedule(Address)` key) | `BeneficiarySchedule`'s field shape is storage-breaking since it's nested behind a per-beneficiary key. This contract stores one schedule per beneficiary under `Schedule(Address)` rather than a single global schedule — do not reintroduce flat, single-beneficiary keys (`Beneficiary`, `CliffLedger`, `EndLedger`, `Amount`, `Claimed`, `Revoked`) alongside this per-beneficiary layout. |
| `wrapped-token` | `Admin`, `WrappedToken`, `UnderlyingToken`, `TotalWrapped`, `Paused`, `MaxWrapPerAddress`; dynamic: `WrappedByAddress(Address)` | — | None beyond the general rules. |

`contracts/common` has no storage of its own beyond the `AdminKey::Admin`
type it defines for `escrow` to reuse (see the shared-pattern table above);
it is not deployed independently.
