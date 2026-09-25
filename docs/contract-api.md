# Contract API Reference

Complete public API documentation for all Soroban starter kit contracts.

## Token Contract

**Location:** `contracts/token/src/lib.rs`

### Core Operations

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

### Admin Transfer

Two-step admin handoff — the incoming admin must accept before control transfers, avoiding accidental lockout to an unreachable address.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `propose_admin` | `env: Env, new_admin: Address` | `Result<(), TokenError>` | `NotInitialized` |
| `accept_admin` | `env: Env` | `Result<(), TokenError>` | `Unauthorized` |
| `cancel_admin_proposal` | `env: Env` | `Result<(), TokenError>` | `NotInitialized` |
| `set_admin` | `env: Env, new_admin: Address` | `Result<(), TokenError>` | `NotInitialized` |
| `admin` | `env: Env` | `Result<Address, TokenError>` | `NotInitialized` |

`set_admin` transfers admin immediately in a single call and exists for backward compatibility; `propose_admin` + `accept_admin` is the recommended two-step flow. `accept_admin` must be called by the pending admin and fails with `Unauthorized` if no proposal is pending or the caller isn't it.

### Batch, Burn & Versioning

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `batch_mint` | `env: Env, recipients: Vec<(Address, i128)>` | `Result<(), TokenError>` | `Unauthorized`, `InvalidAmount`, `Overflow` |
| `admin_burn` | `env: Env, from: Address, amount: i128` | `Result<(), TokenError>` | `Unauthorized`, `InvalidAmount`, `Overflow` |
| `balance_of` | `env: Env, id: Address` | `Option<i128>` | None |
| `version` | `env: Env` | `String` | None |
| `contract_version` | `env: Env` | `u32` | None |

`batch_mint` mints to multiple recipients atomically in one call. `admin_burn` is the admin-initiated counterpart to `burn` (which burns from the caller's own balance). `balance_of` returns `None` for an account with no storage entry, unlike `balance` which returns `0`. `version` returns the git commit hash baked in at compile time; `contract_version` returns the on-chain schema version, bumped by `execute_upgrade` *(feature `upgradeable`)*.

### Permit (Gasless Approvals)

ERC-2612-style signed approvals — a spender can submit `owner`'s pre-signed approval without `owner` submitting a transaction themselves.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `set_permit_signer` | `env: Env, owner: Address, public_key: BytesN<32>` | `()` | None |
| `permit_signer` | `env: Env, owner: Address` | `Option<BytesN<32>>` | None |
| `permit_nonce` | `env: Env, owner: Address` | `u32` | None |
| `approve_with_signature` | `env: Env, owner: Address, spender: Address, amount: i128, nonce: u32, expiry_ledger: u32, signature: BytesN<64>` | `Result<(), TokenError>` | `PermitExpired`, `InvalidNonce`, `PermitSignerNotSet` |
| `allowance_expiry` | `env: Env, from: Address, spender: Address` | `Option<u32>` | None |

`owner` registers an ed25519 public key via `set_permit_signer`; `approve_with_signature` verifies a signature over `(owner, spender, amount, nonce, expiry_ledger)` and grants the allowance without `owner` signing the transaction itself. Replay is prevented by `permit_nonce`, which must match exactly and advances on each successful call.

### Governance Snapshots

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `snapshot` | `env: Env, caller: Address, ledger: u32` | `Result<(), TokenError>` | None |
| `balance_at` | `env: Env, account: Address, ledger: u32` | `Option<i128>` | None |

`snapshot` records the caller's own balance at a given ledger for later voting-power lookups (see the DAO contract); `balance_at` returns `None` if no snapshot was recorded for that `(account, ledger)` pair.

### Feature-Gated Operations

| Function | Feature | Parameters | Returns | Errors |
|----------|---------|-----------|---------|--------|
| `pause` | `pausable` | `env: Env` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `unpause` | `pausable` | `env: Env` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `freeze_account` | `freeze` | `env: Env, account: Address` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `unfreeze_account` | `freeze` | `env: Env, account: Address` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `max_supply` | `capped-supply` | `env: Env` | `Option<i128>` | None |
| `propose_upgrade` | `upgradeable` | `env: Env, wasm_hash: BytesN<32>` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `execute_upgrade` | `upgradeable` | `env: Env` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `set_transfer_hook` | `transfer-hook` | `env: Env, hook: Option<Address>` | `Result<(), TokenError>` | `Unauthorized`, `NotInitialized` |
| `get_transfer_hook` | `transfer-hook` | `env: Env` | `Option<Address>` | None |

While `pausable`/`freeze` are enabled, `mint`/`admin_burn`/transfer-family calls that would otherwise succeed instead return `Unauthorized` if the contract is paused or the `from` account is frozen. `propose_upgrade` starts a 17,280-ledger (~24h) timelock before `execute_upgrade` can install the new Wasm and bump `contract_version`. `set_transfer_hook` registers a contract to receive an `on_transfer(from, to, amount)` callback on every transfer; a hook failure does not revert the transfer.

**Errors:**
- `InsufficientBalance` (1) — Caller's balance too low
- `InsufficientAllowance` (2) — Allowance too low for transfer_from
- `Unauthorized` (3) — Caller not admin, or a `pausable`/`freeze`-gated check failed
- `AlreadyInitialized` (4) — initialize called twice
- `NotInitialized` (5) — Operation before initialize
- `InvalidAmount` (6) — Amount zero, negative, or exceeds cap
- `Overflow` (7) — Arithmetic overflow
- `InvalidNonce` (8) — `approve_with_signature` nonce doesn't match `permit_nonce(owner)`
- `PermitExpired` (9) — `approve_with_signature` called past `expiry_ledger`
- `PermitSignerNotSet` (10) — `owner` never called `set_permit_signer`

---

## Escrow Contract

**Location:** `contracts/escrow/src/lib.rs`

### Core Operations

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, buyer: Address, seller: Address, arbiter: Address, token_contract: Address, amount: i128, deadline_ledger: u32, dispute_timeout_ledgers: u32, metadata_hash: Option<BytesN<32>>` | `Result<(), EscrowError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidParties` |
| `initialize_with_arbiters` | `env: Env, admin: Address, buyer: Address, seller: Address, arbiters: Vec<Address>, token_contract: Address, amount: i128, deadline_ledger: u32, required_signatures: u32, dispute_timeout_ledgers: u32, metadata_hash: Option<BytesN<32>>` | `Result<(), EscrowError>` | Same + validation |
| `initialize_with_milestones` | `env: Env, admin: Address, buyer: Address, seller: Address, arbiter: Address, token_contract: Address, milestones: Vec<Milestone>, deadline_ledger: u32, dispute_timeout_ledgers: u32, metadata_hash: Option<BytesN<32>>` | `Result<(), EscrowError>` | Same + validation |
| `update_amount` | `env: Env, new_amount: i128` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState`, `InvalidAmount` |
| `fund` | `env: Env` | `Result<(), EscrowError>` | `InvalidState`, `InsufficientFunds` |
| `mark_delivered` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `approve_delivery` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `release_partial` | `env: Env, amount: i128` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState`, `InvalidAmount` |
| `release_milestone` | `env: Env, caller: Address, milestone_index: u32` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` — `caller` must be the buyer or arbiter |
| `request_refund` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `request_partial_refund` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `raise_dispute` | `env: Env, caller: Address` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `resolve_dispute` | `env: Env, caller: Address, release_to_seller: bool` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `claim_dispute_timeout` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `cancel` | `env: Env` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `extend_deadline` | `env: Env, new_deadline: u32` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidState` |
| `bump` | `env: Env` | `Result<(), EscrowError>` | `NotInitialized` — refreshes storage TTL |
| `set_fee_config` | `env: Env, fee_bps: u32, treasury: Address` | `Result<(), EscrowError>` | `NotAuthorized`, `InvalidAmount` — must be called by the buyer |
| `pause` †| `env: Env` | `Result<(), EscrowError>` | `NotAuthorized` |
| `unpause` †| `env: Env` | `Result<(), EscrowError>` | `NotAuthorized` |
| `propose_upgrade` †| `env: Env, wasm_hash: BytesN<32>` | `Result<(), EscrowError>` | `NotAuthorized` |
| `execute_upgrade` †| `env: Env` | `Result<(), EscrowError>` | `NotAuthorized` |

† Only compiled when the contract's `pausable` Cargo feature is enabled.

### Query Functions

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `get_escrow_info` | `env: Env` | `Result<EscrowInfo, EscrowError>` | `NotInitialized` |
| `get_state` | `env: Env` | `Option<EscrowState>` | None |
| `is_deadline_passed` | `env: Env` | `bool` | None |
| `get_remaining_ledgers` | `env: Env` | `i64` | None |
| `get_fee_config` | `env: Env` | `(u32, Option<Address>)` | None — `(fee_bps, treasury)`, or `(0, None)` if unconfigured |
| `get_milestones` | `env: Env` | `Vec<Milestone>` | None — empty for a non-milestone escrow |
| `contract_version` | `env: Env` | `u32` | None |
| `version` †| `env: Env` | `String` | None — build/git-hash version string |

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
| `initialize` | `env: Env, admin: Address, stake_token: Address, reward_token: Address, unbonding_period: u32, slash_destination: Address` | `Result<(), StakingError>` | `AlreadyInitialized` |
| `stake` | `env: Env, staker: Address, amount: i128` | `Result<(), StakingError>` | `NotInitialized`, `InvalidAmount` |
| `unstake` | `env: Env, staker: Address, amount: i128` | `Result<(), StakingError>` | `NotInitialized`, `InvalidAmount`, `InsufficientStake`, `NoStake`, `UnbondRequestPending` |
| `withdraw` | `env: Env, staker: Address` | `Result<i128, StakingError>` | `NotInitialized`, `NoUnbondRequest`, `UnbondingNotComplete` |
| `claim_rewards` | `env: Env, staker: Address` | `Result<i128, StakingError>` | `NotInitialized`, `NoRewards` |
| `add_rewards` | `env: Env, amount: i128` | `Result<(), StakingError>` | `Unauthorized`, `NotInitialized`, `InvalidAmount` |
| `slash` | `env: Env, staker: Address, amount: i128` | `Result<i128, StakingError>` | `Unauthorized`, `NotInitialized`, `InvalidAmount`, `NoStake` |
| `set_compounding` | `env: Env, staker: Address, enabled: bool` | `Result<(), StakingError>` | `NotInitialized` |
| `compound` | `env: Env, staker: Address` | `Result<i128, StakingError>` | `NotInitialized`, `NoRewards`, `CompoundTokenMismatch` |
| `get_stake` | `env: Env, staker: Address` | `i128` | None |
| `get_rewards` | `env: Env, staker: Address` | `i128` | None |
| `get_total_staked` | `env: Env` | `i128` | None |
| `get_total_rewards` | `env: Env` | `i128` | None |
| `get_unbond_request` | `env: Env, staker: Address` | `Option<UnbondRequest>` | None |
| `contract_version` | `env: Env` | `u32` | None |

**Errors:**
- `AlreadyInitialized` (1) — initialize called twice
- `NotInitialized` (2) — Operation before initialize
- `Unauthorized` (3) — Caller not admin
- `InvalidAmount` (4) — Amount zero or negative
- `NoStake` (5) — No stake to unstake/claim/slash
- `InsufficientStake` (6) — Unstake amount exceeds stake
- `NoRewards` (7) — No rewards available
- `CompoundTokenMismatch` (8) — Stake token and reward token differ
- `UnbondingNotComplete` (9) — `withdraw` called before the unbonding period elapsed
- `NoUnbondRequest` (10) — `withdraw` called with no pending unbond request
- `UnbondRequestPending` (11) — `unstake` called while an unbond request is already pending

---

## Vesting Contract

**Location:** `contracts/vesting/src/lib.rs`

The contract supports **multiple beneficiaries per deployed instance**: `initialize` sets up only the admin and token once, then the admin calls `create_schedule` independently for each beneficiary.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, token: Address` | `Result<(), VestingError>` | `AlreadyInitialized` |
| `create_schedule` | `env: Env, beneficiary: Address, cliff_ledger: u32, end_ledger: u32, amount: i128` | `Result<(), VestingError>` | `NotInitialized`, `InvalidAmount`, `InvalidSchedule`, `ScheduleAlreadyExists` |
| `claim` | `env: Env, beneficiary: Address` | `Result<i128, VestingError>` | `NotInitialized`, `ScheduleNotFound`, `NothingToClaim` |
| `revoke` | `env: Env, beneficiary: Address` | `Result<i128, VestingError>` | `NotInitialized`, `ScheduleNotFound`, `AlreadyRevoked` |
| `admin_release` | `env: Env, beneficiary: Address` | `Result<i128, VestingError>` | `NotInitialized`, `ScheduleNotFound`, `AlreadyRevoked`, `CliffAlreadyPassed`, `NothingToClaim` |
| `get_info` | `env: Env, beneficiary: Address` | `Option<BeneficiarySchedule>` | None |
| `claimable` | `env: Env, beneficiary: Address` | `i128` | None |
| `contract_version` | `env: Env` | `u32` | None |

`create_schedule` transfers `amount` tokens from the admin into the contract when the schedule is created, and returns `ScheduleAlreadyExists` if the beneficiary already has one. `claim` releases all currently vested, unclaimed tokens and returns the amount transferred. `revoke` cancels future vesting for a beneficiary: already-vested tokens remain claimable, and unvested tokens are returned to the admin immediately. `admin_release` is an emergency unlock — before a beneficiary's cliff is reached, the admin can release their entire remaining allocation early (marking the schedule revoked in the process).

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before `initialize`
- `NotAuthorized` (3) — Reserved; caller identity is enforced via `require_auth`, not this error
- `InvalidAmount` (4) — `amount` <= 0
- `InvalidSchedule` (5) — `cliff_ledger >= end_ledger`, or `end_ledger` not in the future
- `NothingToClaim` (6) — No tokens vested/releasable since the last claim
- `AlreadyRevoked` (7) — `revoke`/`admin_release` called on an already-revoked schedule
- `CliffAlreadyPassed` (8) — `admin_release` called after the beneficiary's cliff ledger has been reached
- `ScheduleAlreadyExists` (9) — `create_schedule` called twice for the same beneficiary
- `ScheduleNotFound` (10) — No schedule exists for the given beneficiary

---

## Multisig Contract

**Location:** `contracts/multisig/src/lib.rs`

### Core Operations

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, signers: Vec<Address>, threshold: u32, weights: Option<Vec<SignerWeight>>` | `Result<(), MultisigError>` | `AlreadyInitialized`, `InvalidThreshold`, `InvalidSigners`, `InvalidWeight` |
| `add_signer` | `env: Env, approvals: Vec<Address>, signer: Address, weight: u32, new_threshold: u32` | `Result<(), MultisigError>` | `NotInitialized`, `NotSigner`, `InsufficientApprovals`, `InvalidThreshold`, `InvalidSigners`, `InvalidWeight` |
| `remove_signer` | `env: Env, approvals: Vec<Address>, signer: Address, new_threshold: u32` | `Result<(), MultisigError>` | `NotInitialized`, `NotSigner`, `InsufficientApprovals`, `InvalidThreshold`, `InvalidSigners` |
| `update_signer_weight` | `env: Env, approvals: Vec<Address>, signer: Address, new_weight: u32` | `Result<(), MultisigError>` | `NotInitialized`, `NotSigner`, `InsufficientApprovals`, `InvalidThreshold`, `InvalidSigners`, `InvalidWeight` |
| `propose_signer_change` | `env: Env, proposer: Address, change: SignerChange, expiry_ledgers: u32` | `Result<u64, MultisigError>` | `NotSigner`, `NotInitialized` |
| `sign_signer_change` | `env: Env, signer: Address, proposal_id: u64` | `Result<(), MultisigError>` | `NotSigner`, `TransactionNotFound`, `AlreadyExecuted`, `ProposalExpired`, `AlreadySigned` |
| `execute_signer_change` | `env: Env, proposal_id: u64` | `Result<(), MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `ProposalExpired`, `NotInitialized`, `ThresholdNotMet`, plus any error from the applied change |
| `get_signer_proposal` | `env: Env, proposal_id: u64` | `Option<SignerProposal>` | None |
| `propose_transaction` | `env: Env, proposer: Address, target: Address, function: Symbol, args: Vec<Val>, expiry_ledgers: u32` | `Result<u64, MultisigError>` | `NotSigner`, `NotInitialized` |
| `sign_transaction` | `env: Env, signer: Address, tx_id: u64` | `Result<(), MultisigError>` | `NotSigner`, `TransactionNotFound`, `AlreadyExecuted`, `ProposalExpired`, `AlreadySigned` |
| `execute_transaction` | `env: Env, tx_id: u64` | `Result<Val, MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `ProposalExpired`, `NotInitialized`, `ThresholdNotMet` |
| `cancel_proposal` | `env: Env, proposer: Address, tx_id: u64` | `Result<(), MultisigError>` | `TransactionNotFound`, `NotProposer`, `AlreadyExecuted` |
| `revoke_signature` | `env: Env, signer: Address, tx_id: u64` | `Result<(), MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `NotSigned` |
| `execute_batch` | `env: Env, proposal_ids: Vec<u64>` | `Vec<u64>` | None |
| `get_signers` | `env: Env` | `Vec<Address>` | None |
| `get_threshold` | `env: Env` | `Option<u32>` | None |
| `get_signer_weight` | `env: Env, signer: Address` | `u32` | None |
| `is_signer` | `env: Env, address: Address` | `bool` | None |
| `get_transaction` | `env: Env, tx_id: u64` | `Option<Transaction>` | None |
| `signature_count` | `env: Env, tx_id: u64` | `Option<u32>` | None |
| `cleanup_expired` | `env: Env, tx_id: u64` | `Result<(), MultisigError>` | `TransactionNotFound`, `AlreadyExecuted`, `NotYetExpired` |
| `contract_version` | `env: Env` | `u32` | None |

`initialize`'s optional `weights` parameter assigns each signer a custom vote weight (any signer omitted from `weights` defaults to weight 1); `threshold` is then measured in accumulated weight rather than raw signer count, so unweighted wallets behave exactly like the original flat-count design. `execute_batch` attempts each proposal ID independently — a failure for one (not found, already executed, expired, or under threshold) is silently skipped rather than aborting the batch; diff the returned `Vec<u64>` against the input to see what ran, or inspect individual proposals via `get_transaction`. Every proposal expires `expiry_ledgers` after `propose_transaction`; `cleanup_expired` lets anyone reclaim storage for an expired, unexecuted proposal. Before execution the original proposer may `cancel_proposal` (removing it from storage), and any signer may `revoke_signature`, which recomputes `accumulated_weight` from the remaining signatures using current weights. `add_signer` takes a custom `weight` (≥ 1), and `update_signer_weight` changes an existing signer's weight while revalidating the current threshold against the new total weight. Signer additions, removals, weight updates, and threshold changes can also be approved asynchronously: `propose_signer_change` stores a `SignerChange` (`AddSigner`, `RemoveSigner`, `UpdateSignerWeight`, `ChangeThreshold`) that signers approve over time with `sign_signer_change`; `execute_signer_change` applies it once accumulated weight meets the threshold.

**Errors:**
- `AlreadyInitialized` (1) — `initialize` called twice
- `NotInitialized` (2) — Operation before initialize
- `InvalidThreshold` (3) — Threshold zero or greater than total signer weight
- `InvalidSigners` (4) — Signers empty, duplicate, or invalid
- `NotSigner` (5) — Caller/approver not in signer set
- `TransactionNotFound` (6) — TX ID does not exist
- `AlreadyExecuted` (7) — Transaction already executed
- `AlreadySigned` (8) — Signer already signed this transaction
- `ThresholdNotMet` (9) — Accumulated weight below threshold
- `InsufficientApprovals` (10) — Signer-management approval list lacks threshold weight
- `ProposalExpired` (11) — Proposal is past its `expiry_ledger`
- `InvalidWeight` (12) — A `SignerWeight.weight` of zero was supplied
- `NotYetExpired` (13) — `cleanup_expired` called before the proposal's expiry ledger was reached
- `NotProposer` (14) — `cancel_proposal` called by someone other than the original proposer
- `NotSigned` (15) — `revoke_signature` called by a signer who has not signed the transaction

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
| `start` | `env: Env, seller: Address, token: Address, start_price: i128, min_increment: i128, deadline: u32, reserve_price: Option<i128>, extension_window: u32, cancellation_grace_ledgers: u32, cancellation_fee: i128` | `Result<(), AuctionError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidDeadline` |
| `bid` | `env: Env, bidder: Address, amount: i128` | `Result<(), AuctionError>` | `InvalidAmount`, `AuctionEnded`, `NotInitialized`, `BidTooLow` |
| `cancel` | `env: Env, seller: Address` | `Result<(), AuctionError>` | `NotInitialized`, `NotAuthorized`, `AlreadyEnded`, `BidAlreadyPlaced`, `InvalidAmount` |
| `end` | `env: Env` | `Result<(), AuctionError>` | `NotInitialized`, `AuctionNotEnded`, `AlreadyEnded` |
| `start` | `env: Env, seller: Address, token: Address, start_price: i128, min_increment: i128, deadline: u32, reserve_price: Option<i128>, extension_window: u32, nft_contract: Option<Address>, token_id: Option<u32>` | `Result<(), AuctionError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidDeadline`, `InvalidNftParams` |
| `start_dutch` | `env: Env, seller: Address, token: Address, start_price: i128, floor_price: i128, start_ledger: u32, duration_ledgers: u32, nft_contract: Option<Address>, token_id: Option<u32>` | `Result<(), AuctionError>` | `AlreadyInitialized`, `InvalidAmount`, `InvalidDeadline`, `Overflow`, `InvalidNftParams` |
| `bid` | `env: Env, bidder: Address, amount: i128` | `Result<(), AuctionError>` | `InvalidAmount`, `WrongMode`, `AuctionEnded`, `NotInitialized`, `BidTooLow`, `Overflow` |
| `bid_with_credit` | `env: Env, bidder: Address, total_bid: i128` | `Result<(), AuctionError>` | Same as `bid` |
| `get_current_price` | `env: Env` | `Result<i128, AuctionError>` | `NotInitialized`, `WrongMode` |
| `buy` | `env: Env, buyer: Address, max_price: i128` | `Result<i128, AuctionError>` (price paid) | `NotInitialized`, `WrongMode`, `AuctionEnded`, `AuctionNotStarted`, `BidTooLow` |
| `cancel` | `env: Env, seller: Address` | `Result<(), AuctionError>` | `NotInitialized`, `NotAuthorized`, `AlreadyEnded`, `BidAlreadyPlaced` |
| `end` | `env: Env` | `Result<(), AuctionError>` | `NotInitialized`, `WrongMode`, `AuctionNotEnded`, `AlreadyEnded` |
| `withdraw` | `env: Env, bidder: Address` | `Result<(), AuctionError>` | `NothingToWithdraw` |
| `get_pending` | `env: Env, bidder: Address` | `i128` | None |
| `get_dutch_config` | `env: Env` | `Option<DutchConfig>` | None |
| `get_info` | `env: Env` | `Result<AuctionInfo, AuctionError>` | `NotInitialized` |
| `is_cancelled` | `env: Env` | `bool` | None |

`bid` extends `deadline` by `extension_window` ledgers when a bid lands within that window of the current deadline (anti-sniping). `end` settles to the seller when `highest_bid >= reserve_price` (or no reserve is set); otherwise it refunds the highest bidder and the item goes unsold. `cancel` always succeeds before the first bid is placed. After a bid, it only succeeds inside the cancellation grace window (`current_ledger <= start_ledger + cancellation_grace_ledgers`, disabled when `cancellation_grace_ledgers` is `0`): the seller pays `cancellation_fee` into the contract, the top bidder's pending refund is credited with their full bid plus the fee, and a `cancelled_with_compensation` event carrying `AuctionCancelledWithCompensation { seller, top_bidder, compensation_amount }` is emitted. A cancelled auction cannot be settled with `end`.

**Checked arithmetic (#1070):** the minimum-bid computation (`highest_bid + min_increment`), refund queueing (`pending + highest_bid`), credit offsets, and Dutch price decay all use checked operations and return `Overflow` rather than trapping.

**Custodial NFT escrow (#1069):** when `nft_contract` and `token_id` are both supplied, `start`/`start_dutch` transfer the NFT from the seller into the auction contract (the seller's authorization of `start` covers the nested NFT `transfer`). The NFT is delivered to the winner in the same `end`/`buy` invocation that pays the seller, and returned to the seller on `cancel`, on `end` with no bids, or when the reserve is not met. The NFT contract must expose `transfer(from: Address, to: Address, token_id: u32)`, as `contracts/nft` does.

**Refund-credit counter-bids (#1068):** `bid_with_credit` applies the bidder's `Pending(bidder)` balance toward `total_bid` and transfers only `total_bid - credit` from their wallet (nothing when the credit covers the bid). Unused credit stays pending and withdrawable. The current highest bidder may also use it to raise their own bid, paying only the increase.

**Dutch auctions (#1071):** `start_dutch` requires `0 <= floor_price < start_price`, `duration_ledgers > 0`, and a `start_ledger` at or after the current ledger. The price is

```text
price = start_price - (start_price - floor_price) * (now - start_ledger) / duration_ledgers
```

equal to `start_price` before `start_ledger` and clamped to `floor_price` from `start_ledger + duration_ledgers` onwards. `buy` succeeds for the first caller with `max_price >= get_current_price()`: the auction is marked settled before any external call, the price goes directly from buyer to seller, and the NFT (if any) goes to the buyer. English-only calls (`bid`, `bid_with_credit`, `end`) return `WrongMode` on a Dutch auction, and vice versa.

**Errors:**
- `AlreadyInitialized` (1) — `start` called twice
- `NotInitialized` (2) — Operation before `start`
- `AuctionEnded` (3) — Deadline passed or auction cancelled
- `AuctionNotEnded` (4) — `end` called before the deadline
- `BidTooLow` (5) — Bid below the required minimum
- `AlreadyEnded` (6) — Already settled or cancelled
- `NoBids` (7) — Reserved; `end()` handles the no-bids case via an event, not this error
- `NotAuthorized` (8) — Caller is not the seller
- `InvalidAmount` (9) — `start_price`/`min_increment`/bid <= 0, or `cancellation_fee` < 0
- `InvalidDeadline` (10) — `deadline` not in the future
- `NothingToWithdraw` (11) — No pending refund
- `ReserveNotMet` (12) — Reserved; `end()` handles this case via an event, not this error
- `BidAlreadyPlaced` (13) — `cancel` called after a bid was placed, outside the cancellation grace window
- `BidAlreadyPlaced` (13) — `cancel` called after a bid was placed
- `Overflow` (14) — Checked arithmetic on a bid, refund, credit, or Dutch price overflowed
- `WrongMode` (15) — English-only call on a Dutch auction, or vice versa
- `AuctionNotStarted` (16) — `buy` before the Dutch `start_ledger`
- `InvalidNftParams` (17) — Only one of `nft_contract` / `token_id` supplied

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
| `get_proposals` | `env: Env, cursor: u32, limit: u32, state: Option<ProposalState>` | `ProposalPage` | None |

Voting weight is the voter's token balance at vote time, capped at the total supply snapshot taken at proposal creation (flash-loan resistant). `execute_proposal` requires both the absolute `quorum` and, if `quorum_bps > 0`, that participation reach `quorum_bps` of the snapshotted total supply, plus `yes_votes > no_votes`. `get_proposals` lists proposals in ascending ID order via `soroban_common::paginate`, optionally filtered by `ProposalState`; `limit` is clamped to `[1, MAX_PAGE_SIZE]` (50), and the returned `next_cursor` is `None` once the end is reached.

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

Each listing carries its own `payment_token`; purchases, offers and royalties on
that listing all settle in that token. The admin may optionally enforce a
payment-token whitelist on new listings (off by default; the default token
passed to `initialize` is always whitelisted). Batch entry points accept at most
`MAX_BATCH_SIZE` (20) items and are all-or-nothing.

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, payment_token: Address, royalty_bps: u32, royalty_recipient: Address` | `Result<(), MarketplaceError>` | `AlreadyInitialized`, `InvalidRoyalty` |
| `set_payment_token_allowed` | `env: Env, token: Address, allowed: bool` (admin) | `Result<(), MarketplaceError>` | `NotInitialized` |
| `set_whitelist_enabled` | `env: Env, enabled: bool` (admin) | `Result<(), MarketplaceError>` | `NotInitialized` |
| `is_whitelist_enabled` | `env: Env` | `bool` | None |
| `is_payment_token_allowed` | `env: Env, token: Address` | `bool` | None |
| `get_default_payment_token` | `env: Env` | `Option<Address>` | None |
| `list` | `env: Env, seller: Address, nft_contract: Address, token_id: u32, price: i128, payment_token: Address` | `Result<u64, MarketplaceError>` | `NotInitialized`, `InvalidPrice`, `PaymentTokenNotAllowed` |
| `list_with_expiry` | `env: Env, seller: Address, nft_contract: Address, token_id: u32, price: i128, payment_token: Address, expires_at: u32` | `Result<u64, MarketplaceError>` | `NotInitialized`, `InvalidPrice`, `InvalidExpiry`, `PaymentTokenNotAllowed` |
| `list_batch` | `env: Env, seller: Address, items: Vec<ListingParams>` | `Result<Vec<u64>, MarketplaceError>` | `EmptyBatch`, `BatchTooLarge`, + `list_with_expiry` errors |
| `buy` | `env: Env, buyer: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive`, `ListingExpired`, `SellerNotOwner` |
| `buy_batch` | `env: Env, buyer: Address, listing_ids: Vec<u64>` | `Result<(), MarketplaceError>` | `EmptyBatch`, `BatchTooLarge`, + `buy` errors |
| `cancel` | `env: Env, seller: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `NotAuthorized`, `ListingNotFound`, `ListingInactive` |
| `cancel_batch` | `env: Env, seller: Address, listing_ids: Vec<u64>` | `Result<(), MarketplaceError>` | `EmptyBatch`, `BatchTooLarge`, + `cancel` errors |
| `sweep_expired` | `env: Env, seller: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `NotAuthorized`, `ListingNotFound`, `ListingInactive`, `ListingNotExpired` |
| `invalidate_listing` | `env: Env, listing_id: u64` (permissionless) | `Result<bool, MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive` |
| `make_offer` | `env: Env, buyer: Address, listing_id: u64, amount: i128` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive`, `InvalidOfferAmount` |
| `accept_offer` | `env: Env, seller: Address, listing_id: u64, buyer: Address` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive`, `NotAuthorized`, `OfferNotFound`, `SellerNotOwner` |
| `cancel_offer` | `env: Env, buyer: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `OfferNotFound` |
| `sweep_offers` | `env: Env, listing_id: u64, buyers: Vec<Address>` (permissionless) | `Result<u32, MarketplaceError>` | `NotInitialized`, `EmptyBatch`, `BatchTooLarge`, `ListingNotFound`, `ListingStillActive` |
| `get_listing` | `env: Env, listing_id: u64` | `Option<Listing>` | None |
| `get_offer` | `env: Env, listing_id: u64, buyer: Address` | `Option<i128>` | None |
| `get_active_listings` | `env: Env, cursor: u64, limit: u32` | `ListingPage` (ghost listings filtered out) | None |
| `list` | `env: Env, seller: Address, token_id: u32, price: i128` | `Result<u64, MarketplaceError>` | `NotInitialized`, `NotAuthorized` |
| `buy` | `env: Env, buyer: Address, listing_id: u64, max_price: i128` | `Result<(), MarketplaceError>` | `NotInitialized`, `ListingNotFound`, `ListingInactive`, `ListingExpired`, `PriceExceedsMax` |
| `cancel` | `env: Env, caller: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `NotAuthorized`, `ListingNotFound`, `ListingInactive` |
| `cancel_offer` | `env: Env, buyer: Address, listing_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `OfferNotFound` |
| `get_listing` | `env: Env, listing_id: u64` | `Option<Listing>` | None |
| `get_offer` | `env: Env, listing_id: u64, buyer: Address` | `Option<i128>` | None |
| `get_active_listings` | `env: Env, cursor: u64, limit: u32` | `ListingPage` | None |
| `make_collection_offer` | `env: Env, buyer: Address, nft_contract: Address, amount: i128, expires_at: u32` | `Result<u64, MarketplaceError>` | `NotInitialized`, `InvalidOfferAmount`, `InvalidExpiry` |
| `accept_collection_offer` | `env: Env, seller: Address, offer_id: u64, token_id: u32` | `Result<(), MarketplaceError>` | `NotInitialized`, `CollectionOfferNotFound`, `CollectionOfferExpired`, `NotAuthorized` |
| `cancel_collection_offer` | `env: Env, buyer: Address, offer_id: u64` | `Result<(), MarketplaceError>` | `NotInitialized`, `CollectionOfferNotFound`, `NotAuthorized` |
| `get_collection_offer` | `env: Env, offer_id: u64` | `Option<CollectionOffer>` | None |
| `set_royalty_splits` | `env: Env, admin: Address, splits: Vec<(Address, u32)>` | `Result<(), MarketplaceError>` | `NotInitialized`, `NotAuthorized`, `InvalidRoyalty` |
| `get_royalty_splits` | `env: Env` | `Vec<(Address, u32)>` | None |

`buy` takes a `max_price` slippage bound: pass the price you observed; the call fails with `PriceExceedsMax` if the listing price is higher when it executes.

`set_royalty_splits` splits the marketplace royalty across up to 10 `(recipient, bps)` entries whose basis points sum to the total royalty (≤ 10 000). A royalty returned by the NFT contract's `royalty_info` still takes priority.

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
- `PaymentTokenNotAllowed` (13) — Whitelist enabled and listing token not on it
- `SellerNotOwner` (14) — Seller no longer owns the listed NFT (ghost listing)
- `BatchTooLarge` (15) — Batch exceeds `MAX_BATCH_SIZE`
- `EmptyBatch` (16) — Batch contains no items
- `ListingStillActive` (17) — `sweep_offers` called on an open listing
- `Reentrant` (18) — Reentrancy guard already held

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
| `subscribe` | `env: Env, subscriber: Address, plan_id: Symbol, trial_ledgers: Option<u32>` | `Result<(), SubscriptionError>` | `NotInitialized`, `PlanNotFound`, `PlanInactive`, `AlreadySubscribed` |
| `set_grace_period` | `env: Env, grace_period_ledgers: u32` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotAuthorized` |
| `charge` | `env: Env, subscriber: Address, plan_id: Symbol` | `Result<ChargeOutcome, SubscriptionError>` | `NotInitialized`, `NotAuthorized`, `NotSubscribed`, `SubscriptionSuspended`, `SubscriptionInactive`, `IntervalNotElapsed` |
| `charge_batch` | `env: Env, subscriptions: Vec<(Address, Symbol)>` | `Result<BatchChargeResult, SubscriptionError>` | `NotInitialized`, `NotAuthorized` |
| `change_plan` | `env: Env, subscriber: Address, old_plan_id: Symbol, new_plan_id: Symbol` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotSubscribed`, `SubscriptionSuspended`, `SubscriptionInactive`, `PlanNotFound`, `PlanInactive`, `AlreadySubscribed`, `PaymentOverdue`, `ArithmeticOverflow` |
| `cancel` | `env: Env, subscriber: Address, plan_id: Symbol` | `Result<(), SubscriptionError>` | `NotInitialized`, `NotSubscribed`, `SubscriptionInactive` |
| `get_subscription` | `env: Env, subscriber: Address, plan_id: Symbol` | `Option<SubscriptionInfo>` | None |
| `get_provider` | `env: Env` | `Option<Address>` | None |
| `get_token` | `env: Env` | `Option<Address>` | None |
| `get_plan` | `env: Env, plan_id: Symbol` | `Option<Plan>` | None |
| `get_grace_period` | `env: Env` | `u32` | None |

The subscriber must pre-approve this contract as a token spender (`token.approve(subscriber, subscription_contract, amount * periods, expiry_ledger)`) before the provider can `charge`. An optional `trial_ledgers` on `subscribe` delays the first real charge; `charge` during the trial window only marks the trial complete without transferring funds.

**Metered billing:** plans with a non-zero `unit_price` bill `amount + usage_units * unit_price` per interval. The provider accumulates usage with `report_usage`; the meter resets to zero on each successful `charge` (and is discarded on `cancel`).

**Prepaid billing:** passing `prepay_intervals: Some(n)` to `subscribe` transfers `amount * n` less the plan's `prepay_discount_bps` into contract escrow. Each `charge` releases one interval's share to the provider (usage fees are still pulled from the allowance). `cancel` refunds the unconsumed escrow balance.
Subscriptions are keyed by `(subscriber, plan_id)`, so one address can hold several plans concurrently, each billed on its own interval.

**Grace period:** a failed `charge` (insufficient allowance or balance) does not revert; it returns `ChargeOutcome::PaymentFailed`, increments `failed_charges_count`, records `delinquent_since_ledger`, and emits `charge_failed`. A failed attempt at or after `delinquent_since_ledger + grace_period_ledgers` sets the subscription to suspended (`ChargeOutcome::Suspended`, `subscription_suspended` event). A successful charge clears the delinquency. The grace period defaults to 120 960 ledgers (~7 days).

**Batch billing:** `charge_batch` charges many `(subscriber, plan_id)` pairs under one provider authorization. Entries that are not due or not chargeable are skipped without reverting the others, and a single `batch_charged` event carries the totals.

**Plan changes:** `change_plan` credits the unused part of the current paid interval (`old_amount * remaining / old_interval`). On an upgrade the shortfall is charged immediately and a new interval starts; on a downgrade the credit is turned into extra ledgers on the new plan before the next charge is due.

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
- `SubscriptionSuspended` (14) — Suspended after the grace period expired
- `PaymentOverdue` (15) — Current period is due or delinquent; charge before changing plans
- `ArithmeticOverflow` (16) — Pro-ration arithmetic overflowed

---

## Swap Contract

**Location:** `contracts/swap/src/lib.rs`

| Function | Parameters | Returns | Errors |
|----------|-----------|---------|--------|
| `initialize` | `env: Env, admin: Address, treasury: Address, fee_bps: u32` | `Result<(), SwapError>` | `AlreadyInitialized`, `InvalidFee` |
| `set_treasury` | `env: Env, new_treasury: Address` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized` |
| `set_fee_bps` | `env: Env, new_fee_bps: u32` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized`, `InvalidFee` |
| `set_admin` | `env: Env, new_admin: Address` | `Result<(), SwapError>` | `NotInitialized`, `NotAuthorized` |
| `get_admin` | `env: Env` | `Result<Address, SwapError>` | `NotInitialized` |
| `get_treasury` | `env: Env` | `Result<Address, SwapError>` | `NotInitialized` |
| `get_fee_bps` | `env: Env` | `Result<u32, SwapError>` | `NotInitialized` |
| `swap_count` | `env: Env` | `Result<u32, SwapError>` | `NotInitialized` |
| `propose_swap` | `env: Env, party_a: Address, token_a: Address, amount_a: i128, token_b: Address, amount_b: i128, expires_at: u32` | `Result<u32, SwapError>` | `NotInitialized`, `InvalidDeadline` |
| `accept_swap` | `env: Env, swap_id: u32, party_b: Address` | `Result<u32, SwapError>` | `NotInitialized`, `SwapNotFound`, `InvalidState`, `DeadlineExpired` |
| `cancel_swap` | `env: Env, swap_id: u32` | `Result<(), SwapError>` | `SwapNotFound`, `InvalidState`, `NotAuthorized` |
| `get_swap` | `env: Env, swap_id: u32` | `Result<SwapInfo, SwapError>` | `SwapNotFound` |

`propose_swap` escrows `token_a` and writes the complete `SwapInfo` under the persistent composite key `DataKey::Swap(swap_id)`. The instance entry contains only configuration and the monotonic counter; every mutation and read of a swap bumps that swap's persistent TTL.

`accept_swap` deducts a `fee_bps` fee (paid to the configured treasury) from `token_b`'s transfer to party A, then executes both legs of the swap atomically. `cancel_swap` returns the escrowed `token_a` to party A.

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
