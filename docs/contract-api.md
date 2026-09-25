# Contract API Reference

Complete public API documentation for all Soroban starter kit contracts.

## Token Contract

**Location:** `contracts/token/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, name: String, symbol: String, decimals: u32, max_supply: Option<i128>` | `Result<(), TokenError>` | `AlreadyInitialized`, `InvalidAmount` |
| `mint` | `env: Env, to: Address, amount: i128` | `Result<(), TokenError>` | `Unauthorized`, `Overflow`, `InvalidAmount` |
| `burn` | `env: Env, from: Address, amount: i128` | `Result<(), TokenError>` | `InsufficientBalance`, `Unauthorized`, `InvalidAmount` |
| `transfer` | `env: Env, from: Address, to: Address, amount: i128` | `Result<(), TokenError>` | `InsufficientBalance`, `InvalidAmount` |
| `transfer_from` | `env: Env, spender: Address, from: Address, to: Address, amount: i128` | `Result<(), TokenError>` | `InsufficientAllowance`, `InsufficientBalance`, `InvalidAmount` |
| `approve` | `env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32` | `Result<(), TokenError>` | `InvalidAmount` |
| `allowance` | `env: Env, from: Address, spender: Address` | `i128` | None |
| `balance` | `env: Env, id: Address` | `i128` | None |
| `total_supply` | `env: Env` | `i128` | None |
| `name` | `env: Env` | `String` | None |
| `symbol` | `env: Env` | `String` | None |
| `decimals` | `env: Env` | `u32` | None |

**Errors:**
- `InsufficientBalance` (1) — Caller's balance too low
- `InsufficientAllowance` (2) — Allowance too low for transfer_from
- `Unauthorized` (3) — Caller not admin
- `AlreadyInitialized` (4) — initialize called twice
- `NotInitialized` (5) — Operation before initialize
- `InvalidAmount` (6) — Amount zero, negative, or exceeds cap
- `Overflow` (7) — Arithmetic overflow

---

## Escrow Contract

**Location:** `contracts/escrow/src/lib.rs`

### Core Operations

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, buyer: Address, seller: Address, arbiter: Address, token_contract: Address, amount: i128, deadline_ledger: u32` | `Result<(), EscrowError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidParties` |
| `initialize_with_arbiters` | `env: Env, buyer: Address, seller: Address, arbiters: Vec<Address>, token_contract: Address, amount: i128, deadline_ledger: u32, required_signatures: u32` | `Result<(), EscrowError>` | Same + validation |
| `fund` | `env: Env` | `Result<(), EscrowError>` | `InvalidState`, `InsufficientFunds` |
| `mark_delivered` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `approve_delivery` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `release_partial` | `env: Env, amount: i128` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState`, `InvalidAmount` |
| `request_refund` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `raise_dispute` | `env: Env, caller: Address` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `resolve_dispute` | `env: Env, release_to_seller: bool` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `cancel` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `extend_deadline` | `env: Env, new_deadline: u32` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |

### Query Functions

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `get_escrow_info` | `env: Env` | `Result<EscrowInfo, EscrowError>` | `NotInitialized` |
| `get_state` | `env: Env` | `Option<EscrowState>` | None |
| `is_deadline_passed` | `env: Env` | `bool` | None |
| `get_remaining_ledgers` | `env: Env` | `i64` | None |

**Errors:**
- `NotAuthorized` (1) — Caller not permitted
- `InvalidState` (2) — Escrow not in required state
- `DeadlinePassed` (3) — Deadline already elapsed
- `DeadlineNotReached` (4) — Deadline not yet passed
- `AlreadyInitialized` (5) — initialize called twice
- `NotInitialized` (6) — Operation before initialize
- `InsufficientFunds` (7) — Buyer balance too low
- `InvalidAmount` (8) — Amount zero or invalid
- `InvalidParties` (9) — Invalid addresses or conflicts

---

## Staking Contract

**Location:** `contracts/staking/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, stake_token: Address, reward_token: Address` | `Result<(), StakingError>` | `AlreadyInitialized` |
| `stake` | `env: Env, staker: Address, amount: i128` | `Result<(), StakingError>` | `NotInitialized`, `InvalidAmount` |
| `unstake` | `env: Env, staker: Address, amount: i128` | `Result<(), StakingError>` | `NotInitialized`, `InvalidAmount`, `InsufficientStake`, `NoStake` |
| `add_rewards` | `env: Env, amount: i128` | `Result<(), StakingError>` | `Unauthorized`, `NotInitialized`, `InvalidAmount` |
| `claim_rewards` | `env: Env, staker: Address` | `Result<(), StakingError>` | `NotInitialized`, `NoRewards` |
| `total_staked` | `env: Env` | `i128` | None |
| `total_rewards` | `env: Env` | `i128` | None |
| `user_stake` | `env: Env, staker: Address` | `i128` | None |
| `user_rewards` | `env: Env, staker: Address` | `i128` | None |

**Errors:**
- `AlreadyInitialized` (1) — initialize called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `InvalidAmount` (4) — Amount zero or negative
- `NoStake` (5) — No stake to unstake/claim
- `InsufficientStake` (6) — Unstake amount exceeds stake
- `NoRewards` (7) — No rewards available

---

## Vesting Contract

**Location:** `contracts/vesting/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, beneficiary: Address, token: Address, amount: i128, cliff_ledger: u32, end_ledger: u32` | `Result<(), VestingError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidSchedule` |
| `claim` | `env: Env` | `Result<(), VestingError>` | `NotInitialized`, `NothingToClaim` |
| `revoke` | `env: Env` | `Result<(), VestingError>` | `NotInitialized`, `Unauthorized`, `AlreadyRevoked` |
| `get_vested_amount` | `env: Env` | `i128` | None |
| `get_claimed_amount` | `env: Env` | `i128` | None |
| `get_unvested_amount` | `env: Env` | `i128` | None |
| `is_revoked` | `env: Env` | `bool` | None |

**Errors:**
- `AlreadyInitialized` (1) — initialize called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `InvalidAmount` (4) — Amount zero or negative
- `InvalidSchedule` (5) — cliff_ledger >= end_ledger or end_ledger in past
- `NothingToClaim` (6) — No tokens vested since last claim
- `AlreadyRevoked` (7) — revoke called on revoked schedule

---

## Multisig Contract

**Location:** `contracts/multisig/src/lib.rs`

### Core Operations

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, signers: Vec<Address>, threshold: u32` | `Result<(), MultisigError>` | `AlreadyInitialized`, `InvalidThreshold`, `InvalidSigners` |
| `add_signer` | `env: Env, approvals: Vec<Address>, signer: Address, new_threshold: u32` | `Result<(), MultisigError>` | `NotInitialized`, `NotSigner`, `InsufficientApprovals`, `InvalidThreshold` |
| `remove_signer` | `env: Env, approvals: Vec<Address>, signer: Address, new_threshold: u32` | `Result<(), MultisigError>` | `NotInitialized`, `NotSigner`, `InsufficientApprovals`, `InvalidThreshold` |
| `propose` | `env: Env, target: Address, func: Symbol, args: Vec<Val>` | `Result<u64, MultisigError>` | `NotInitialized` |
| `approve` | `env: Env, tx_id: u64` | `Result<(), MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `AlreadySigned`, `NotSigner` |
| `execute` | `env: Env, tx_id: u64` | `Result<Val, MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `ThresholdNotMet` |
| `get_signers` | `env: Env` | `Vec<Address>` | None |
| `get_threshold` | `env: Env` | `u32` | None |

**Errors:**
- `AlreadyInitialized` (1) — initialize called twice
- `NotInitialized` (2) — Operation before initialize
- `InvalidThreshold` (3) — Threshold zero or > signer count
- `InvalidSigners` (4) — Signers empty, duplicate, or invalid
- `NotSigner` (5) — Caller/approver not in signer set
- `TransactionNotFound` (6) — TX ID does not exist
- `AlreadyExecuted` (7) — Transaction already executed
- `AlreadySigned` (8) — Signer already approved
- `ThresholdNotMet` (9) — Not enough signatures
- `InsufficientApprovals` (10) — Signer change lacks threshold approvals

---

## Airdrop Contract

**Location:** `contracts/airdrop/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address, claim_deadline: u32` | `Result<(), AirdropError>` | `AlreadyInitialized` |
| `set_root` | `env: Env, root: BytesN<32>` | `Result<(), AirdropError>` | `NotInitialized`, `Unauthorized` |
| `claim` | `env: Env, recipient: Address, amount: i128, proof: Vec<BytesN<32>>` | `Result<(), AirdropError>` | `NotInitialized`, `RootNotSet`, `InvalidAmount`, `ClaimWindowClosed`, `AlreadyClaimed`, `InvalidProof` |
| `claim_batch` | `env: Env, entries: Vec<(Address, i128, Vec<BytesN<32>>)>` | `Result<(), AirdropError>` | `NotInitialized`, `RootNotSet`, `ClaimWindowClosed`, `InvalidAmount`, `AlreadyClaimed`, `InvalidProof` |
| `is_claimed` | `env: Env, address: Address` | `bool` | None |
| `get_root` | `env: Env` | `Option<Bytes>` | None |

`claim_batch` uses all-or-nothing semantics: every entry is validated before any transfer executes, so a single bad entry aborts the whole batch.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `RootNotSet` (4) — No merkle root configured yet
- `InvalidProof` (5) — Merkle proof does not verify
- `AlreadyClaimed` (6) — Address already claimed
- `InvalidAmount` (7) — Claim amount <= 0
- `ClaimWindowClosed` (8) — Current ledger past `claim_deadline`

---

## Auction Contract

**Location:** `contracts/auction/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `start` | `env: Env, seller: Address, token: Address, start_price: i128, min_increment: i128, deadline: u32, reserve_price: Option<i128>, extension_window: u32` | `Result<(), AuctionError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidDeadline` |
| `bid` | `env: Env, bidder: Address, amount: i128` | `Result<(), AuctionError>` | `InvalidAmount`, `AuctionEnded`, `NotInitialized`, `BidTooLow` |
| `cancel` | `env: Env, seller: Address` | `Result<(), AuctionError>` | `NotInitialized`, `NotAuthorized`, `AlreadyEnded`, `BidAlreadyPlaced` |
| `end` | `env: Env` | `Result<(), AuctionError>` | `NotInitialized`, `AuctionNotEnded`, `AlreadyEnded` |
| `withdraw` | `env: Env, bidder: Address` | `Result<(), AuctionError>` | `NothingToWithdraw` |
| `get_pending` | `env: Env, bidder: Address` | `i128` | None |
| `get_info` | `env: Env` | `Result<AuctionInfo, AuctionError>` | `NotInitialized` |

`bid` extends `deadline` by `extension_window` ledgers when a bid lands within that window of the current deadline (anti-sniping). `end` settles to the seller when `highest_bid >= reserve_price` (or no reserve is set); otherwise it refunds the highest bidder and the item goes unsold. `cancel` only succeeds before the first bid is placed.

**Errors:**
- `AlreadyInitialized` (1) — `start` called twice
- `NotInitialized` (2) — Operation before `start`
- `AuctionEnded` (3) — Deadline passed or auction cancelled
- `AuctionNotEnded` (4) — `end` called before the deadline
- `BidTooLow` (5) — Bid below the required minimum
- `AlreadyEnded` (6) — Already settled
- `NoBids` (7) — Reserved; `end()` handles the no-bids case via an event, not this error
- `NotAuthorized` (8) — Caller is not the seller
- `InvalidAmount` (9) — `start_price`/`min_increment`/bid <= 0
- `InvalidDeadline` (10) — `deadline` not in the future
- `NothingToWithdraw` (11) — No pending refund
- `ReserveNotMet` (12) — Reserved; `end()` handles this case via an event, not this error
- `BidAlreadyPlaced` (13) — `cancel` called after a bid was placed

---

## Ballot Contract

**Location:** `contracts/ballot/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, voting_start: u32, voting_end: u32, choices: Vec<String>` | `Result<(), BallotError>` | `AlreadyInitialized`, `NoChoices`, `InvalidWindow` |
| `register_voter` | `env: Env, voter: Address` | `Result<(), BallotError>` | `NotInitialized`, `Unauthorized` |
| `deregister_voter` | `env: Env, voter: Address` | `Result<(), BallotError>` | `NotInitialized`, `Unauthorized`, `VotingAlreadyStarted`, `NotRegistered` |
| `vote` | `env: Env, voter: Address, choice: u32` | `Result<(), BallotError>` | `NotInitialized`, `VotingNotStarted`, `VotingClosed`, `NotRegistered`, `AlreadyVoted`, `InvalidChoice` |
| `tally_all` | `env: Env` | `Result<Vec<i128>, BallotError>` | `NotInitialized`, `Unauthorized` |
| `tally` | `env: Env` | `Result<(i128, i128), BallotError>` | `NotInitialized`, `Unauthorized` |
| `get_yes_votes` | `env: Env` | `i128` | None |
| `get_no_votes` | `env: Env` | `i128` | None |
| `get_choice_votes` | `env: Env, choice_index: u32` | `i128` | None |
| `get_choices` | `env: Env` | `Vec<String>` | None |

`choice` is a 0-based index into the `choices` list from `initialize`; for two-choice ballots the convention is `0 = no`, `1 = yes`, kept in sync via `get_yes_votes`/`get_no_votes`. `tally_all` (multi-choice) and `tally` (legacy two-choice) both close voting when called.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `NotRegistered` (4) — Voter not registered
- `AlreadyVoted` (5) — Voter already voted
- `InvalidChoice` (6) — Choice index out of range
- `VotingClosed` (7) — Voting inactive or past `voting_end`
- `VotingAlreadyStarted` (8) — `deregister_voter` called after votes were cast
- `InvalidWindow` (9) — Invalid `voting_start`/`voting_end`
- `VotingNotStarted` (10) — Before `voting_start`
- `NoChoices` (11) — Empty `choices` list

---

## Bonding Curve Contract

**Location:** `contracts/bonding-curve/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address` | `Result<(), BondingCurveError>` | `AlreadyInitialized` |
| `buy` | `env: Env, buyer: Address, amount: i128, max_cost: i128` | `Result<(), BondingCurveError>` | `NotInitialized`, `InvalidAmount`, `Overflow` |
| `sell` | `env: Env, seller: Address, amount: i128, min_proceeds: i128` | `Result<(), BondingCurveError>` | `NotInitialized`, `InvalidAmount`, `InsufficientReserve`, `Overflow` |
| `get_reserve` | `env: Env` | `i128` | None |
| `get_supply` | `env: Env` | `i128` | None |
| `get_price` | `env: Env` | `i128` | None |

Linear curve: `price = reserve / (supply + 1)` (scaled by `PRICE_SCALE`). `buy` fails with `InvalidAmount` if the computed cost exceeds `max_cost`; `sell` fails the same way if proceeds fall below `min_proceeds`.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Reserved; not returned by any current entry point
- `InvalidAmount` (4) — Amount <= 0, or a slippage bound was violated
- `InsufficientReserve` (5) — Reserve too low to pay out a sell
- `Overflow` (6) — Arithmetic overflow in price/cost/proceeds calculation

---

## Crowdfund Contract

**Location:** `contracts/crowdfund/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, creator: Address, token: Address, goal: i128, deadline: u32, tiers: Vec<FundingTier>, max_pledge_per_address: Option<i128>` | `Result<(), CrowdfundError>` | `AlreadyInitialized`, `InvalidGoal`, `InvalidDeadline`, `InvalidTier`, `InvalidAmount` |
| `pledge` | `env: Env, pledger: Address, amount: i128` | `Result<(), CrowdfundError>` | `NotInitialized`, `DeadlinePassed`, `InvalidAmount`, `PledgeCapExceeded` |
| `withdraw` | `env: Env, pledger: Address` | `Result<(), CrowdfundError>` | `NotInitialized`, `DeadlinePassed`, `NothingToWithdraw` |
| `extend_deadline` | `env: Env, new_deadline: u32` | `Result<(), CrowdfundError>` | `NotInitialized`, `DeadlinePassed`, `DeadlineAlreadyExtended`, `InvalidDeadline` |
| `claim` | `env: Env` | `Result<(), CrowdfundError>` | `NotInitialized`, `NotAuthorized`, `DeadlineNotReached`, `GoalNotMet`, `AlreadyClaimed` |
| `refund` | `env: Env, pledger: Address` | `Result<(), CrowdfundError>` | `NotInitialized`, `DeadlineNotReached`, `GoalAlreadyMet`, `NothingToWithdraw` |
| `get_info` | `env: Env` | `Result<CrowdfundInfo, CrowdfundError>` | `NotInitialized` |
| `get_pledge` | `env: Env, pledger: Address` | `i128` | None |

`get_info` reports each configured `FundingTier` alongside whether `total_pledged` has crossed its threshold. `extend_deadline` may only be used once per campaign.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `DeadlinePassed` (3) — Deadline has passed
- `DeadlineNotReached` (4) — Deadline not yet reached
- `GoalAlreadyMet` (5) — `refund` called on a successful campaign
- `GoalNotMet` (6) — `claim` called on an unsuccessful campaign
- `AlreadyClaimed` (7) — Funds already claimed
- `NothingToPledge` (8) — Reserved; `pledge` reports a bad amount via `InvalidAmount` instead
- `NothingToWithdraw` (9) — No active pledge to withdraw/refund
- `InvalidAmount` (10) — Pledge amount or cap <= 0
- `InvalidDeadline` (11) — Deadline not in the future / not a valid extension
- `InvalidGoal` (12) — Goal <= 0
- `NotAuthorized` (13) — Caller is not the creator
- `InvalidTier` (14) — A tier threshold <= 0
- `PledgeCapExceeded` (15) — Pledge would exceed `max_pledge_per_address`
- `DeadlineAlreadyExtended` (16) — Deadline already extended once

---

## DAO Contract

**Location:** `contracts/dao/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address, voting_period: u32, quorum: i128, quorum_bps: u32` | `Result<(), DaoError>` | `AlreadyInitialized`, `InvalidQuorumBps` |
| `create_proposal` | `env: Env, proposer: Address, title: String, description: String` | `Result<u32, DaoError>` | `NotInitialized`, `InsufficientVotingPower` |
| `vote` | `env: Env, voter: Address, proposal_id: u32, support: bool` | `Result<(), DaoError>` | `NotInitialized`, `ProposalNotFound`, `InvalidState`, `VotingClosed`, `AlreadyVoted`, `InsufficientVotingPower` |
| `execute_proposal` | `env: Env, proposal_id: u32` | `Result<(), DaoError>` | `NotInitialized`, `ProposalNotFound`, `InvalidState`, `DeadlineNotReached`, `QuorumNotMet`, `ProposalRejected` |
| `cancel_proposal` | `env: Env, proposal_id: u32` | `Result<(), DaoError>` | `NotInitialized`, `NotAuthorized`, `ProposalNotFound`, `InvalidState` |
| `proposer_cancel_proposal` | `env: Env, proposer: Address, proposal_id: u32` | `Result<(), DaoError>` | `NotInitialized`, `ProposalNotFound`, `NotAuthorized`, `InvalidState`, `VotesAlreadyCast` |
| `get_proposal` | `env: Env, proposal_id: u32` | `Result<Proposal, DaoError>` | `ProposalNotFound` |
| `proposal_count` | `env: Env` | `u32` | None |

Voting weight is the voter's token balance at vote time, capped at the total supply snapshot taken at proposal creation (flash-loan resistant). `execute_proposal` requires both the absolute `quorum` and, if `quorum_bps > 0`, that participation reach `quorum_bps` of the snapshotted total supply, plus `yes_votes > no_votes`.

**Errors:**
- `NotAuthorized` (1) — Caller not admin/original proposer
- `AlreadyInitialized` (2) — `initialize` called twice
- `NotInitialized` (3) — Operation before initialize
- `ProposalNotFound` (4) — Unknown `proposal_id`
- `InvalidState` (5) — Proposal not `Active`
- `DeadlineNotReached` (6) — Voting deadline not yet passed
- `AlreadyVoted` (7) — Voter already voted on this proposal
- `QuorumNotMet` (8) — Total votes below `quorum`/`quorum_bps`
- `ProposalRejected` (9) — `no_votes >= yes_votes`
- `InsufficientVotingPower` (10) — Caller holds no governance tokens
- `VotesAlreadyCast` (11) — Proposer self-cancel rejected; votes already cast
- `InvalidQuorumBps` (12) — `quorum_bps` outside `0`–`10 000`
- `VotingClosed` (13) — Voting period has ended

---

## Lottery Contract

**Location:** `contracts/lottery/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address, ticket_price: i128, winner_count: u32, prize_splits: Vec<u32>, max_tickets_per_address: Option<u32>` | `Result<(), LotteryError>` | `AlreadyInitialized`, `InvalidTicketPrice`, `InvalidWinnerConfig`, `InvalidTicketCap` |
| `buy_ticket` | `env: Env, buyer: Address` | `Result<(), LotteryError>` | `NotInitialized`, `LotteryClosed`, `TicketCapExceeded` |
| `commit` | `env: Env, hash: BytesN<32>, reveal_deadline: u32` | `Result<(), LotteryError>` | `NotInitialized`, `Unauthorized`, `LotteryClosed`, `CommitAlreadySubmitted`, `DrawAlreadyDone`, `NoTickets`, `InvalidRevealDeadline` |
| `draw` | `env: Env, secret: BytesN<32>, salt: BytesN<32>` | `Result<Vec<Address>, LotteryError>` | `NotInitialized`, `Unauthorized`, `DrawNotDone`, `DrawAlreadyDone`, `RevealMismatch`, `InsufficientParticipants` |
| `claim_refund` | `env: Env, buyer: Address` | `Result<i128, LotteryError>` | `NotInitialized`, `RefundNotAvailable`, `DrawAlreadyDone`, `NothingToRefund` |
| `get_info` | `env: Env` | `Result<LotteryInfo, LotteryError>` | `NotInitialized` |
| `get_winner` | `env: Env` | `Result<Address, LotteryError>` | `NotInitialized`, `DrawNotDone` |
| `get_winners` | `env: Env` | `Result<Vec<Address>, LotteryError>` | `NotInitialized`, `DrawNotDone` |
| `get_ticket_count` | `env: Env, buyer: Address` | `u32` | None |

Commit-reveal randomness: `commit` locks in `hash(secret ++ salt)` and a reveal deadline; `draw` verifies the reveal, then derives `winner_count` distinct winners from `hash(secret ++ salt ++ ledger ++ round)` and splits the prize pool per `prize_splits` (basis points, summing to 10 000). If the admin never calls `draw` before `reveal_deadline`, ticket holders can `claim_refund`.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `LotteryClosed` (4) — Not in `Open` state
- `DrawAlreadyDone` (5) — Already drawn
- `DrawNotDone` (6) — Not yet drawn / not yet committed
- `InvalidTicketPrice` (7) — `ticket_price` <= 0
- `CommitAlreadySubmitted` (8) — Already committed
- `RevealMismatch` (9) — Reveal doesn't match the commitment
- `NoTickets` (10) — No tickets sold
- `InvalidRevealDeadline` (11) — `reveal_deadline` not in the future
- `RefundNotAvailable` (12) — Refund conditions not met
- `NothingToRefund` (13) — Caller holds no tickets
- `InvalidWinnerConfig` (14) — Bad `winner_count`/`prize_splits`
- `InsufficientParticipants` (15) — Too few distinct participants for `winner_count`
- `InvalidTicketCap` (16) — `max_tickets_per_address` is `Some(0)`
- `TicketCapExceeded` (17) — Buyer already at their ticket cap

---

## Marketplace Contract

**Location:** `contracts/marketplace/src/lib.rs`

> **Known issue:** `lib.rs` currently contains corrupted/duplicated code — two
> conflicting `get_active_listings` definitions (one of which is actually the
> body of an offer-accept flow), references to error variants and a `NftClient`
> type that don't exist anywhere in the crate, and no function that actually
> creates an `Offer` (despite `cancel_offer` and offer-acceptance logic, and an
> `Offer` storage key, existing). The table below documents only the coherent,
> internally-consistent subset of the file, using the canonical error names
> from `errors.rs`. Treat this section as a known-incomplete placeholder until
> the contract code itself is fixed in a separate PR — see the note in
> `error-reference.md`'s Marketplace section.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, payment_token: Address, royalty_bps: u32, royalty_recipient: Address` | `Result<(), MarketplaceError>` | `AlreadyInitialized`, `InvalidRoyalty` |
| `list` | `env: Env, seller: Address, token_id: u32, price: i128` | `Result<u64, MarketplaceError>` | `NotInitialized`, `NotAuthorized` |
| `buy` | `env: Env, buyer: Address, listing_id: u64, payment_amount: i128` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive`, `InvalidPrice` |
| `cancel` | `env: Env, caller: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `NotAuthorized`, `ListingNotFound`, `ListingInactive` |
| `cancel_offer` | `env: Env, buyer: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `OfferNotFound` |
| `get_listing` | `env: Env, listing_id: u64` | `Option<Listing>` | None |
| `get_offer` | `env: Env, listing_id: u64, buyer: Address` | `Option<i128>` | None |
| `get_active_listings` | `env: Env, cursor: u64, limit: u32` | `ListingPage` | None |

**Not currently reachable through a working entry point:** making an offer (an `Offer` is only ever read/cancelled, never created) and accepting an offer (the intended logic exists but is bound to a duplicate, misnamed `get_active_listings` definition rather than its own function).

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `NotAuthorized` (3) — Caller not seller/admin
- `InvalidPrice` (4) — Listing price <= 0
- `ListingNotFound` (5) — Unknown `listing_id`
- `ListingInactive` (6) — Listing already sold/cancelled
- `InvalidRoyalty` (7) — Royalty BPS > 10 000
- `InvalidExpiry` (8) — Listing expiry not in the future
- `ListingExpired` (9) — Listing expiry has passed
- `ListingNotExpired` (10) — Sweep called on a non-expired listing
- `InvalidOfferAmount` (11) — Offer amount invalid or not below price
- `OfferNotFound` (12) — No offer for `(listing_id, buyer)`

---

## NFT Contract

**Location:** `contracts/nft/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, name: String, symbol: String, max_supply: Option<u32>, royalty_bps: Option<u32>, royalty_recipient: Option<Address>` | `Result<(), NftError>` | `AlreadyInitialized`, `InvalidRoyalty`, `RoyaltyRecipientMissing` |
| `mint` | `env: Env, to: Address, token_uri: String, token_royalty_bps: Option<u32>, token_royalty_recipient: Option<Address>` | `Result<u32, NftError>` | `NotAuthorized`, `SupplyCapReached`, `InvalidRoyalty`, `RoyaltyRecipientMissing` |
| `royalty_info` | `env: Env, token_id: u32, sale_price: i128` | `Result<Option<RoyaltyInfo>, NftError>` | `TokenNotFound` |
| `transfer` | `env: Env, from: Address, to: Address, token_id: u32` | `Result<(), NftError>` | `TokenNotFound`, `NotOwner` |
| `approve` | `env: Env, owner: Address, spender: Address, token_id: u32` | `Result<(), NftError>` | `TokenNotFound`, `NotOwner` |
| `owner_of` | `env: Env, token_id: u32` | `Result<Address, NftError>` | `TokenNotFound` |
| `token_uri` | `env: Env, token_id: u32` | `Result<String, NftError>` | `TokenNotFound` |
| `total_supply` | `env: Env` | `u32` | None |
| `get_royalty_bps` | `env: Env` | `Option<u32>` | None |
| `get_royalty_recipient` | `env: Env` | `Option<Address>` | None |

`royalty_info` resolves per-token royalty overrides (set at `mint`) before falling back to the collection-level default (set at `initialize`), returning `None` when no royalty is configured at either level. `errors.rs` names the ownership/authorization variant `NotOwner`/`NotAuthorized` and the supply-cap variant `SupplyCapReached`; the doc comments in `lib.rs` refer to these as `Unauthorized`/`MaxSupplyReached` in a few places — this table uses the canonical `errors.rs` names.

**Errors:**
- `NotAuthorized` (1) — Caller not admin/owner
- `AlreadyInitialized` (2) — `initialize` called twice
- `NotInitialized` (3) — Operation before initialize
- `TokenNotFound` (4) — Unknown `token_id`
- `TokenAlreadyMinted` (5) — Reserved; not reachable via the current sequential-ID `mint`
- `NotOwner` (6) — Caller is not the token's owner
- `NotApproved` (7) — Reserved; `transfer` does not currently check approved spenders
- `SupplyCapReached` (8) — Minting would exceed `max_supply`
- `InvalidTokenId` (9) — Reserved; entry points derive `token_id` internally
- `InvalidRoyalty` (10) — Royalty BPS > 10 000
- `RoyaltyRecipientMissing` (11) — Royalty BPS > 0 without a recipient (or vice versa)

---

## Oracle Contract

**Location:** `contracts/oracle/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, staleness_threshold: u32` | `Result<(), OracleError>` | `AlreadyInitialized`, `InvalidStalenessThreshold` |
| `update_price` | `env: Env, price: i128` | `Result<(), OracleError>` | `NotInitialized`, `Unauthorized` |
| `get_price` | `env: Env` | `Result<i128, OracleError>` | `NotInitialized`, `StalePrice` |
| `get_price_checked` | `env: Env, max_age: u64` | `Result<i128, OracleError>` | `NotInitialized`, `StalePrice` |
| `get_price_data` | `env: Env` | `Result<PriceData, OracleError>` | `NotInitialized` |
| `set_publishers` | `env: Env, admin: Address, publishers: Vec<Address>` | `Result<(), OracleError>` | `NotInitialized`, `Unauthorized` |
| `submit_price` | `env: Env, publisher: Address, price: i128` | `Result<(), OracleError>` | `Unauthorized` |
| `get_median_price` | `env: Env, max_staleness_seconds: u64` | `Result<i128, OracleError>` | `NotInitialized`, `NoPublisherData` |
| `get_twap` | `env: Env, window: u64` | `Result<i128, OracleError>` | `InsufficientHistory` |

`get_price` checks staleness in ledgers against `staleness_threshold`; `get_price_checked` instead checks a caller-supplied `max_age` in seconds. `get_median_price` takes the median of fresh multi-publisher submissions rather than trusting a single admin-pushed price. `get_twap` computes a time-weighted average over a ring buffer of up to 30 (`MAX_HISTORY`) recorded observations.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin/authorized publisher
- `StalePrice` (4) — Price older than the staleness threshold
- `InvalidStalenessThreshold` (5) — Threshold is zero
- `NoPublisherData` (6) — No fresh publisher submission
- `InsufficientHistory` (7) — No observation within the requested TWAP window

---

## Subscription Contract

**Location:** `contracts/subscription/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, provider: Address, token: Address` | `Result<(), SubscriptionError>` | `AlreadyInitialized` |
| `register_plan` | `env: Env, plan_id: Symbol, amount: i128, interval_ledgers: u32, unit_price: i128, prepay_discount_bps: u32` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotAuthorized`, `InvalidAmount`, `InvalidInterval`, `InvalidDiscount`, `PlanAlreadyExists` |
| `set_plan_active` | `env: Env, plan_id: Symbol, active: bool` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotAuthorized`, `PlanNotFound` |
| `subscribe` | `env: Env, subscriber: Address, plan_id: Symbol, trial_ledgers: Option<u32>, prepay_intervals: Option<u32>` | `Result<(), SubscriptionError>` | `NotInitialized`, `PlanNotFound`, `PlanInactive`, `AlreadySubscribed`, `ArithmeticOverflow` |
| `charge` | `env: Env, subscriber: Address` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotAuthorized`, `NotSubscribed`, `SubscriptionInactive`, `IntervalNotElapsed`, `InsufficientAllowance`, `ArithmeticOverflow` |
| `report_usage` | `env: Env, subscriber: Address, units: u64` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotAuthorized`, `InvalidAmount`, `NotSubscribed`, `SubscriptionInactive`, `ArithmeticOverflow` |
| `cancel` | `env: Env, subscriber: Address` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotSubscribed`, `SubscriptionInactive` |
| `get_subscription` | `env: Env, subscriber: Address` | `Option<SubscriptionInfo>` | None |
| `get_provider` | `env: Env` | `Option<Address>` | None |
| `get_token` | `env: Env` | `Option<Address>` | None |
| `get_plan` | `env: Env, plan_id: Symbol` | `Option<Plan>` | None |

The subscriber must pre-approve this contract as a token spender (`token.approve(subscriber, subscription_contract, amount * periods, expiry_ledger)`) before the provider can `charge`. An optional `trial_ledgers` on `subscribe` delays the first real charge; `charge` during the trial window only marks the trial complete without transferring funds.

**Metered billing:** plans with a non-zero `unit_price` bill `amount + usage_units * unit_price` per interval. The provider accumulates usage with `report_usage`; the meter resets to zero on each successful `charge` (and is discarded on `cancel`).

**Prepaid billing:** passing `prepay_intervals: Some(n)` to `subscribe` transfers `amount * n` less the plan's `prepay_discount_bps` into contract escrow. Each `charge` releases one interval's share to the provider (usage fees are still pulled from the allowance). `cancel` refunds the unconsumed escrow balance.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `NotAuthorized` (3) — Caller not the provider
- `InvalidAmount` (4) — Plan amount <= 0
- `InvalidInterval` (5) — Plan interval is zero
- `AlreadySubscribed` (6) — Subscriber already has an active subscription
- `NotSubscribed` (7) — No subscription found
- `SubscriptionInactive` (8) — Subscription cancelled
- `IntervalNotElapsed` (9) — Charge interval/trial not yet elapsed
- `InsufficientAllowance` (10) — Subscriber's token allowance too low
- `PlanAlreadyExists` (11) — Plan ID already registered
- `PlanNotFound` (12) — Unknown plan ID
- `PlanInactive` (13) — Plan deactivated
- `InvalidDiscount` (14) — `prepay_discount_bps` > 10,000
- `ArithmeticOverflow` (15) — A billing calculation overflowed

---

## Swap Contract

**Location:** `contracts/swap/src/lib.rs`

> **Known issue:** `lib.rs` currently contains corrupted/duplicated code — both
> `set_fee_bps` and `get_fee_bps` are defined twice in the same `impl` block,
> and some branches reference states/errors (`SwapState::Pending`/`Accepted`,
> `SwapError::SwapNotPending`/`SwapExpired`) that don't exist in `storage.rs`
> / `errors.rs` (which define `SwapState::Open`/`Completed`/`Cancelled` and
> `SwapError::InvalidState`/`DeadlineExpired`). The table below documents the
> coherent, internally-consistent subset of the file using the canonical
> names from `errors.rs`/`storage.rs`. Treat this section as a
> known-incomplete placeholder until the contract code itself is fixed in a
> separate PR — see the note in `error-reference.md`'s Swap section.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, fee_bps: u32` | `Result<(), SwapError>` | `AlreadyInitialized`, `InvalidFee` |
| `set_treasury` | `env: Env, new_treasury: Address` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized` |
| `set_fee_bps` | `env: Env, new_fee_bps: u32` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized`, `InvalidFee` |
| `set_admin` | `env: Env, new_admin: Address` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized` |
| `get_admin` | `env: Env` | `Result<Address, SwapError>` | `NotInitialized` |
| `get_treasury` | `env: Env` | `Result<Address, SwapError>` | `NotInitialized` |
| `get_fee_bps` | `env: Env` | `Result<u32, SwapError>` | `NotInitialized` |
| `propose_swap` | `env: Env, party_a: Address, token_a: Address, amount_a: i128, token_b: Address, amount_b: i128, expires_at: u32` | `Result<u32, SwapError>` | `NotInitialized`, `InvalidDeadline` |
| `accept_swap` | `env: Env, swap_id: u32, party_b: Address` | `Result<u32, SwapError>` | `NotInitialized`, `SwapNotFound`, `InvalidState`, `DeadlineExpired` |
| `cancel_swap` | `env: Env, swap_id: u32` | `Result<(), SwapError>` | `SwapNotFound`, `InvalidState`, `NotAuthorized` |
| `get_swap` | `env: Env, swap_id: u32` | `Result<SwapInfo, SwapError>` | `SwapNotFound` |

`accept_swap` deducts a `fee_bps` fee (paid to the admin) from `token_b`'s transfer to party A, then executes both legs of the swap atomically.

**Errors:**
- `NotAuthorized` (1) — Caller not permitted
- `SwapNotFound` (2) — Unknown `swap_id`
- `InvalidState` (3) — Swap not `Open`
- `DeadlineExpired` (4) — `expires_at` has passed
- `InvalidAmount` (5) — `amount_a`/`amount_b` <= 0
- `InvalidDeadline` (6) — `expires_at` not in the future
- `AlreadyCompleted` (7) — Swap already accepted
- `AlreadyCancelled` (8) — Swap already cancelled
- `AlreadyInitialized` (9) — `initialize` called twice
- `NotInitialized` (10) — Operation before initialize
- `InvalidFee` (11) — Fee BPS > 10 000

---

## Timelock Contract

**Location:** `contracts/timelock/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address, beneficiary: Address, release_ledger: u32, amount: i128` | `Result<(), TimelockError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidReleaseLedger` |
| `initialize_with_tranches` | `env: Env, admin: Address, token: Address, beneficiary: Address, tranches: Vec<ReleaseTranche>` | `Result<(), TimelockError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidReleaseLedger` |
| `release` | `env: Env` | `Result<(), TimelockError>` | `NotInitialized`, `AlreadyReleased`, `AlreadyCancelled`, `NotYetReleasable` |
| `cancel` | `env: Env` | `Result<(), TimelockError>` | `NotInitialized`, `NotAuthorized`, `AlreadyReleased`, `AlreadyCancelled` |
| `reassign_beneficiary` | `env: Env, new_beneficiary: Address` | `Result<(), TimelockError>` | `NotInitialized`, `NotAuthorized`, `AlreadyReleased`, `AlreadyCancelled` |
| `get_info` | `env: Env` | `Result<TimelockInfo, TimelockError>` | `NotInitialized` |
| `is_releasable` | `env: Env` | `bool` | None |
| `get_remaining_ledgers` | `env: Env` | `i64` | None |

`initialize` locks a single amount for one release ledger; `initialize_with_tranches` instead locks a schedule of `(release_ledger, amount)` tranches, releasing each independently as its ledger is reached (`release` may be called repeatedly, transitioning to `Released` only once every tranche has been paid out).

**Errors:**
- `NotAuthorized` (1) — Caller not admin
- `AlreadyInitialized` (2) — Either initializer called twice
- `NotInitialized` (3) — Operation before initialize
- `NotYetReleasable` (4) — No tranche (or the single release ledger) is due yet
- `AlreadyReleased` (5) — Fully released already
- `AlreadyCancelled` (6) — Already cancelled
- `InvalidAmount` (7) — Amount <= 0, or an empty tranche list
- `InvalidReleaseLedger` (8) — Release ledger not strictly in the future / not increasing

---

## Wrapped-Token Contract

**Location:** `contracts/wrapped-token/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, wrapped_token: Address, underlying_token: Address, max_wrap_per_address: Option<i128>` | `Result<(), WrappedTokenError>` | `AlreadyInitialized`, `InvalidAmount` |
| `wrap` | `env: Env, user: Address, amount: i128` | `Result<(), WrappedTokenError>` | `NotInitialized`, `Unauthorized`, `InvalidAmount`, `MaxWrapExceeded` |
| `unwrap` | `env: Env, user: Address, amount: i128` | `Result<(), WrappedTokenError>` | `NotInitialized`, `Unauthorized`, `InvalidAmount`, `InsufficientBalance` |
| `get_total_wrapped` | `env: Env` | `i128` | None |
| `get_reserve_balance` | `env: Env` | `Result<i128, WrappedTokenError>` | `NotInitialized` |
| `max_wrap_per_address` | `env: Env` | `Option<i128>` | None |
| `wrapped_by` | `env: Env, account: Address` | `i128` | None |
| `pause` *(feature `pausable`)* | `env: Env` | `Result<(), WrappedTokenError>` | `NotInitialized` |
| `unpause` *(feature `pausable`)* | `env: Env` | `Result<(), WrappedTokenError>` | `NotInitialized` |

`wrap`/`unwrap` maintain a 1:1 peg between the wrapped and underlying tokens; `get_total_wrapped() <= get_reserve_balance()` should always hold (see `monitoring.md`'s reserve-backing invariant). `pause`/`unpause` and the `Unauthorized`-while-paused check on `wrap`/`unwrap` only compile when the `pausable` feature is enabled.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Contract paused (`pausable` feature only)
- `InvalidAmount` (4) — Amount <= 0, or an invalid cap at initialize
- `InsufficientBalance` (5) — Caller lacks enough wrapped tokens to unwrap
- `InsufficientReserve` (6) — Contract lacks enough underlying reserve
- `MaxWrapExceeded` (7) — Would exceed `max_wrap_per_address`

---

## Cross-Contract Patterns

### Token Interface
All contracts that transfer tokens implement the standard Soroban token interface:
- `transfer(from, to, amount)`
- `transfer_from(spender, from, to, amount)`
- `approve(from, spender, amount, expiration_ledger)`
- `balance(id)`
- `allowance(from, spender)`

### Admin Pattern
Contracts with admin controls require:
1. Admin address set at initialization
2. `require_auth()` on admin operations
3. TTL bumping on state changes

### Error Handling
All contracts follow consistent error patterns:
- `AlreadyInitialized` / `NotInitialized` gate operations
- `Unauthorized` for permission failures
- `InvalidAmount` for validation failures
- Specific errors for state machine violations
