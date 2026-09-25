# Error Reference

Comprehensive reference for all error codes returned by the contracts in this repository.

---

## Token Contract — `TokenError`

### `InsufficientBalance` (code 1)

**Description:** The caller's token balance is too low to complete the requested transfer or burn.

**Common cause:** Attempting to transfer or burn more tokens than the account currently holds.

**Resolution:** Check the account balance with `balance()` before calling `transfer` or `burn`. Ensure the amount does not exceed the available balance.

---

### `InsufficientAllowance` (code 2)

**Description:** The approved allowance is too low for the requested `transfer_from` amount.

**Common cause:** The spender is trying to move more tokens than the owner approved via `approve`.

**Resolution:** Call `allowance()` to verify the current approved amount. Ask the token owner to call `approve` with a sufficient amount before retrying.

---

### `Unauthorized` (code 3)

**Description:** The caller is not the admin or does not have permission for this operation.

**Common cause:** Calling `mint`, `set_admin`, or other admin-only functions from a non-admin address.

**Resolution:** Ensure the transaction is signed by the current admin address. Use `admin()` to confirm who holds admin rights.

---

### `AlreadyInitialized` (code 4)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment. Check contract state before calling it.

---

### `NotInitialized` (code 5)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Invoking any contract function before `initialize` has been called.

**Resolution:** Call `initialize` with the required parameters (admin, name, symbol, decimals) before any other interaction.

---

### `InvalidAmount` (code 6)

**Description:** The amount is zero, negative, or exceeds the configured max supply.

**Common cause:** Passing `0` or a negative value as an amount, or minting beyond the cap set at initialization.

**Resolution:** Validate that the amount is a positive integer within the allowed range. Check the max supply with `max_supply()` if applicable.

---

### `Overflow` (code 7)

**Description:** Arithmetic overflow occurred during a balance or supply calculation.

**Common cause:** Minting or transferring an amount that would push a balance or the total supply past `i128::MAX`.

**Resolution:** Ensure amounts are within safe bounds. This error should not occur under normal usage; if it does, it indicates a logic error in the calling contract.

---

### `InvalidNonce` (code 8)

**Description:** The nonce supplied to `approve_with_signature` does not match the owner's current expected nonce.

**Common cause:** Replaying an already-consumed permit message, or submitting permits out of order.

**Resolution:** Call `permit_nonce(owner)` to fetch the current nonce and have the owner sign a fresh message with it.

---

### `PermitExpired` (code 9)

**Description:** The current ledger is already past the `expiry_ledger` in the signed permit message.

**Common cause:** Submitting a permit too long after it was signed.

**Resolution:** Have the owner sign a new permit with a later `expiry_ledger`.

---

### `PermitSignerNotSet` (code 10)

**Description:** `approve_with_signature` was called for an `owner` who has never registered a permit signing key.

**Common cause:** Skipping the one-time `set_permit_signer` call before attempting signature-based approvals.

**Resolution:** Have the owner call `set_permit_signer` with their ed25519 public key before any spender submits a permit on their behalf.

---

## Escrow Contract — `EscrowError`

### `NotAuthorized` (code 1)

**Description:** The caller is not permitted to invoke this function.

**Common cause:** A party calling a function reserved for another role — e.g., the buyer calling `mark_delivered`, or a non-arbiter calling `resolve_dispute`.

**Resolution:** Verify which address is allowed to call each function. Refer to the contract docs for role requirements per function.

---

### `InvalidState` (code 2)

**Description:** The escrow is not in the required lifecycle state for this operation.

**Common cause:** Calling `fund` on an already-funded escrow, or calling `approve_delivery` before the escrow has been marked as delivered.

**Resolution:** Check the current escrow state with `get_state()` before calling state-dependent functions. Follow the expected lifecycle: `Created → Funded → Delivered → Completed`.

---

### `DeadlinePassed` (code 3)

**Description:** The escrow deadline has already elapsed; the operation is no longer valid.

**Common cause:** Attempting to fund or interact with an escrow after its deadline ledger has passed.

**Resolution:** Check `is_deadline_passed()` before interacting. If the deadline has passed, only refund-related flows are available.

---

### `DeadlineNotReached` (code 4)

**Description:** The deadline has not yet passed; a premature refund or timeout claim was attempted.

**Common cause:** Calling `request_refund` before the escrow deadline has been reached.

**Resolution:** Wait until the deadline ledger has passed before requesting a timeout-based refund. Use `is_deadline_passed()` to check.

---

### `AlreadyInitialized` (code 5)

**Description:** `initialize` was called on an escrow that is already set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** Only call `initialize` once, right after deployment. Guard against re-initialization in your integration code.

---

### `NotInitialized` (code 6)

**Description:** An operation was attempted before the escrow was initialized.

**Common cause:** Calling any escrow function before `initialize` has been invoked.

**Resolution:** Always call `initialize` first with buyer, seller, arbiter, token, amount, and deadline parameters.

---

### `InsufficientFunds` (code 7)

**Description:** The buyer's token balance is too low to cover the escrowed amount.

**Common cause:** Calling `fund` when the buyer does not hold enough tokens, or has not approved the escrow contract to spend on their behalf.

**Resolution:** Ensure the buyer has called `approve` on the token contract for at least the escrow amount, and that their balance is sufficient.

---

### `InvalidAmount` (code 8)

**Description:** The specified escrow amount is zero or otherwise invalid.

**Common cause:** Passing `0` as the amount during `initialize`, or calling `update_amount` with a non-positive value.

**Resolution:** Always provide a positive, non-zero amount. Validate inputs before calling the contract.

---

### `InvalidParties` (code 9)

**Description:** Buyer, seller, or arbiter addresses are invalid or conflict with each other.

**Common cause:** Passing the same address for two different roles (e.g., buyer and seller are the same), or providing an invalid address.

**Resolution:** Ensure buyer, seller, and arbiter are three distinct, valid addresses before calling `initialize`.

---

## Wrapped-Token Contract — `WrappedTokenError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Invoking `wrap`, `unwrap`, or `get_reserve_balance` before `initialize` has been called.

**Resolution:** Call `initialize` with the admin, wrapped token, underlying token, and optional per-address cap before any other interaction.

---

### `Unauthorized` (code 3)

**Description:** The caller is not permitted to perform this action, or the contract is currently paused.

**Common cause:** Calling `wrap` or `unwrap` while the contract is paused (only possible when built with the `pausable` feature).

**Resolution:** Check contract state before retrying; wait for the admin to call `unpause()`.

---

### `InvalidAmount` (code 4)

**Description:** The amount is zero or negative, or `max_wrap_per_address` was set to zero or negative at `initialize`.

**Common cause:** Passing `0` or a negative value as an amount to `wrap`/`unwrap`, or an invalid cap to `initialize`.

**Resolution:** Validate that amounts are positive integers. If setting a cap, ensure it is greater than zero (or pass `None` for uncapped).

---

### `InsufficientBalance` (code 5)

**Description:** The caller does not hold enough wrapped tokens to complete the requested `unwrap`.

**Common cause:** Attempting to unwrap more than the wrapped token balance held by the caller.

**Resolution:** Check the wrapped token balance before calling `unwrap`.

---

### `InsufficientReserve` (code 6)

**Description:** The contract does not hold enough of the underlying asset to honor an `unwrap`.

**Common cause:** The underlying reserve has been depleted relative to the wrapped supply — this should never happen under normal 1:1-backed operation; see the reserve-backing invariant in the [monitoring guide](monitoring.md).

**Resolution:** Investigate immediately; this indicates a broken invariant between `get_total_wrapped()` and `get_reserve_balance()`.

---

### `MaxWrapExceeded` (code 7)

**Description:** Wrapping this amount would push the caller's cumulative wrapped total past the `max_wrap_per_address` cap set at `initialize`.

**Common cause:** A single address attempting to wrap more than the configured concentration-risk limit, either in one call or across multiple calls.

**Resolution:** Check `wrapped_by(address)` and `max_wrap_per_address()` before calling `wrap`, and keep cumulative wraps within the cap.

---

## Airdrop Contract — `AirdropError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Calling `set_root`, `claim`, or `claim_batch` before `initialize` has been called.

**Resolution:** Call `initialize` with the admin, payment token, and claim deadline before any other interaction.

---

### `Unauthorized` (code 3)

**Description:** The caller is not permitted to perform this action.

**Common cause:** A non-admin caller invoking an admin-only entry point.

**Resolution:** Ensure the transaction is signed by the address that called `initialize`.

---

### `RootNotSet` (code 4)

**Description:** `claim` or `claim_batch` was called before the admin set a merkle root.

**Common cause:** Skipping the `set_root` call after `initialize`.

**Resolution:** Have the admin call `set_root` with the distribution's merkle root before accepting claims. Use `get_root()` to verify.

---

### `InvalidProof` (code 5)

**Description:** The supplied merkle proof does not verify against the stored root for the given `(recipient, amount)` leaf.

**Common cause:** A stale or incorrectly generated proof, or a mismatch between the claimed `amount` and the amount encoded in the distribution tree.

**Resolution:** Regenerate the proof from the canonical distribution tree for the exact `(recipient, amount)` pair, and confirm it against `get_root()`.

---

### `AlreadyClaimed` (code 6)

**Description:** The address has already successfully claimed its airdrop allocation.

**Common cause:** Calling `claim` or `claim_batch` a second time for the same recipient.

**Resolution:** Call `is_claimed(address)` before submitting a claim to avoid a redundant, failing transaction.

---

### `InvalidAmount` (code 7)

**Description:** The claim amount is zero or negative.

**Common cause:** Passing `0` (or a negative value) as the `amount` argument to `claim` or an entry in `claim_batch`.

**Resolution:** Use the exact positive amount assigned to the recipient in the distribution tree.

---

### `ClaimWindowClosed` (code 8)

**Description:** The current ledger sequence is past the `claim_deadline` set at `initialize`.

**Common cause:** Submitting a claim after the airdrop's claim window has expired.

**Resolution:** Claims must be submitted before the configured deadline ledger; there is no way to extend it post-deployment.

---

### `InsufficientBalance` (code 9)

**Description:** The contract's balance of the claimed token, minus tokens already locked in vesting schedules, is smaller than the claim (or, for `claim_batch`, the batch total for that token).

**Common cause:** The contract was not funded with every token in a multi-token tree, or was under-funded.

**Resolution:** Transfer enough of each distributed token to the airdrop contract.

---

### `InvalidVestingConfig` (code 10)

**Description:** `initial_unlock_bps` exceeds 10 000, or part of each claim would be locked while `vesting_duration_ledgers` is 0.

**Resolution:** Use `initial_unlock_bps` in `0..=10_000` and a non-zero duration whenever `initial_unlock_bps < 10_000`.

---

### `NoVestingSchedule` (code 11)

**Description:** `release` was called for a `(recipient, token)` pair that has no vesting schedule.

**Common cause:** Vesting is disabled, the pair has not claimed yet, or the whole claim was liquid.

---

### `NothingToRelease` (code 12)

**Description:** No additional tokens have vested since the last `release`.

**Resolution:** Wait for more ledgers to pass; query `releasable` first.

---

### `DuplicateEntry` (code 13)

**Description:** A `claim_batch` call listed the same `(recipient, token)` pair more than once.

---

### `ArithmeticOverflow` (code 14)

**Description:** A checked arithmetic operation overflowed while computing a claim, batch total, or vesting amount.

---

### `EmptyBatch` (code 15)

**Description:** `claim_batch` was called with no entries.

---

## Auction Contract — `AuctionError`

### `AlreadyInitialized` (code 1)

**Description:** `start` was called on an auction that has already been started.

**Common cause:** Calling `start` more than once on the same contract instance.

**Resolution:** Deploy a new contract instance per auction; `start` may only be called once.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the auction was started.

**Common cause:** Calling `bid`, `cancel`, `end`, or `get_info` before `start` has been called.

**Resolution:** Call `start` with the seller, token, pricing, and deadline parameters first.

---

### `AuctionEnded` (code 3)

**Description:** A bid was placed after the auction's deadline passed or after it was cancelled.

**Common cause:** Submitting `bid` too late, or against an auction the seller already cancelled.

**Resolution:** Check `get_info()` for the current deadline and cancellation status before bidding.

---

### `AuctionNotEnded` (code 4)

**Description:** `end` was called before the auction deadline was reached.

**Common cause:** Calling `end` prematurely, or racing anti-sniping deadline extensions triggered by late bids.

**Resolution:** Wait until the current ledger exceeds `get_info().deadline` (which may have been extended by a late bid) before calling `end`.

---

### `BidTooLow` (code 5)

**Description:** The submitted bid does not meet the minimum required amount.

**Common cause:** Bidding below `start_price` on the first bid, or below `highest_bid + min_increment` on subsequent bids.

**Resolution:** Read `get_info()` to determine the current highest bid and increment, and bid at least that much.

---

### `AlreadyEnded` (code 6)

**Description:** The auction has already been settled (or cancelled), so `end` or `cancel` cannot run again.

**Common cause:** Calling `end` a second time, or calling `cancel` after the auction was already settled or cancelled.

**Resolution:** Check `get_info().settled` before calling `end` or `cancel`.

---

### `NoBids` (code 7)

**Description:** Reserved for the no-bids code path; the contract instead emits an `ended_no_bids` event from `end()` rather than returning this error. Included for completeness with the error enum.

**Common cause:** N/A — `end()` succeeds with no transfer when no bids were placed.

**Resolution:** N/A.

---

### `NotAuthorized` (code 8)

**Description:** The caller is not permitted to perform this action.

**Common cause:** Someone other than the seller calling `cancel`.

**Resolution:** Only the address passed as `seller` to `start` may call `cancel`.

---

### `InvalidAmount` (code 9)

**Description:** `start_price`, `min_increment`, or a bid `amount` is zero or negative.

**Common cause:** Passing non-positive values to `start` or `bid`.

**Resolution:** Ensure `start_price`, `min_increment`, and every bid amount are strictly positive.

---

### `InvalidDeadline` (code 10)

**Description:** The supplied `deadline` is not in the future relative to the current ledger.

**Common cause:** Passing a `deadline` <= the current ledger sequence to `start`.

**Resolution:** Choose a `deadline` ledger sequence greater than `env.ledger().sequence()` at call time.

---

### `NothingToWithdraw` (code 11)

**Description:** The caller has no pending refund available.

**Common cause:** Calling `withdraw` without ever having been outbid, or calling it twice.

**Resolution:** Call `get_pending(address)` to check for a refund balance before calling `withdraw`.

---

### `ReserveNotMet` (code 12)

**Description:** Reserved for the reserve-price code path; the contract instead emits an `ended_reserve_not_met` event and refunds the highest bidder rather than returning this error from `end()`. Included for completeness with the error enum.

**Common cause:** N/A — `end()` succeeds (refunding the bidder) when `highest_bid < reserve_price`.

**Resolution:** N/A.

---

### `BidAlreadyPlaced` (code 13)

**Description:** `cancel` was called after at least one bid has already been placed.

**Common cause:** The seller attempting to cancel an auction that already has bidding activity.

**Resolution:** `cancel` is only valid before the first bid; once bidding starts the auction must run to `end()`.

---

## Ballot Contract — `BallotError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a ballot that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the ballot was initialized.

**Common cause:** Calling `register_voter`, `vote`, `tally`, or `tally_all` before `initialize`.

**Resolution:** Call `initialize` with the admin, voting window, and choices list first.

---

### `Unauthorized` (code 3)

**Description:** The caller is not the admin.

**Common cause:** A non-admin address calling `register_voter`, `deregister_voter`, `tally`, or `tally_all`.

**Resolution:** Ensure the transaction is signed by the address passed as `admin` to `initialize`.

---

### `NotRegistered` (code 4)

**Description:** The voter is not on the registered-voter list.

**Common cause:** Calling `vote` or `deregister_voter` for an address the admin never registered via `register_voter`.

**Resolution:** Have the admin call `register_voter` for the address before it attempts to vote.

---

### `AlreadyVoted` (code 5)

**Description:** The voter has already cast a vote.

**Common cause:** Calling `vote` a second time for the same registered voter.

**Resolution:** Each registered voter may vote exactly once; there is no re-vote mechanism.

---

### `InvalidChoice` (code 6)

**Description:** The `choice` index passed to `vote` is out of range for the configured choices list.

**Common cause:** Passing an index >= the number of choices set at `initialize`.

**Resolution:** Call `get_choices()` to see the valid indices before voting.

---

### `VotingClosed` (code 7)

**Description:** Voting is no longer active — either it was explicitly closed (by `tally`/`tally_all`) or the current ledger is past `voting_end`.

**Common cause:** Calling `vote` after the voting window has closed or after tallying has already occurred.

**Resolution:** Vote within the `[voting_start, voting_end]` window and before the admin calls `tally`/`tally_all`.

---

### `VotingAlreadyStarted` (code 8)

**Description:** `deregister_voter` was called after at least one vote has already been cast.

**Common cause:** The admin trying to correct a registration mistake after voting has begun.

**Resolution:** `deregister_voter` is only allowed while `TotalVotes == 0`; corrections must happen before the first vote.

---

### `InvalidWindow` (code 9)

**Description:** The voting window passed to `initialize` is invalid.

**Common cause:** `voting_start >= voting_end`, or `voting_end` is not in the future relative to the current ledger.

**Resolution:** Choose `voting_start < voting_end`, with `voting_end` greater than the current ledger sequence.

---

### `VotingNotStarted` (code 10)

**Description:** `vote` was called before the current ledger reached `voting_start`.

**Common cause:** Submitting a vote too early, before the configured voting window opens.

**Resolution:** Wait until the current ledger sequence reaches `voting_start` before calling `vote`.

---

### `NoChoices` (code 11)

**Description:** `initialize` was called with an empty `choices` list.

**Common cause:** Passing an empty `Vec<String>` for `choices`.

**Resolution:** Supply at least one named choice at `initialize`.

---

## Bonding Curve Contract — `BondingCurveError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Calling `buy` or `sell` before `initialize` has been called.

**Resolution:** Call `initialize` with the admin and token address before any trading.

---

### `Unauthorized` (code 3)

**Description:** The caller is not the admin. Reserved for future admin-only entry points; no current entry point returns this variant.

**Common cause:** N/A in the current implementation.

**Resolution:** N/A.

---

### `InvalidAmount` (code 4)

**Description:** The trade amount is zero, negative, exceeds a caller-supplied slippage bound, or (for `sell`) exceeds the current supply.

**Common cause:** Passing `amount <= 0` to `buy`/`sell`, a `buy` whose computed cost exceeds `max_cost`, or a `sell` whose computed proceeds fall below `min_proceeds`.

**Resolution:** Pass a positive `amount`, and set `max_cost` / `min_proceeds` slippage bounds wide enough to accommodate the current curve price (read via `get_price()`).

---

### `InsufficientReserve` (code 5)

**Description:** The contract does not hold enough reserve to pay out a `sell`'s proceeds.

**Common cause:** Selling when the computed proceeds exceed the current reserve balance — should not occur under normal curve operation.

**Resolution:** Check `get_reserve()` before selling; if this occurs it indicates a pricing/reserve invariant violation worth investigating.

---

### `Overflow` (code 6)

**Description:** Arithmetic overflow occurred while computing the curve price, buy cost, or sell proceeds.

**Common cause:** Trading an extremely large `amount` (e.g. near `i128::MAX`) that overflows the fixed-point price calculation.

**Resolution:** Trade in smaller increments; this error is a safe failure mode rather than a panic.

---

## Crowdfund Contract — `CrowdfundError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a campaign that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the campaign was initialized.

**Common cause:** Calling `pledge`, `withdraw`, `claim`, `refund`, or `get_info` before `initialize`.

**Resolution:** Call `initialize` with the creator, token, goal, deadline, and tiers first.

---

### `DeadlinePassed` (code 3)

**Description:** `pledge`, `withdraw`, or `extend_deadline` was called after the campaign deadline elapsed.

**Common cause:** Attempting to pledge or withdraw funds after the deadline ledger has passed.

**Resolution:** Pledges, withdrawals, and deadline extensions must happen before `get_info().deadline`.

---

### `DeadlineNotReached` (code 4)

**Description:** `claim` or `refund` was called before the campaign deadline was reached.

**Common cause:** The creator or a pledger attempting to settle the campaign early.

**Resolution:** Wait until the current ledger exceeds the deadline before calling `claim` or `refund`.

---

### `GoalAlreadyMet` (code 5)

**Description:** `refund` was called but the total pledged amount met or exceeded the goal.

**Common cause:** A pledger calling `refund` on a successfully funded campaign.

**Resolution:** When the goal is met, the creator calls `claim` instead; pledgers cannot refund a successful campaign.

---

### `GoalNotMet` (code 6)

**Description:** `claim` was called but the total pledged amount is below the goal.

**Common cause:** The creator attempting to claim funds from a campaign that failed to reach its goal.

**Resolution:** When the goal is not met, pledgers call `refund` individually instead.

---

### `AlreadyClaimed` (code 7)

**Description:** `claim` was called after the creator already claimed the funds once.

**Common cause:** Calling `claim` a second time.

**Resolution:** Funds can only be claimed once; check `get_info().claimed` first.

---

### `NothingToPledge` (code 8)

**Description:** Reserved for a zero-pledge guard path. Included for completeness with the error enum; `pledge` currently reports a zero/negative amount via `InvalidAmount` instead.

**Common cause:** N/A in the current implementation.

**Resolution:** N/A.

---

### `NothingToWithdraw` (code 9)

**Description:** The caller has no active pledge to withdraw or refund.

**Common cause:** Calling `withdraw` or `refund` for an address that never pledged, or that already withdrew/refunded.

**Resolution:** Call `get_pledge(address)` to confirm an active pledge exists first.

---

### `InvalidAmount` (code 10)

**Description:** The pledge amount is zero or negative, or the optional `max_pledge_per_address` cap passed to `initialize` is non-positive.

**Common cause:** Passing `amount <= 0` to `pledge`, or `Some(cap) <= 0` to `initialize`.

**Resolution:** Pledge a positive amount; pass `None` to `initialize` for an uncapped campaign or a positive cap.

---

### `InvalidDeadline` (code 11)

**Description:** The supplied deadline is invalid.

**Common cause:** `initialize` called with `deadline <= current ledger`, or `extend_deadline` called with `new_deadline <= current deadline`.

**Resolution:** Choose a deadline strictly in the future, and only extend to a strictly later ledger.

---

### `InvalidGoal` (code 12)

**Description:** `initialize` was called with a non-positive `goal`.

**Common cause:** Passing `goal <= 0`.

**Resolution:** Set a positive funding goal.

---

### `NotAuthorized` (code 13)

**Description:** The caller is not the campaign creator.

**Common cause:** Someone other than the creator calling `claim` or `extend_deadline`.

**Resolution:** Only the address passed as `creator` to `initialize` may call these entry points.

---

### `InvalidTier` (code 14)

**Description:** A funding tier passed to `initialize` has a non-positive `threshold`.

**Common cause:** Including a `FundingTier { threshold: 0, .. }` or negative threshold in the `tiers` list.

**Resolution:** Every stretch-goal tier threshold must be a positive amount.

---

### `PledgeCapExceeded` (code 15)

**Description:** A pledge would push the pledger's cumulative total above `max_pledge_per_address`.

**Common cause:** A single address pledging more than the configured per-address cap, across one or more `pledge` calls.

**Resolution:** Call `get_pledge(address)` to check the running total against `get_info().max_pledge_per_address` before pledging more.

---

### `DeadlineAlreadyExtended` (code 16)

**Description:** `extend_deadline` was called after the deadline had already been extended once.

**Common cause:** Calling `extend_deadline` a second time.

**Resolution:** The deadline may only be extended once per campaign.

---

## DAO Contract — `DaoError`

### `NotAuthorized` (code 1)

**Description:** The caller is not permitted to perform this action.

**Common cause:** A non-admin address calling `cancel_proposal`, or a non-proposer calling `proposer_cancel_proposal`.

**Resolution:** Only the DAO admin may cancel arbitrary proposals; only the original proposer may self-cancel via `proposer_cancel_proposal`.

---

### `AlreadyInitialized` (code 2)

**Description:** `initialize` was called on a DAO that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 3)

**Description:** An operation was attempted before the DAO was initialized.

**Common cause:** Calling `create_proposal`, `vote`, or `execute_proposal` before `initialize`.

**Resolution:** Call `initialize` with the admin, governance token, voting period, and quorum settings first.

---

### `ProposalNotFound` (code 4)

**Description:** The referenced `proposal_id` does not exist.

**Common cause:** Passing a stale or out-of-range `proposal_id` to `vote`, `execute_proposal`, `cancel_proposal`, or `get_proposal`.

**Resolution:** Use `proposal_count()` to confirm valid IDs, or read the ID returned by `create_proposal`.

---

### `InvalidState` (code 5)

**Description:** The proposal is not in the `Active` state required for this operation.

**Common cause:** Calling `vote`, `execute_proposal`, `cancel_proposal`, or `proposer_cancel_proposal` on a proposal that is already `Executed` or `Cancelled`.

**Resolution:** Check `get_proposal(id).state` before acting on a proposal.

---

### `DeadlineNotReached` (code 6)

**Description:** `execute_proposal` was called before the proposal's voting deadline passed.

**Common cause:** Attempting to execute a proposal while voting is still open.

**Resolution:** Wait until the current ledger exceeds `get_proposal(id).deadline` before executing.

---

### `AlreadyVoted` (code 7)

**Description:** The voter has already voted on this proposal.

**Common cause:** Calling `vote` a second time for the same `(voter, proposal_id)` pair.

**Resolution:** Each address may cast one vote per proposal; there is no vote-changing mechanism.

---

### `QuorumNotMet` (code 8)

**Description:** Total votes cast are below the required absolute `quorum`, or below the `quorum_bps` percentage of total token supply.

**Common cause:** Executing a proposal that did not attract enough participation to satisfy either configured quorum threshold.

**Resolution:** Encourage more voting before the deadline, or check the DAO's configured `quorum`/`quorum_bps` values.

---

### `ProposalRejected` (code 9)

**Description:** `no_votes >= yes_votes` on an otherwise quorum-satisfying proposal.

**Common cause:** Attempting to execute a proposal that did not receive a majority of `yes` votes.

**Resolution:** A rejected proposal cannot be executed; a new proposal must be created if desired.

---

### `InsufficientVotingPower` (code 10)

**Description:** The caller holds zero governance tokens.

**Common cause:** Calling `create_proposal` or `vote` from an address with no balance in the configured governance token.

**Resolution:** Acquire governance tokens before creating proposals or voting.

---

### `VotesAlreadyCast` (code 11)

**Description:** `proposer_cancel_proposal` was called after at least one vote has already been recorded.

**Common cause:** The proposer attempting to retract a proposal after voting has begun.

**Resolution:** Self-cancellation is only available before any vote is cast; afterward, only the admin can cancel via `cancel_proposal`.

---

### `InvalidQuorumBps` (code 12)

**Description:** `initialize` was called with `quorum_bps > 10_000`.

**Common cause:** Passing a basis-points value outside the valid `0`–`10_000` range.

**Resolution:** `quorum_bps` must be between `0` (disabled) and `10_000` (100%).

---

### `VotingClosed` (code 13)

**Description:** `vote` was called after the proposal's voting deadline passed.

**Common cause:** Submitting a vote too late.

**Resolution:** Vote before `get_proposal(id).deadline` is reached.

---

## Lottery Contract — `LotteryError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a lottery that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the lottery was initialized.

**Common cause:** Calling `buy_ticket`, `commit`, or `draw` before `initialize`.

**Resolution:** Call `initialize` with the admin, token, ticket price, and winner configuration first.

---

### `Unauthorized` (code 3)

**Description:** The caller is not the admin.

**Common cause:** A non-admin address calling `commit` or `draw`.

**Resolution:** Only the address passed as `admin` to `initialize` may commit or draw.

---

### `LotteryClosed` (code 4)

**Description:** `buy_ticket` was called while the lottery is no longer in the `Open` state.

**Common cause:** Buying a ticket after the admin has already called `commit`.

**Resolution:** Purchase tickets only while `get_info().state == Open`.

---

### `DrawAlreadyDone` (code 5)

**Description:** `commit`, `draw`, or `claim_refund` was called after the lottery already transitioned to `Drawn`.

**Common cause:** Attempting a second `draw`, or claiming a refund after winners were already selected.

**Resolution:** Once drawn, winners are final; refunds are only available before a successful `draw`.

---

### `DrawNotDone` (code 6)

**Description:** `draw` was called before `commit`, or `get_winner`/`get_winners` was called before a draw occurred.

**Common cause:** Calling `draw` while the lottery is still `Open`, or reading winners too early.

**Resolution:** Call `commit` before `draw`, and only read winner results after `draw` succeeds.

---

### `InvalidTicketPrice` (code 7)

**Description:** `initialize` was called with a non-positive `ticket_price`.

**Common cause:** Passing `ticket_price <= 0`.

**Resolution:** Set a positive ticket price.

---

### `CommitAlreadySubmitted` (code 8)

**Description:** `commit` was called while the lottery is already in the `Committed` state.

**Common cause:** Calling `commit` a second time before drawing.

**Resolution:** `commit` may only be called once per lottery, transitioning `Open → Committed`.

---

### `RevealMismatch` (code 9)

**Description:** The `secret`/`salt` pair passed to `draw` does not hash to the commitment stored by `commit`.

**Common cause:** Revealing the wrong secret/salt pair, or a bug in how the commitment was originally computed.

**Resolution:** Ensure `draw` is called with the exact `(secret, salt)` used to compute the hash passed to `commit`.

---

### `NoTickets` (code 10)

**Description:** `commit` was called with no tickets sold.

**Common cause:** Attempting to commit before any `buy_ticket` calls succeeded.

**Resolution:** Wait for at least one ticket purchase before calling `commit`.

---

### `InvalidRevealDeadline` (code 11)

**Description:** The `reveal_deadline` passed to `commit` is not in the future.

**Common cause:** Passing a `reveal_deadline <= current ledger`.

**Resolution:** Choose a `reveal_deadline` ledger sequence greater than the current one, leaving enough time to call `draw`.

---

### `RefundNotAvailable` (code 12)

**Description:** `claim_refund` was called while the lottery is still `Open`, or before the reveal deadline has passed.

**Common cause:** Requesting a refund too early — refunds are only for a `Committed` lottery whose admin missed the reveal deadline.

**Resolution:** Wait until the current ledger exceeds `reveal_deadline` (and the admin has not called `draw`) before requesting a refund.

---

### `NothingToRefund` (code 13)

**Description:** The caller holds no tickets to refund.

**Common cause:** Calling `claim_refund` for an address that never bought a ticket, or that already refunded.

**Resolution:** Call `get_ticket_count(address)` to confirm outstanding tickets before requesting a refund.

---

### `InvalidWinnerConfig` (code 14)

**Description:** `winner_count` is zero, `prize_splits.len() != winner_count`, or the splits do not sum to 10 000 basis points.

**Common cause:** Misconfigured `winner_count`/`prize_splits` arguments at `initialize`.

**Resolution:** Ensure `prize_splits` has exactly `winner_count` entries summing to `10_000`.

---

### `InsufficientParticipants` (code 15)

**Description:** Fewer distinct ticket-holding addresses exist than the configured `winner_count`.

**Common cause:** Calling `draw` when too few unique participants bought tickets to select the configured number of winners.

**Resolution:** Ensure enough distinct addresses hold tickets before drawing, or reduce `winner_count` at `initialize`.

---

### `InvalidTicketCap` (code 16)

**Description:** `initialize` was called with `max_tickets_per_address = Some(0)`.

**Common cause:** Passing a zero cap instead of `None` (uncapped) or a positive value.

**Resolution:** Pass `None` for no cap, or a positive integer cap.

---

### `TicketCapExceeded` (code 17)

**Description:** `buy_ticket` would push the buyer's ticket count above `max_tickets_per_address`.

**Common cause:** A single address attempting to buy more tickets than the configured per-address cap.

**Resolution:** Call `get_ticket_count(address)` before buying to check against the configured cap.

---

## Marketplace Contract — `MarketplaceError`

> **Note:** `contracts/marketplace/src/lib.rs` currently contains corrupted/duplicated
> code (two conflicting `get_active_listings` definitions, references to error
> variants and types that don't exist anywhere in the crate, and no function that
> creates an `Offer`). This section is cross-checked against the authoritative
> `errors.rs` enum below; see `contract-api.md` for how this affects the
> documented public API.

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a marketplace that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the marketplace was initialized.

**Common cause:** Calling `list`, `buy`, or `cancel` before `initialize`.

**Resolution:** Call `initialize` with the admin, payment token, and royalty configuration first.

---

### `NotAuthorized` (code 3)

**Description:** The caller is not permitted to perform this action.

**Common cause:** Someone other than the seller or admin calling `cancel`, or a non-seller accepting an offer.

**Resolution:** Only the listing's seller (or the admin, for `cancel`) may perform seller-restricted actions.

---

### `InvalidPrice` (code 4)

**Description:** A listing price is zero or negative.

**Common cause:** Passing `price <= 0` to `list`.

**Resolution:** List at a positive price.

---

### `ListingNotFound` (code 5)

**Description:** The referenced `listing_id` does not exist.

**Common cause:** Passing a stale or invalid `listing_id` to `buy`, `cancel`, or `get_listing`.

**Resolution:** Use the ID returned by `list`, or enumerate listings via `get_active_listings`.

---

### `ListingInactive` (code 6)

**Description:** The listing is no longer active (already bought or cancelled).

**Common cause:** Calling `buy` or `cancel` on a listing that already sold or was cancelled.

**Resolution:** Check `get_listing(id).active` before acting on it.

---

### `InvalidRoyalty` (code 7)

**Description:** The royalty basis points exceed 10 000 (100%).

**Common cause:** Passing `royalty_bps > 10_000` to `initialize`.

**Resolution:** Keep `royalty_bps` within `0`–`10_000`.

---

### `InvalidExpiry` (code 8)

**Description:** The provided listing expiry ledger sequence is not in the future.

**Common cause:** Passing an `expires_at <= current ledger` when creating a listing with an expiry.

**Resolution:** Choose an expiry ledger strictly greater than the current ledger sequence.

---

### `ListingExpired` (code 9)

**Description:** The listing's expiry ledger sequence has already passed.

**Common cause:** Calling `buy` on a listing whose optional `expires_at` has elapsed.

**Resolution:** Check the listing's `expires_at` before buying; expired listings must be swept rather than bought.

---

### `ListingNotExpired` (code 10)

**Description:** A sweep-style cleanup was called on a listing that has no expiry, or whose expiry has not yet passed.

**Common cause:** Attempting to sweep an active, non-expired listing.

**Resolution:** Only sweep listings whose `expires_at` is `Some` and in the past.

---

### `InvalidOfferAmount` (code 11)

**Description:** An offer amount is zero, negative, or not below the listing price.

**Common cause:** Proposing an offer that does not represent a genuine discount off the asking price.

**Resolution:** Offer a positive amount strictly less than the listing's `price`.

---

### `OfferNotFound` (code 12)

**Description:** No offer exists for the given `(listing_id, buyer)` pair.

**Common cause:** Calling `cancel_offer` or accepting an offer that was never made, or was already cancelled/accepted.

**Resolution:** Call `get_offer(listing_id, buyer)` to confirm an active offer exists first.

---

## Multisig Contract — `MultisigError`

### `ProposalExpired` (code 11)

**Description:** The proposal has expired and can no longer be signed or executed.

**Common cause:** Signing or executing a transaction proposal after its expiry ledger has passed.

**Resolution:** Propose a new transaction; expired proposals cannot be revived.

---

### `InvalidWeight` (code 12)

**Description:** A signer weight of zero was supplied.

**Common cause:** Adding or configuring a signer with weight `0` in a weighted-multisig setup.

**Resolution:** Every signer must have a weight of at least `1`.

> See `contract-api.md` for the full `MultisigContract` public API and the
> remaining `MultisigError` codes 1–10.

---

## NFT Contract — `NftError`

### `NotAuthorized` (code 1)

**Description:** The caller is not permitted to perform this action.

**Common cause:** A non-admin address calling `mint`, or a non-owner calling `transfer`/`approve`.

**Resolution:** Only the collection admin may `mint`; only a token's current owner may `transfer` or `approve` it.

---

### `AlreadyInitialized` (code 2)

**Description:** `initialize` was called on a collection that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 3)

**Description:** An operation was attempted before the collection was initialized.

**Common cause:** Calling `mint`, `transfer`, or `owner_of` before `initialize`.

**Resolution:** Call `initialize` with the admin, name, symbol, and optional supply/royalty settings first.

---

### `TokenNotFound` (code 4)

**Description:** The referenced `token_id` does not exist.

**Common cause:** Passing a `token_id` that was never minted to `owner_of`, `token_uri`, `transfer`, or `approve`.

**Resolution:** Use a `token_id` returned by a prior `mint` call.

---

### `TokenAlreadyMinted` (code 5)

**Description:** Reserved for a duplicate-mint guard on explicit token IDs. Included for completeness with the error enum; the current `mint` always assigns the next sequential ID, so this cannot be triggered through the public API today.

**Common cause:** N/A in the current implementation.

**Resolution:** N/A.

---

### `NotOwner` (code 6)

**Description:** The caller does not own the referenced token.

**Common cause:** An address that is not `owner_of(token_id)` attempting to transfer or approve it.

**Resolution:** Only the current owner (per `owner_of`) may transfer or approve a token.

---

### `NotApproved` (code 7)

**Description:** Reserved for approved-spender transfer checks. Included for completeness with the error enum; the current `transfer` only checks direct ownership, not an approved spender.

**Common cause:** N/A in the current implementation.

**Resolution:** N/A.

---

### `SupplyCapReached` (code 8)

**Description:** Minting would exceed the collection's configured `max_supply`.

**Common cause:** Calling `mint` after `total_supply()` has reached the cap set at `initialize`.

**Resolution:** Check `total_supply()` against the collection's `max_supply` before minting further.

---

### `InvalidTokenId` (code 9)

**Description:** Reserved for token-ID range validation. Included for completeness with the error enum; current entry points derive `token_id` internally rather than accepting it as caller input.

**Common cause:** N/A in the current implementation.

**Resolution:** N/A.

---

### `InvalidRoyalty` (code 10)

**Description:** A royalty basis-points value exceeds 10 000 (100%).

**Common cause:** Passing `royalty_bps > 10_000` to `initialize` or a per-token override to `mint`.

**Resolution:** Keep royalty BPS values within `0`–`10_000`.

---

### `RoyaltyRecipientMissing` (code 11)

**Description:** A royalty BPS was provided without a corresponding recipient (or vice versa).

**Common cause:** Passing `royalty_bps > 0` without `royalty_recipient` (collection-level at `initialize`, or per-token at `mint`).

**Resolution:** Always supply both `royalty_bps` and `royalty_recipient` together when configuring a non-zero royalty.

---

## Oracle Contract — `OracleError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on an oracle that has already been set up.

**Common cause:** Calling `initialize` more than once on the same contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the oracle was initialized.

**Common cause:** Calling `update_price`, `get_price`, or `set_publishers` before `initialize`.

**Resolution:** Call `initialize` with the admin and staleness threshold first.

---

### `Unauthorized` (code 3)

**Description:** The caller is not the configured admin, or (for `submit_price`) not an authorized publisher.

**Common cause:** A non-admin calling `update_price`/`set_publishers`, or a publisher not in the configured set calling `submit_price`.

**Resolution:** Only the admin may push prices directly or configure publishers; only addresses passed to `set_publishers` may `submit_price`.

---

### `StalePrice` (code 4)

**Description:** The stored price is older than the configured staleness threshold.

**Common cause:** Calling `get_price` (ledger-based threshold) or `get_price_checked` (caller-supplied `max_age` in seconds) when no fresh price update has occurred recently.

**Resolution:** Ensure the admin (or a publisher) has pushed a recent price before consumers read it, or widen the staleness tolerance if appropriate.

---

### `InvalidStalenessThreshold` (code 5)

**Description:** `initialize` was called with a `staleness_threshold` of zero.

**Common cause:** Passing `0` for `staleness_threshold`.

**Resolution:** Set a positive number of ledgers for the staleness threshold.

---

### `NoPublisherData` (code 6)

**Description:** No authorized publisher has a fresh price submission for `get_median_price`.

**Common cause:** All publisher submissions are older than the requested `max_staleness_seconds`, or no publisher has ever called `submit_price`.

**Resolution:** Ensure at least one authorized publisher submits a recent price, or widen `max_staleness_seconds`.

---

### `InsufficientHistory` (code 7)

**Description:** No recorded price observation falls within the requested TWAP `window`.

**Common cause:** Calling `get_twap` with a `window` shorter than the time since the last `update_price`/`submit_price` call.

**Resolution:** Widen the `window`, or ensure prices are updated more frequently relative to the desired TWAP window.

---

## Staking Contract — `StakingError`

### `CompoundTokenMismatch` (code 8)

**Description:** `compound` was called but the stake token and reward token differ.

**Common cause:** Calling `compound` on a pool configured with different stake and reward tokens, where compounding rewards back into the stake is not meaningful.

**Resolution:** Only call `compound` on pools initialized with `stake_token == reward_token`; otherwise call `claim_rewards` instead.

---

### `UnbondingNotComplete` (code 9)

**Description:** `withdraw` was called before the unbonding period elapsed.

**Common cause:** Attempting to withdraw unbonded tokens before the ledger recorded in the `UnbondRequest` (`available_at`) has been reached.

**Resolution:** Wait until the current ledger reaches the `available_at` ledger returned by the `unbond_requested` event before calling `withdraw`.

---

### `NoUnbondRequest` (code 10)

**Description:** `withdraw` was called with no pending unbond request.

**Common cause:** Calling `withdraw` without first calling `unstake` to queue an unbond request (or after already withdrawing it).

**Resolution:** Call `unstake` to queue an unbond request before calling `withdraw`.

---

### `UnbondRequestPending` (code 11)

**Description:** `unstake` was called while a pending unbond request already exists for this staker.

**Common cause:** Calling `unstake` a second time before withdrawing (or cancelling) the first unbond request.

**Resolution:** Complete the pending `withdraw` before queuing another `unstake`.

> See `contract-api.md` for the full `StakingContract` public API and error
> codes 1–7 (`AlreadyInitialized`, `NotInitialized`, `Unauthorized`,
> `InvalidAmount`, `NoStake`, `InsufficientStake`, `NoRewards`).

---

## Subscription Contract — `SubscriptionError`

### `AlreadyInitialized` (code 1)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 2)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Calling `register_plan`, `subscribe`, or `charge` before `initialize`.

**Resolution:** Call `initialize` with the provider and payment token first.

---

### `NotAuthorized` (code 3)

**Description:** The caller is not the configured provider.

**Common cause:** A non-provider address calling `register_plan`, `set_plan_active`, or `charge`.

**Resolution:** Only the address passed as `provider` to `initialize` may call these entry points.

---

### `InvalidAmount` (code 4)

**Description:** `register_plan` was called with a non-positive `amount`.

**Common cause:** Passing `amount <= 0`.

**Resolution:** Set a positive per-interval charge amount.

---

### `InvalidInterval` (code 5)

**Description:** `register_plan` was called with `interval_ledgers == 0`.

**Common cause:** Passing a zero billing interval.

**Resolution:** Set a positive number of ledgers between charges.

---

### `AlreadySubscribed` (code 6)

**Description:** The subscriber already has an active subscription.

**Common cause:** Calling `subscribe` while a prior subscription for the same address is still active.

**Resolution:** Call `cancel` on the existing subscription before subscribing to a new plan, or check `get_subscription(address)` first.

---

### `NotSubscribed` (code 7)

**Description:** No subscription record exists for the given subscriber.

**Common cause:** Calling `charge` or `cancel` for an address that never called `subscribe`.

**Resolution:** Confirm a subscription exists via `get_subscription(address)` before charging or cancelling.

---

### `SubscriptionInactive` (code 8)

**Description:** The subscription has already been cancelled.

**Common cause:** Calling `charge` or `cancel` a second time after `cancel` already ran.

**Resolution:** Check `get_subscription(address).active` before acting on it; a cancelled subscriber must call `subscribe` again to resume.

---

### `IntervalNotElapsed` (code 9)

**Description:** `charge` was called before the trial period or billing interval had fully elapsed.

**Common cause:** Charging a subscriber too soon after the last charge (or before the configured `trial_ledgers` completed).

**Resolution:** Wait until `last_charged_ledger + interval_ledgers` (or `+ trial_ledgers` during the trial) is reached before charging again.

---

### `InsufficientAllowance` (code 10)

**Description:** The subscriber has not granted this contract enough token allowance to cover the charge.

**Common cause:** The subscriber never called `token.approve`, or the allowance has been exhausted or expired.

**Resolution:** Have the subscriber call `approve(subscriber, subscription_contract, amount * periods, expiry_ledger)` on the payment token with sufficient allowance.

---

### `PlanAlreadyExists` (code 11)

**Description:** `register_plan` was called with a `plan_id` that already exists.

**Common cause:** Registering the same plan ID twice.

**Resolution:** Use `set_plan_active` to update an existing plan instead of re-registering it.

---

### `PlanNotFound` (code 12)

**Description:** The referenced `plan_id` does not exist.

**Common cause:** Calling `subscribe` or `set_plan_active` with a `plan_id` that was never registered.

**Resolution:** Call `get_plan(plan_id)` to confirm the plan exists, or register it first via `register_plan`.

---

### `PlanInactive` (code 13)

**Description:** `subscribe` was called for a plan the provider has deactivated.

**Common cause:** Attempting to subscribe to a plan after the provider called `set_plan_active(plan_id, false)`.

**Resolution:** Only subscribe to plans where `get_plan(plan_id).active == true`.

---

## Swap Contract — `SwapError`

> **Note:** `contracts/swap/src/lib.rs` currently contains corrupted/duplicated
> code (e.g. `set_fee_bps` and `get_fee_bps` are each defined twice, and some
> branches reference states/errors like `SwapState::Pending`/`Accepted` and
> `SwapError::SwapNotPending`/`SwapExpired` that don't exist in `storage.rs` /
> `errors.rs`). This section is cross-checked against the authoritative
> `errors.rs` enum below; see `contract-api.md` for how this affects the
> documented public API.

### `NotAuthorized` (code 1)

**Description:** The caller is not permitted to perform this action.

**Common cause:** Someone other than party A calling `cancel_swap`, or someone other than the admin calling admin-only setters.

**Resolution:** Only the swap's proposing party (or the admin, for configuration) may perform restricted actions.

---

### `SwapNotFound` (code 2)

**Description:** The referenced `swap_id` does not exist.

**Common cause:** Passing a stale or invalid `swap_id` to `accept_swap`, `cancel_swap`, or `get_swap`.

**Resolution:** Use the ID returned by `propose_swap`.

---

### `InvalidState` (code 3)

**Description:** The swap is not in the `Open` state required for this operation.

**Common cause:** Calling `accept_swap` or `cancel_swap` on a swap that was already completed or cancelled.

**Resolution:** Check `get_swap(id).state` before acting on it.

---

### `DeadlineExpired` (code 4)

**Description:** The swap's `expires_at` ledger has already passed.

**Common cause:** Calling `accept_swap` after the proposed swap's expiry.

**Resolution:** Accept before `get_swap(id).expires_at`; an expired swap can no longer be accepted.

---

### `InvalidAmount` (code 5)

**Description:** A swap amount is zero or negative.

**Common cause:** Passing `amount_a <= 0` or `amount_b <= 0` to `propose_swap`.

**Resolution:** Propose swaps with strictly positive amounts on both sides.

---

### `InvalidDeadline` (code 6)

**Description:** The supplied `expires_at` is not in the future.

**Common cause:** Passing `expires_at <= current ledger` to `propose_swap`.

**Resolution:** Choose an `expires_at` ledger sequence greater than the current one.

---

### `AlreadyCompleted` (code 7)

**Description:** An operation was attempted on a swap that has already been accepted/completed.

**Common cause:** Calling `cancel_swap` on a swap that `accept_swap` already settled.

**Resolution:** Check `get_swap(id).state` before cancelling.

---

### `AlreadyCancelled` (code 8)

**Description:** An operation was attempted on a swap that has already been cancelled.

**Common cause:** Calling `accept_swap` or `cancel_swap` a second time after cancellation.

**Resolution:** Check `get_swap(id).state` before acting on it.

---

### `AlreadyInitialized` (code 9)

**Description:** `initialize` was called on a contract that has already been set up.

**Common cause:** Calling `initialize` more than once on the same deployed contract instance.

**Resolution:** `initialize` should only be called once, immediately after deployment.

---

### `NotInitialized` (code 10)

**Description:** An operation was attempted before the contract was initialized.

**Common cause:** Calling `propose_swap`, `set_admin`, or other entry points before `initialize`.

**Resolution:** Call `initialize` with the admin and fee configuration first.

---

### `InvalidFee` (code 11)

**Description:** A fee basis-points value exceeds 10 000 (100%).

**Common cause:** Passing `fee_bps > 10_000` to `initialize` or `set_fee_bps`.

**Resolution:** Keep fee BPS values within `0`–`10_000`.

---

## Timelock Contract — `TimelockError`

### `NotAuthorized` (code 1)

**Description:** The caller is not the admin.

**Common cause:** A non-admin address calling `cancel` or `reassign_beneficiary`.

**Resolution:** Only the address passed as `admin` to `initialize` (or `initialize_with_tranches`) may call these entry points.

---

### `AlreadyInitialized` (code 2)

**Description:** `initialize` (or `initialize_with_tranches`) was called on a timelock that has already been set up.

**Common cause:** Calling either initializer more than once on the same contract instance.

**Resolution:** Only one of `initialize` / `initialize_with_tranches` should be called once, immediately after deployment.

---

### `NotInitialized` (code 3)

**Description:** An operation was attempted before the timelock was initialized.

**Common cause:** Calling `release`, `cancel`, or `get_info` before `initialize`/`initialize_with_tranches`.

**Resolution:** Call one of the initializer functions first.

---

### `NotYetReleasable` (code 4)

**Description:** `release` was called before any tranche (or the single release ledger) became due.

**Common cause:** Calling `release` before the current ledger reaches `release_ledger` (single-tranche) or any tranche's `release_ledger` (multi-tranche).

**Resolution:** Check `is_releasable()` before calling `release`.

---

### `AlreadyReleased` (code 5)

**Description:** `release` or `cancel` was called after the timelock already fully released.

**Common cause:** Calling `release` again once all tranches have been released, or calling `cancel` post-release.

**Resolution:** Check `get_info().state` before acting; a fully released timelock is terminal.

---

### `AlreadyCancelled` (code 6)

**Description:** `release` or `cancel` was called after the timelock was already cancelled.

**Common cause:** Calling `cancel` twice, or calling `release` after cancellation.

**Resolution:** Check `get_info().state` before acting; a cancelled timelock is terminal.

---

### `InvalidAmount` (code 7)

**Description:** The lock amount (or a tranche amount) is zero, negative, or the tranches list is empty.

**Common cause:** Passing `amount <= 0` to `initialize`, an empty `tranches` vec, or any `tranche.amount <= 0` to `initialize_with_tranches`.

**Resolution:** Ensure every locked amount is strictly positive and at least one tranche is provided.

---

### `InvalidReleaseLedger` (code 8)

**Description:** A release ledger is not strictly in the future relative to the prior tranche (or the current ledger).

**Common cause:** Passing `release_ledger <= current ledger` to `initialize`, or tranches whose `release_ledger` values are not strictly increasing / not in the future, to `initialize_with_tranches`.

**Resolution:** Choose release ledgers strictly greater than the current ledger, and (for tranches) strictly increasing across the schedule.

---

## Vesting Contract — `VestingError`

### `CliffAlreadyPassed` (code 8)

**Description:** An admin release action was attempted after the vesting cliff has already passed.

**Common cause:** Calling an admin-only pre-cliff release adjustment once the beneficiary's cliff has already been reached.

**Resolution:** Admin release actions gated by this check are only valid before `CliffLedger`; afterward the beneficiary should use `claim` directly.

---

### `ScheduleAlreadyExists` (code 9)

**Description:** A vesting schedule already exists for the specified beneficiary.

**Common cause:** Attempting to create a second schedule for a beneficiary that already has one.

**Resolution:** Check for an existing schedule (e.g. via `get_vested_amount`/`get_claimed_amount`) before creating a new one for the same beneficiary.

---

### `ScheduleNotFound` (code 10)

**Description:** No vesting schedule was found for the specified beneficiary.

**Common cause:** Querying or claiming against a beneficiary address that was never set up with a schedule.

**Resolution:** Confirm the beneficiary address matches the one used when the schedule was created.

> See `contract-api.md` for the full `VestingContract` public API and error
> codes 1–7 (`AlreadyInitialized`, `NotInitialized`, `NotAuthorized`,
> `InvalidAmount`, `InvalidSchedule`, `NothingToClaim`, `AlreadyRevoked`).
