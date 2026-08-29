# Threat Model

This document catalogues, per contract, who is trusted, what each trusted role can and cannot do, and the assets or actions at risk if that role's key is compromised. It complements [Security Best Practices](security.md), which covers re-entrancy, state-machine, and overflow analysis, and [Known Issues](known-issues.md), which covers accepted design tradeoffs.

**How to read the tables below**: "Role" is an address category established at initialization or by prior calls (not necessarily a single fixed key — e.g. a multisig contract can act as `admin`). "Compromise blast radius" describes the worst outcome if an attacker obtains signing authority for that role, assuming all other roles remain honest.

## Airdrop

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `set_root` (replace merkle root at any time, no lock) | Claim on behalf of a recipient without a matching proof | Full drain: attacker publishes a new root whose proofs they control, then claims the entire airdrop balance |
| Recipient | `claim` / `claim_batch` for entries proven against the current root | Claim more than their allotted amount, or claim without a valid proof | Limited to that recipient's own allotment |

## Auction

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Seller | `start`, `cancel` (only before any bid) | Cancel after a bid lands, alter bids | Low — cannot touch bidder funds; at most disrupts their own auction pre-bid |
| Bidder | `bid`, `withdraw` (own outbid funds) | Withdraw another bidder's funds | Limited to that bidder's own pending/escrowed bid amount |
| Anyone | `end()` (permissionless settlement) | — | None — settlement logic is fixed regardless of caller |

## Ballot

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `register_voter`, `deregister_voter`, `tally` | Vote on a registered voter's behalf | Governance integrity: attacker can register sybil voters and force a tally at a chosen time, controlling the outcome |
| Voter | `vote` (once, if registered) | Vote twice, vote unregistered | None beyond that voter's own single vote |

## Bonding Curve

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize` only (sets token address); no further privileged calls after setup | Re-initialize, pause, withdraw reserves | Minimal — admin key has no standing power once initialized |
| Buyer/Seller | `buy`, `sell` against the curve, bounded by their own `max_cost` / `min_proceeds` | Affect another trader's transaction | Limited to that trader's own transaction |

## Crowdfund

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Creator | `initialize`, `extend_deadline` (single use), `claim` (sweeps entire pool once goal is met) | Claim before the goal is met, extend the deadline more than once | Full drain of raised funds once the goal is met — `claim()` sends the whole pledged pool to the (attacker-controlled) creator address |
| Pledger | `pledge`, `withdraw` / `refund` (own pledge, if goal not met or before campaign end) | Withdraw another pledger's contribution | Limited to that pledger's own pledge |

## DAO

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `cancel_proposal` (only while the proposal is `Active`, i.e. within its voting window) | Vote on others' behalf, force a proposal to pass; cancel or reverse an already-`Executed` or already-`Cancelled` proposal | Governance availability: attacker can veto any proposal that is still being voted on, but cannot retroactively cancel or reverse a proposal that has already executed. This bounds the blast radius to the active voting window — a censorship/liveness risk, not a direct fund-theft risk |
| Proposer/Voter | `create_proposal`, `vote` (weight = live token balance) | Vote more than once per proposal | Vote-weight manipulation is possible via flash-loan-style balance changes (see [Known Issues](known-issues.md#dao)), not role-key compromise |

## Escrow

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Buyer | `fund`, `approve_delivery`, `request_refund` / `request_partial_refund`, `cancel` (pre-fund), co-signs `extend_deadline` | Release funds unilaterally to itself, bypass seller/arbiter checks | Buyer can, at most, redirect its own escrowed refund path (e.g. force a dispute); cannot seize seller-bound funds |
| Seller | `mark_delivered`, co-signs `extend_deadline` | Force fund release without buyer approval or arbiter resolution | Cannot unilaterally move funds; can only claim delivery, which the state machine still gates behind buyer approval or dispute resolution |
| Arbiter(s) | `resolve_dispute` (release to either party) once a dispute is raised | Resolve a dispute that hasn't been raised, bypass the state machine | Full escrowed `amount` for that agreement can be sent to either party — direct fund theft up to the escrow amount, contained to that single escrow instance |
| Admin (feature: `pausable`) | `pause`, `unpause`, `propose_upgrade`, `execute_upgrade` (after a ~1-day timelock) | Skip the upgrade timelock | Full logic takeover via WASM replacement, but delayed by the upgrade timelock, giving parties a window to react |

See [ADR-0003](adr/0003-admin-model.md), [ADR-0004](adr/0004-escrow-state-machine.md), and [ADR-0006](adr/0006-escrow-arbiter-model.md) for the design rationale behind this role split.

## Lottery

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `commit` (hash of self-chosen secret+salt), `draw` (reveal + pay winner) | Change ticket purchases after the fact | Severe: the admin *chooses* the secret before committing, so a dishonest admin can grind candidate secrets off-chain and commit only the one that produces a favorable winner — see [Known Issues](known-issues.md#lottery). This is a property of the design, not merely a key-compromise scenario |
| Buyer | `buy_ticket` (pre-commit), `claim_refund` (if admin never draws) | Influence winner selection | Limited to that buyer's own ticket purchase/refund |

## Marketplace

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize` only (payment token, royalty config); no further privileged calls | Change royalty settings after init, seize listings | Minimal — no standing power post-init |
| Seller | `list` / `list_with_expiry`, `cancel`, `sweep_expired`, `accept_offer` | Affect another seller's listings | Limited to that seller's own listings and any offers made on them |
| Buyer | `buy`, `make_offer`, `cancel_offer` | Affect another buyer's offers | Limited to that buyer's own purchase/offer funds |

## Multisig

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Signer (below threshold) | `propose_transaction`, `sign_transaction` | Execute without threshold approvals | A single compromised signer below threshold cannot act alone |
| Signer quorum (≥ threshold) | `execute_transaction` → invoke any target contract/function with chosen args; `add_signer` / `remove_signer` | Bypass the threshold itself | Full arbitrary-call capability: a colluding quorum can drain any asset the multisig controls elsewhere, or dilute/lock out other signers by changing the signer set |
| Anyone | `execute_transaction` once threshold is met (execution itself is permissionless) | Change proposal content | None beyond triggering an already-approved call |

## NFT

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `mint` (up to `max_supply` if set) | Transfer, burn, or force-move tokens owned by others | Attacker can mint arbitrary new tokens (diluting/counterfeiting the collection) but cannot seize already-minted tokens — no admin forced-transfer or admin-burn function exists |
| Owner | `transfer`, `batch_transfer`, `burn`, `approve` (their own tokens) | Move tokens they don't own | Limited to that owner's own token set |
| Approved spender | `transfer_from` for the single token they were approved on | Act on any other token | Limited to the one approved token |

## Oracle

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `update_price` (single-source push), `set_publishers` (replace publisher allow-list, no timelock) | Force a specific consumer contract to trust a given price feed | Any downstream consumer using `get_price` / `get_price_checked` can be fed an arbitrary price until staleness rejects it — the scope of harm depends entirely on what consumer contracts do with that price |
| Publisher (if configured) | `submit_price` | Override the admin's single-source `get_price` value | Can skew `get_median_price` alongside other publishers, mitigated by requiring collusion across the publisher set |

## Staking

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize`, `add_rewards` | Withdraw stakers' principal, withdraw accrued rewards, pause staking | Minimal — worst case is refusing to top up future rewards; principal and accrued rewards are not admin-reachable |
| Staker | `stake`, `unstake`, `claim_rewards`, `compound`, `set_compounding` | Affect another staker's principal or rewards | Limited to that staker's own principal and accrued rewards |

## Subscription

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Provider | `initialize`, `charge(subscriber)` once per elapsed interval, bounded by the subscriber's remaining token allowance | Charge beyond the subscriber's approved allowance | Can accelerate draining a subscriber's pre-approved allowance the moment each interval elapses, but is hard-capped by that allowance |
| Subscriber | `subscribe`, `cancel` | Prevent an already-elapsed, pre-approved charge | Limited to that subscriber's own allowance exposure |

## Swap

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Party A | `propose_swap` (deposits `token_a` up front), `cancel_swap` (pre-deadline) | Access party B's `token_b` before acceptance | Limited to that party's own deposited `token_a` |
| Party B | `accept_swap` | Alter the proposed terms | Limited to that party's own funds committed at acceptance |
| Anyone | `cancel_swap` after deadline (permissionless, returns party A's funds) | Redirect funds to themselves | None — permissionless cancellation only returns funds to their original owner |

## Timelock

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize` (deposits funds), `cancel` (pre-release, reclaims full locked amount) | Cancel after `release_ledger` has passed and someone has called `release` | Full amount can be redirected back to the (attacker-controlled) admin at any point before release — a complete rug-pull of the beneficiary's locked funds |
| Anyone | `release()` once `release_ledger` is reached (permissionless) | Release early | None — release timing and destination are fixed |

## Token

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `mint`, `batch_mint`, `admin_burn`, admin transfer functions, plus feature-gated `pause`/`unpause`, `freeze_account`/`unfreeze_account`, `propose_upgrade`/`execute_upgrade`, `set_transfer_hook` | Bypass an active `capped-supply` cap | The most severe role in the template set: unlimited minting (unless `capped-supply` is enabled), burning any holder's balance, freezing any account, pausing all transfers, and — with `upgradeable` enabled — full logic takeover after the upgrade timelock |
| Holder | Standard SEP-41 `transfer`, `approve`, `burn`, etc. on their own balance | Move another holder's balance | Limited to that holder's own balance and allowances granted to them |

See [ADR-0003](adr/0003-admin-model.md), [ADR-0005](adr/0005-feature-flags.md), and [ADR-0009](adr/0009-batch-mint-design.md).

## Vesting

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize` (deposits funds), `revoke` (claws back unvested remainder to itself), `admin_release` (emergency unlock, releases full balance early to the beneficiary) | Reclaim tokens already vested and claimed | `revoke` diverts all unvested tokens to the (attacker-controlled) admin; `admin_release` forces early release to the beneficiary, defeating the vesting/lockup guarantee even though funds still land with the legitimate beneficiary |
| Beneficiary | `claim` (vested-to-date amount) | Claim unvested tokens | Limited to that beneficiary's own vested balance |

## Wrapped Token

| Role | Can do | Cannot do | Compromise blast radius |
|------|--------|-----------|--------------------------|
| Admin | `initialize` only (sets `wrapped_token` / `underlying_token`); no further privileged calls | Mint/burn wrapped tokens directly | Minimal within this contract post-init; the real trust dependency is external — whichever account holds mint/burn authority on the underlying `wrapped_token` contract |
| User | `wrap`, `unwrap` (self-authorizing) | Affect another user's deposit | Limited to that user's own wrap/unwrap amount, assuming the external wrapped-token minter authority is correctly restricted to this contract |

## See Also

- [Known Issues](known-issues.md) — accepted design tradeoffs referenced above
- [Security Best Practices](security.md) — authorization table, re-entrancy, and state-machine analysis
- [Architecture](architecture.md) — admin model and contract relationships
