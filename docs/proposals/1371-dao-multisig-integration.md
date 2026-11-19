# #1371: Integration Test for a DAO Proposal Executed Through a Multisig

_Status: Proposed · Tracking issue: #1371_

## Goal

Test the governance flow where a DAO vote authorizes a treasury action that a
multisig then carries out:

1. `DaoContract` passes a proposal for a treasury transfer.
2. The approved proposal creates a multisig transaction.
3. Multisig signers approve it, and it executes after a delay.

## Current state (read before implementing)

The issue assumes behaviour the contracts don't have yet:

| Issue assumes | What the code has |
|---|---|
| DAO proposals carry an executable action | `Proposal` holds only `title`, `description` and vote tallies. `execute_proposal` sets `state = Executed` and emits an event; it calls no other contract |
| `MultisigContract::propose` | The entry point is `propose_transaction(proposer, target, function, args, expiry_ledgers) -> u64`, and `proposer` must be a registered signer |
| Timelocked multisig execution | Multisig has an **expiry** (`expiry_ledger`: execution must happen *before* it), not a delay. `execute_transaction` runs as soon as `accumulated_weight >= threshold` |

So "authorization chaining" between the two contracts can't happen on-chain
today. There are two ways forward.

### Option A: test the composition as it exists (recommended first PR)

The test acts as the off-chain relayer that links the two contracts. This is
how the starter kit's DAO is meant to be used today.

1. The DAO passes proposal `N`. Its `description` encodes the action (target,
   recipient, amount).
2. The test asserts `dao.get_proposal(N).state == Executed`, then a multisig
   signer calls `propose_transaction` with the matching target and args.
3. The signers call `sign_transaction`, and anyone calls
   `execute_transaction`.
4. For the "timelock", deploy `TimelockContract` as the transfer's
   **recipient**: the multisig releases treasury funds into a timelock that
   pays the beneficiary only after its release ledger.

### Option B: add on-chain chaining (contract change)

- Extend `Proposal` with `action: Option<(Address, Symbol, Vec<Val>)>`, and
  have `execute_proposal` call `env.invoke_contract` with it.
- Register the **DAO contract address as a multisig signer**, so its
  `require_auth()` in `propose_transaction` is satisfied by the DAO's own
  invocation.
- Optionally add a `min_delay_ledgers` to multisig so `execute_transaction`
  rejects calls before `proposed_at + delay`.

Option B changes storage layout and the threat model (see
[threat-model.md](../threat-model.md) and
[upgrade-compatibility-matrix.md](../upgrade-compatibility-matrix.md)) and
should go in its own PR. The test below works for Option A, and Option B only
removes step 2's manual relay.

## Test setup (Option A)

| Actor / contract | Setup |
|---|---|
| Governance token | `TokenContract`; mint voting power to `voter_a` and `voter_b` |
| `DaoContract` | `initialize(admin, gov_token, voting_period, quorum, quorum_bps)` |
| Treasury asset | SAC; mint treasury balance to the multisig address |
| `MultisigContract` | `initialize(vec![s1, s2, s3], 2, None)` (2-of-3) |
| `TimelockContract` | Beneficiary `recipient`, release ledger `now + DELAY` |

## Test outline

```rust
#[test]
fn test_dao_proposal_executed_via_multisig_into_timelock() {
    let env = Env::default();
    env.mock_all_auths();

    // DAO phase
    let pid = dao.create_proposal(&voter_a, &title, &desc);
    dao.vote(&voter_a, &pid, &true);
    dao.vote(&voter_b, &pid, &true);
    env.ledger().with_mut(|l| l.sequence_number += VOTING_PERIOD + 1);
    dao.execute_proposal(&pid);
    assert_eq!(dao.get_proposal(&pid).state, ProposalState::Executed);

    // Multisig phase: transfer treasury → timelock
    let args = vec![&env, msig_addr.into_val(&env), timelock_addr.into_val(&env), AMOUNT.into_val(&env)];
    let tx = msig.propose_transaction(&s1, &treasury_sac, &symbol_short!("transfer"), &args, &100);
    msig.sign_transaction(&s1, &tx);
    assert!(msig.try_execute_transaction(&tx).is_err()); // 1-of-3 is below the threshold
    msig.sign_transaction(&s2, &tx);
    msig.execute_transaction(&tx);

    // Timelock phase
    assert!(timelock.try_release().is_err());             // before the delay
    env.ledger().with_mut(|l| l.sequence_number += DELAY);
    timelock.release();
}
```

`sign_transaction` and `execute_transaction` signatures are taken from
`contracts/multisig/src/lib.rs`. Check the `TimelockContract::initialize`
arguments in `contracts/timelock/src/lib.rs` before writing the test.

## Assertions

**State transitions:**

| Step | Contract | Expected |
|---|---|---|
| After votes, before deadline | DAO | `execute_proposal` → `DeadlineNotReached` |
| After deadline | DAO | `state == Executed` |
| After 1 signature | Multisig | `signature_count(tx) == 1`, `execute` → `ThresholdNotMet` |
| After 2 signatures + execute | Multisig | `get_transaction(tx).executed == true` |
| Re-execute | Multisig | `AlreadyExecuted` |
| Before delay | Timelock | `release` errors; `is_releasable() == false` |
| After delay | Timelock | Funds arrive at `recipient` |

**Balances:** the multisig treasury decreases by `AMOUNT`, the timelock holds
`AMOUNT` until release, and afterwards the recipient holds `AMOUNT` and the
timelock holds `0`.

**Authorization.** Re-run the flow without `mock_all_auths`, using
`env.mock_auths(&[...])` for each signer, and check `env.auths()` after each
call:

- `propose_transaction` / `sign_transaction` record auth only for the calling signer.
- A non-signer calling `propose_transaction` → `NotSigner`.
- With Option B, `env.auths()` after `execute_proposal` should show the DAO
  contract as the authorizer of `propose_transaction`.

**Negative paths:** a rejected DAO proposal (`yes <= no`) produces no multisig
transaction, and an expired multisig transaction (past `expiry_ledger`)
can't be executed.

## Running

```bash
cargo test -p soroban-integration-tests --test integration dao_proposal_executed_via_multisig
```

Add the `dao`, `multisig` and `timelock` crates to `tests/Cargo.toml`. The
integration crate also needs to compile first; see the blockers in
[#1370](1370-marketplace-wrapped-token-integration.md#blockers-to-resolve-first).

## Acceptance criteria

- [ ] The integration test is in `tests/tests/integration.rs`
- [ ] Authorization between the DAO and multisig is verified (relayed in Option A, on-chain in Option B)
- [ ] State transitions across the DAO, multisig (and timelock) are asserted
