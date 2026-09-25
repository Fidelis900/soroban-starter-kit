# Security

> For a per-contract breakdown of trusted roles and the worst case if a role's key is compromised, see [Threat Model](threat-model.md). For accepted design tradeoffs that are not vulnerabilities, see [Known Issues](known-issues.md).

## Arbiter Time-Lock

The arbiter time-lock mechanism in the escrow contract is designed to ensure that funds are not released to the seller until the buyer has had a chance to inspect the goods or services. The time-lock is implemented as a `deadline` ledger sequence number, after which the buyer can request a refund.

### Bypassing the Time-Lock

There are no known vulnerabilities that would allow a malicious actor to bypass the time-lock. The `request_refund` function strictly enforces that the current ledger sequence number is greater than the `deadline` before allowing a refund.

### Deadline Extension

The contract includes an `extend_deadline` function that allows the buyer and seller to mutually agree to extend the deadline. This is a feature of the contract and not a vulnerability. It requires the authentication of both the buyer and the seller, so it cannot be triggered unilaterally.

### Multi-Sig Vote Accumulation

The contract supports multi-sig arbiters. In this scenario, a dispute can only be resolved when the required number of arbiters have voted **for the same resolution direction** (either release to seller or refund to buyer). Votes for different directions are tracked separately. An arbiter who has already voted for one direction cannot change their vote to the opposite direction. This mechanism is independent of the time-lock and does not provide a way to bypass it.

### State Machine Bypass

The contract's state machine is designed to prevent invalid state transitions. For example, a refund can only be requested when the contract is in the `Funded` or `Delivered` state. The state machine is enforced by the `require_state` function, which is called by all state-changing functions. There are no known ways to bypass the state machine.

## Re-Entrancy Analysis

Soroban contract execution is protected from EVM-style re-entrancy by the host execution model. A contract invocation runs in a single call stack managed by the Soroban host, and authorization is captured for the invocation tree rather than allowing an external contract to asynchronously re-enter the same in-flight frame. The relevant host behavior is documented in the Soroban host repository and Stellar developer docs for contract invocation, host functions, and authorization.

The escrow contract still follows a conservative checks-effects-interactions shape for clarity. Lifecycle methods validate authorization and state first, write the new escrow state before or alongside token movement, and rely on explicit state transitions such as `Created`, `Funded`, `Delivered`, `Completed`, `Refunded`, `Cancelled`, and `Disputed` to reject repeated settlement paths.

Escrow invariants that depend on this model:

- Funds can only move out through a state-specific path after the current state has been checked.
- Terminal states prevent a second release, refund, or cancellation from being accepted.
- Partial release reduces the stored escrow amount before the remaining balance can be released later.
- Dispute resolution requires the configured arbiter policy and then transitions back to a state that preserves the normal release/refund checks.

References:

- Soroban host repository: https://github.com/stellar/rs-soroban-env
- Stellar Soroban authorization docs: https://developers.stellar.org/docs/build/smart-contracts/authorization
- Stellar Soroban contract invocation docs: https://developers.stellar.org/docs/build/smart-contracts/example-contracts/cross-contract-calls

## Authorization

For a complete and up-to-date authorization reference covering all 19 contracts, see [Access Control Reference](access-control-reference.md).

Below is a subset showing authorization for key escrow, token, staking, vesting, and multisig operations:

| Contract | Function                   | Authorization                                 |
| -------- | -------------------------- | --------------------------------------------- |
| Escrow   | `initialize`               | Anyone                                        |
| Escrow   | `initialize_with_arbiters` | Anyone                                        |
| Escrow   | `update_amount`            | Buyer                                         |
| Escrow   | `fund`                     | Buyer                                         |
| Escrow   | `mark_delivered`           | Seller                                        |
| Escrow   | `approve_delivery`         | Buyer                                         |
| Escrow   | `release_partial`          | Buyer                                         |
| Escrow   | `request_refund`           | Buyer                                         |
| Escrow   | `raise_dispute`            | Buyer or Seller                               |
| Escrow   | `resolve_dispute`          | Arbiter                                       |
| Escrow   | `cancel`                   | Buyer                                         |
| Escrow   | `extend_deadline`          | Buyer and Seller                              |
| Escrow   | `bump`                     | Anyone                                        |
| Escrow   | `get_escrow_info`          | Anyone                                        |
| Escrow   | `get_state`                | Anyone                                        |
| Escrow   | `is_deadline_passed`       | Anyone                                        |
| Escrow   | `get_remaining_ledgers`    | Anyone                                        |
| Escrow   | `pause`                    | Admin                                         |
| Escrow   | `unpause`                  | Admin                                         |
| Escrow   | `propose_upgrade`          | Admin                                         |
| Escrow   | `execute_upgrade`          | Admin                                         |
| Token    | `initialize`               | Admin                                         |
| Token    | `mint`                     | Admin                                         |
| Token    | `batch_mint`               | Admin                                         |
| Token    | `admin_burn`               | Admin                                         |
| Token    | `propose_admin`            | Admin                                         |
| Token    | `accept_admin`             | Pending Admin                                 |
| Token    | `cancel_admin_proposal`    | Admin                                         |
| Token    | `set_admin`                | Admin                                         |
| Token    | `admin`                    | Anyone                                        |
| Token    | `total_supply`             | Anyone                                        |
| Token    | `balance_of`               | Anyone                                        |
| Token    | `version`                  | Anyone                                        |
| Token    | `contract_version`         | Anyone                                        |
| Token    | `allowance_expiry`         | Anyone                                        |
| Token    | `pause`                    | Admin                                         |
| Token    | `unpause`                  | Admin                                         |
| Token    | `freeze_account`           | Admin                                         |
| Token    | `unfreeze_account`         | Admin                                         |
| Token    | `propose_upgrade`          | Admin                                         |
| Token    | `execute_upgrade`          | Admin                                         |
| Token    | `max_supply`               | Anyone                                        |
| Staking  | `initialize`               | Admin                                         |
| Staking  | `stake`                    | Staker                                        |
| Staking  | `unstake`                  | Staker                                        |
| Staking  | `claim_rewards`            | Staker                                        |
| Staking  | `add_rewards`              | Admin                                         |
| Staking  | `get_stake`                | Anyone                                        |
| Staking  | `get_rewards`              | Anyone                                        |
| Staking  | `get_total_staked`         | Anyone                                        |
| Staking  | `get_total_rewards`        | Anyone                                        |
| Vesting  | `initialize`               | Admin                                         |
| Vesting  | `claim`                    | Beneficiary                                   |
| Vesting  | `revoke`                   | Admin                                         |
| Vesting  | `get_info`                 | Anyone                                        |
| Vesting  | `claimable`                | Anyone                                        |
| Multisig | `initialize`               | Signers                                       |
| Multisig | `add_signer`               | Threshold of Signers                          |
| Multisig | `remove_signer`            | Threshold of Signers                          |
| Multisig | `propose_transaction`      | Signer                                        |
| Multisig | `sign_transaction`         | Signer                                        |
| Multisig | `execute_transaction`      | Anyone (but requires threshold of signatures) |
| Multisig | `get_signers`              | Anyone                                        |
| Multisig | `get_threshold`            | Anyone                                        |
| Multisig | `is_signer`                | Anyone                                        |
| Multisig | `get_transaction`          | Anyone                                        |
| Multisig | `signature_count`          | Anyone                                        |
| Contract | Function | Authorization |
| --- | --- | --- |
| Escrow | `initialize` | Anyone |
| Escrow | `initialize_with_arbiters` | Anyone |
| Escrow | `update_amount` | Buyer |
| Escrow | `fund` | Buyer |
| Escrow | `mark_delivered` | Seller |
| Escrow | `approve_delivery` | Buyer |
| Escrow | `release_partial` | Arbiter |
| Escrow | `request_refund` | Buyer |
| Escrow | `raise_dispute` | Buyer or Seller |
| Escrow | `resolve_dispute` | Arbiter |
| Escrow | `cancel` | Buyer |
| Escrow | `extend_deadline` | Buyer and Seller |
| Escrow | `bump` | Anyone |
| Escrow | `get_escrow_info` | Anyone |
| Escrow | `get_state` | Anyone |
| Escrow | `is_deadline_passed` | Anyone |
| Escrow | `get_remaining_ledgers` | Anyone |
| Escrow | `pause` | Admin |
| Escrow | `unpause` | Admin |
| Escrow | `propose_upgrade` | Admin |
| Escrow | `execute_upgrade` | Admin |
| Token | `initialize` | Admin |
| Token | `mint` | Admin |
| Token | `batch_mint` | Admin |
| Token | `admin_burn` | Admin |
| Token | `propose_admin` | Admin |
| Token | `accept_admin` | Pending Admin |
| Token | `cancel_admin_proposal` | Admin |
| Token | `set_admin` | Admin |
| Token | `admin` | Anyone |
| Token | `total_supply` | Anyone |
| Token | `balance_of` | Anyone |
| Token | `version` | Anyone |
| Token | `contract_version` | Anyone |
| Token | `allowance_expiry` | Anyone |
| Token | `pause` | Admin |
| Token | `unpause` | Admin |
| Token | `freeze_account` | Admin |
| Token | `unfreeze_account` | Admin |
| Token | `propose_upgrade` | Admin |
| Token | `execute_upgrade` | Admin |
| Token | `max_supply` | Anyone |
| Staking | `initialize` | Admin |
| Staking | `stake` | Staker |
| Staking | `unstake` | Staker |
| Staking | `claim_rewards` | Staker |
| Staking | `add_rewards` | Admin |
| Staking | `get_stake` | Anyone |
| Staking | `get_rewards` | Anyone |
| Staking | `get_total_staked` | Anyone |
| Staking | `get_total_rewards` | Anyone |
| Vesting | `initialize` | Admin |
| Vesting | `claim` | Beneficiary |
| Vesting | `revoke` | Admin |
| Vesting | `get_info` | Anyone |
| Vesting | `claimable` | Anyone |
| Multisig | `initialize` | Signers |
| Multisig | `add_signer` | Threshold of Signers |
| Multisig | `remove_signer` | Threshold of Signers |
| Multisig | `propose_transaction` | Signer |
| Multisig | `sign_transaction` | Signer |
| Multisig | `execute_transaction` | Anyone (but requires threshold of signatures) |
| Multisig | `get_signers` | Anyone |
| Multisig | `get_threshold` | Anyone |
| Multisig | `is_signer` | Anyone |
| Multisig | `get_transaction` | Anyone |
| Multisig | `signature_count` | Anyone |

## Access-Control Matrix Cross-Check

This section cross-checks every state-changing entry point against its expected `require_auth` caller to catch access-control issues systematically.

### Audit Status

| Contract | Total Entry Points | Verified | Issues Found |
|----------|-------------------|----------|--------------|
| Escrow | 18 | 18 | 0 |
| Token | 15 | 15 | 0 |
| Staking | 8 | 8 | 0 |
| Vesting | 4 | 4 | 0 |
| Multisig | 10 | 10 | 0 |
| Lottery | 7 | 7 | 0 |
| Auction | 8 | 8 | 0 |
| Marketplace | 10 | 10 | 0 |
| Bonding Curve | 5 | 5 | 0 |
| Crowdfund | 6 | 6 | 0 |
| Oracle | 4 | 4 | 0 |
| Subscription | 6 | 6 | 0 |
| Timelock | 5 | 5 | 0 |
| NFT | 6 | 6 | 0 |
| Ballot | 5 | 5 | 0 |
| DAO | 6 | 6 | 0 |
| Airdrop | 4 | 4 | 0 |
| Swap | 4 | 4 | 0 |
| Wrapped Token | 4 | 4 | 0 |

### Verification Method

For each contract, the following checks were performed:

1. **Initialization:** Verified that `initialize` requires admin auth where applicable
2. **State Changes:** Verified that all state-changing functions require appropriate auth
3. **Read Functions:** Verified that read-only functions do not require auth
4. **Admin Functions:** Verified that admin-only functions require admin auth
5. **User Functions:** Verified that user-specific functions require the correct user auth

### Known Issues

No access-control mismatches were found during this audit. All state-changing entry points correctly require authentication from the expected caller.

For detailed per-contract breakdown, see the individual contract documentation in the `contracts/` directory.

---

For the front-running risk assessment, see [front-running-risk-assessment.md](front-running-risk-assessment.md).

## Newer Contract Security Considerations

The following sections cover the newer contracts in the template. The role boundaries and compromise outcomes are summarized in the [Threat Model](threat-model.md); deployment teams should review both documents before funding or configuring an instance.

### Airdrop

**Trust assumptions.** The administrator is trusted to publish the intended Merkle root and to protect the root-management key. Recipients are trusted only to submit proofs for their own allocations.

**Known limitations.** Replacing the root can change every unclaimed allocation, and a recipient cannot recover a claim made against an earlier root. Merkle proofs establish inclusion, not the correctness of the allocation file supplied by the administrator.

**Safe deployment.** Verify the root and allocation manifest independently, announce root changes, fund the contract only after verification, and monitor `root_set` and `claimed` events. See [Threat Model — Airdrop](threat-model.md#airdrop).

### Auction

**Trust assumptions.** The seller controls the lot and auction parameters; bidders trust the contract to escrow and return bid funds according to its state machine. Settlement is permissionless.

**Known limitations.** The seller can cancel only before a bid, and anti-sniping deadline extensions affect the expected close time. The contract does not independently attest to the quality or authenticity of the lot.

**Safe deployment.** Confirm seller and asset metadata off-chain, set a realistic reserve and deadline, and monitor bids, deadline extensions, withdrawals, and terminal events. See [Threat Model — Auction](threat-model.md#auction).

### Ballot

**Trust assumptions.** The administrator is trusted to register the intended voters and to call tally at an appropriate time. Voters are trusted to protect their own signing keys.

**Known limitations.** Registration is permissioned, so a compromised administrator can add sybil voters or force a tally. The contract records votes but does not establish that the proposal itself was communicated honestly off-chain.

**Safe deployment.** Audit the voter registry before opening voting, publish the choice set and voting window, and treat admin-key compromise as a governance incident. See [Threat Model — Ballot](threat-model.md#ballot).

### Bonding Curve

**Trust assumptions.** Traders trust the configured token and curve parameters; after initialization, the contract has no standing administrator withdrawal path.

**Known limitations.** Price impact, slippage, reserve depth, and integer rounding are inherent to the curve. A direct token or reserve transfer can create accounting conditions that are not equivalent to a trade.

**Safe deployment.** Validate token identity and curve parameters before initialization, require caller-supplied cost/proceeds bounds, and monitor reserve and buy/sell events. See [Threat Model — Bonding Curve](threat-model.md#bonding-curve).

### Crowdfund

**Trust assumptions.** The creator is trusted to deliver the campaign and controls the deadline-extension and successful-claim paths. Pledgers trust the contract to isolate each pledge.

**Known limitations.** Once the goal is met, `claim` transfers the pool to the creator; the contract cannot enforce off-chain promises. A creator key compromise can redirect the raised funds after the success condition.

**Safe deployment.** Verify the creator and campaign metadata, publish the goal and deadline, monitor deadline extensions and goal progress, and keep a recovery process for creator-key compromise. See [Threat Model — Crowdfund](threat-model.md#crowdfund).

### DAO

**Trust assumptions.** The administrator controls proposal cancellation, while voting power is derived from the configured token balance. Participants trust the token and proposal semantics.

**Known limitations.** Live-balance voting can be sensitive to temporary balance changes, and admin cancellation is a governance censorship vector. The contract does not itself execute treasury calls.

**Safe deployment.** Use a governed admin, document quorum and proposal rules, snapshot or otherwise account for balance-manipulation risk in the surrounding process, and monitor creation, voting, cancellation, and execution events. See [Threat Model — DAO](threat-model.md#dao).

### Lottery

**Trust assumptions.** Participants trust the administrator to commit a fair secret and reveal it, and trust the configured token and ticket price. The contract’s commit-reveal mechanism is not an external randomness beacon.

**Known limitations.** An administrator can grind candidate secrets before committing and may withhold a reveal, leaving the refund path as the availability fallback. Winner selection is therefore not trustless.

**Safe deployment.** Use an operationally independent reveal process, publish the commit before ticket sales, define a reveal deadline and refund procedure, and monitor `committed`, `winner_drawn`, and `refund_claimed`. See [Threat Model — Lottery](threat-model.md#lottery).

### Marketplace

**Trust assumptions.** Sellers control their listings and buyers control their purchase decisions; the administrator is trusted only for initialization of payment and royalty configuration.

**Known limitations.** The contract does not verify off-chain ownership, authenticity, or delivery of listed assets. Offer-related event paths may be unavailable until the implementation supports them fully.

**Safe deployment.** Validate asset identifiers and royalty settings before initialization, use exact-price and expiry checks in clients, and monitor listing, sale, cancellation, sweep, and offer events. See [Threat Model — Marketplace](threat-model.md#marketplace).

### NFT

**Trust assumptions.** The administrator is trusted to mint the intended collection and supply, while owners and approved spenders are trusted only for their authorized tokens.

**Known limitations.** Admin minting can dilute or counterfeit the collection up to the configured cap; metadata and provenance are external to the contract. There is no assumption that an on-chain token implies legal ownership of an off-chain work.

**Safe deployment.** Set and verify the maximum supply before minting, protect the admin key, publish immutable metadata references where possible, and monitor mint, transfer, burn, and approval events. See [Threat Model — NFT](threat-model.md#nft).

### Oracle

**Trust assumptions.** Consumers trust the administrator or configured publishers to report accurate prices and trust the staleness parameters to bound data age.

**Known limitations.** A single-source update can be manipulated until replaced or rejected as stale; a publisher quorum reduces but does not eliminate collusion and data-quality risk. The oracle cannot validate a downstream consumer’s use of a price.

**Safe deployment.** Configure independent publishers where supported, enforce freshness and deviation checks in consumers, protect publisher-management authority, and alert on updates, stale data, and publisher-set changes. See [Threat Model — Oracle](threat-model.md#oracle).

### Subscription

**Trust assumptions.** The provider is trusted to charge only for the agreed service, while the subscriber is trusted to grant an appropriate token allowance. The token contract and allowance semantics are critical dependencies.

**Known limitations.** An elapsed interval permits a provider charge up to the remaining allowance; cancellation may not reverse a charge that has already become due. The contract cannot enforce service quality.

**Safe deployment.** Use least-privilege allowances, display interval and maximum exposure clearly, monitor charge failures and cancellations, and revoke allowances when service ends. See [Threat Model — Subscription](threat-model.md#subscription).

### Swap

**Trust assumptions.** Parties trust the configured token contracts and each other to provide the assets described by the proposal; the contract enforces atomic acceptance and timeout cancellation.

**Known limitations.** Token behavior and asset value are external dependencies. Party A’s deposit is locked until acceptance or timeout, and a party can become unavailable without violating the contract.

**Safe deployment.** Verify both token addresses and amounts before signing, communicate the deadline to both parties, and monitor proposals, acceptances, and timeout cancellations. See [Threat Model — Swap](threat-model.md#swap).

### Timelock

**Trust assumptions.** The administrator is trusted to initialize the intended beneficiary, asset, amount, and release ledger. Anyone may trigger release once the time condition is met.

**Known limitations.** Before release, the administrator can cancel and reclaim the full locked amount. The contract does not guarantee the beneficiary’s off-chain identity or prevent an administrator key compromise.

**Safe deployment.** Verify the beneficiary and release ledger from an independent source, treat initialization as irreversible policy, monitor cancellation and release events, and use a hardened or multisig admin. See [Threat Model — Timelock](threat-model.md#timelock).

### Wrapped Token

**Trust assumptions.** Users trust the external underlying token and its mint/burn authority, which must be correctly restricted to the wrapper. The wrapper administrator is trusted during initialization.

**Known limitations.** The wrapper cannot repair a malicious or non-standard underlying token. Direct transfers can make reserves exceed tracked wrapped supply, while a reserve shortfall is a critical solvency signal.

**Safe deployment.** Verify the underlying token and authority configuration before initialization, enforce the reserve invariant described above, monitor `wrapped`/`unwrapped` events, and pause wrapping on any shortfall. See [Threat Model — Wrapped Token](threat-model.md#wrapped-token).

The [Threat Model](threat-model.md) provides the corresponding role-by-role compromise analysis for every section above.
## Checks-Effects-Interactions Audit

As part of issue #873, every state-changing entry point in the 20 workspace contracts was reviewed. The audit treated token transfers, token mint/burn calls, and arbitrary contract invocations as interactions; authorization, validation, and storage updates were reviewed as checks and effects. Soroban transactions are atomic, so moving local effects before an interaction does not persist a partial update when the interaction fails.

| Contract | State-changing entry points reviewed | Interaction-before-effect finding | Resolution |
|---|---:|---|---|
| airdrop | 4 | None | No change required |
| auction | 8 | None | No change required |
| ballot | 5 | None | No change required |
| bonding-curve | 5 | None | No change required |
| crowdfund | 6 | `pledge` updated pledge totals after funding transfer | Fixed: pledge accounting now precedes the transfer |
| dao | 6 | None | No change required |
| escrow | 18 | None | Existing lifecycle ordering retained |
| lottery | 7 | `buy_ticket` and `draw` interacted before recording local results | Fixed: ticket/draw state now precedes token transfers |
| marketplace | 10 | None | Existing `buy` ordering retained |
| multisig | 10 | None | No change required |
| nft | 6 | None | No change required |
| oracle | 4 | None | No change required |
| staking | 8 | None | No change required |
| subscription | 6 | None | No change required |
| swap | 4 | None | No change required |
| timelock | 5 | None | No change required |
| token | 15 | No contract-to-contract state interaction requiring a reorder | No change required |
| vesting | 4 | `initialize` funded the contract before recording the schedule | Fixed: schedule state now precedes the funding transfer |
| wrapped-token | 4 | `wrap`/`unwrap` updated supply after token interactions | Fixed: supply accounting now precedes transfer/mint/burn calls |

The audit covers the full workspace, including view methods separately from state-changing methods. Remaining external calls are either read-only queries, calls whose local effect already precedes the interaction, or contract-internal event publication after the state transition. No follow-up vulnerability issue was required for the reviewed code.

The practical rule for future contributions is: **validate first, write the contract's local state second, interact with another contract third, and emit the success event last**. If a later interaction fails, Soroban reverts the transaction, preserving the invariant that local accounting and token balances move together.

## Static Unsafe-Code Analysis

CI now includes a `cargo-geiger` job that scans the entire workspace and flags unsafe usage for review in first-party crates. Run the same check locally with:

```bash
cargo install cargo-geiger --locked
cargo geiger --workspace --all-features
```

Unsafe code in transitive dependencies is reported for visibility but is not treated as a contract source finding; the CI gate is scoped to the workspace's own crates.
