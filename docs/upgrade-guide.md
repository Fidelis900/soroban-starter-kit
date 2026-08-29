# Contract Upgrade Guide

Step-by-step guide for upgrading a contract WASM on-chain using timelock proposals and safe key rotation.

## Overview

On-chain upgrades follow a timelock-protected flow:
1. **Propose** — Admin creates upgrade proposal with new WASM hash
2. **Wait** — Timelock delay passes (governance approval period)
3. **Execute** — Deploy new WASM, verify, and resume operations
4. **Verify** — Confirm new code is live

This ensures security by preventing instant unauthorized upgrades.

Before writing the new WASM, check the
[Upgrade Compatibility Matrix](upgrade-compatibility-matrix.md) for what
storage your contract shares with other contracts and which changes would
break existing on-chain data.

## Prerequisites

- Stellar CLI installed and configured
- Admin key (controlled privately)
- New key for signing (for key rotation)
- Testnet or Mainnet RPC endpoint
- Current contract ID

## Step 1: Build New WASM

Build the upgraded contract:

```bash
cd contracts/escrow
stellar contract build

# Output: target/wasm32-unknown-unknown/release/soroban_escrow_contract.wasm
```

Get the WASM hash:

```bash
stellar contract install \
  --network testnet \
  --source admin-key \
  target/wasm32-unknown-unknown/release/soroban_escrow_contract.wasm

# Returns: WASM hash (xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx)
# Save this for the proposal step
WASM_HASH="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

## Step 2: Propose Upgrade (with Timelock)

**Important:** Only call this if your contract includes timelock pause functionality (escrow with `pausable` feature).

Propose the upgrade to activate after the timelock delay:

> **Important:** The timelock delay is a **fixed contract constant** (`UPGRADE_DELAY_LEDGERS = 17_280`), not a parameter you supply at call time. `propose_upgrade` only accepts the WASM hash; the delay is always 17,280 ledgers (≈ 24 hours at 5 s/ledger: 17,280 × 5 s = 86,400 s) regardless of any shell variable you set locally.

```bash
CONTRACT_ID="CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
ADMIN_KEY="admin-key"

stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- propose_upgrade --wasm_hash $WASM_HASH
```

**Response:** Proposal created. Current ledger: `12345`. Execution available at: `29625` (12345 + 17280).

Record the target ledger for step 3.

```bash
TARGET_LEDGER=29625  # example: 12345 + 17280
```

## Step 3: Wait for Timelock (Optional Key Rotation)

Wait until the target ledger is reached. During this window, you can rotate keys for security:

### Key Rotation (Optional but Recommended)

1. **Generate new key:**

```bash
stellar keys generate new-admin-key
# Securely back up the secret key
```

2. **Export current admin from contract** (token contract only):

> **Note:** This step applies to the **token** contract, which exposes a single `admin` function. The **escrow** contract uses role-based parties (buyer / seller / arbiter) with no single admin address — skip this step for escrow.

```bash
CURRENT_ADMIN=$(stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- admin)
```

3. **Update admin in new code** (if contract includes admin management):

Edit contract code to hardcode new admin, or use a contract upgrade that changes admin permissions.

4. **Fund new key** (testnet):

```bash
stellar account create new-admin-key --starting-balance 10 --network testnet
```

## Step 4: Execute Upgrade

Check current ledger:

```bash
CURRENT_LEDGER=$(stellar ledger info --network testnet | grep "^Sequence" | awk '{print $2}')
echo "Current ledger: $CURRENT_LEDGER, Target: $TARGET_LEDGER"
```

If `CURRENT_LEDGER >= TARGET_LEDGER`, execute:

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- execute_upgrade
```

**Response:** Upgrade executed. New WASM deployed.

## Step 5: Verify New Code

Verify the upgrade:

```bash
# 1. Check new WASM hash matches:
stellar contract info \
  --id $CONTRACT_ID \
  --network testnet | grep "WASM Hash"

# Should match $WASM_HASH from step 1

# 2. Call a query function to confirm contract is live:
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- version

# Should return new version string
```

## Step 6: Resume Operations

Resume normal contract operations:

```bash
# Unpause (if contract was paused during upgrade)
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- unpause
```

## Rollback Procedure

If the upgrade fails:

### 1. Immediate Actions (Before Timelock Expires)

> **Note:** There is currently **no `cancel_upgrade` function** in any contract in this repository. Once `propose_upgrade` is called, the pending proposal cannot be retracted — it will simply become executable after 17,280 ledgers. If you proposed an upgrade by mistake:
> - Do **not** execute it when the timelock expires.
> - If you need to prevent execution entirely (e.g., the contract should not be upgraded at all), pausing the contract first and then re-deploying a patched version via a second proposal is the safest path.
> - As a future improvement, a `cancel_upgrade` function could be added to allow the admin to retract a pending proposal before it becomes executable.

### 2. Restore Previous WASM

Redeploy the previous WASM:

```bash
# 1. Install old WASM
PREV_HASH=$(stellar contract install \
  --network testnet \
  --source $ADMIN_KEY \
  path/to/previous/wasm)

# 2. Create new upgrade proposal
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- propose_upgrade --wasm_hash $PREV_HASH

# 3. Wait and execute as in steps 3-4
```

### 3. Key Rotation for Revoked Keys

If keys were compromised, use the two-step admin transfer (token contract):

```bash
# 1. Propose the emergency key as the new admin (from current, still-accessible key)
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $ADMIN_KEY \
  -- propose_admin --new_admin $EMERGENCY_KEY

# 2. Accept from the new emergency key
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $EMERGENCY_KEY \
  -- accept_admin

# 3. Disable all other keys in your key management system
```

> **Note:** The `rotate_admin` function does not exist. The token contract uses a two-step `propose_admin` / `accept_admin` flow to prevent admin lockout from typo'd addresses. The escrow contract has no single admin to rotate — its roles (buyer / seller / arbiter) are fixed at initialization; key compromise on escrow requires deploying a new contract instance.

## Testing Upgrades (Local)

Test locally before mainnet deployment:

```bash
# 1. Deploy contract to local node
docker compose up stellar-node
stellar contract deploy \
  --network local \
  --source local-admin \
  target/wasm32-unknown-unknown/release/soroban_escrow_contract.wasm

# 2. Propose upgrade
stellar contract invoke \
  --id $CONTRACT_ID \
  --network local \
  --source local-admin \
  -- propose_upgrade --wasm_hash $WASM_HASH

# 3. Mine blocks until timelock passed (locally, instant)
stellar ledger bump --network local

# 4. Execute and verify
stellar contract invoke \
  --id $CONTRACT_ID \
  --network local \
  --source local-admin \
  -- execute_upgrade

# 5. Confirm new code
stellar contract invoke \
  --id $CONTRACT_ID \
  --network local \
  -- version
```

## Safety Checklist

Before executing any upgrade:

- [ ] New WASM built and tested locally
- [ ] WASM hash verified matches `stellar contract install` output
- [ ] All state migrations planned (if storage format changed)
- [ ] Timelock delay respected (no shortcuts)
- [ ] Emergency key prepared and funded
- [ ] Rollback procedure tested locally
- [ ] Team notified of upgrade window
- [ ] Monitoring and alerts configured for post-upgrade
- [ ] Backup of previous WASM and state taken
- [ ] Admin key access reviewed and restricted

## Common Issues

### "Timelock not reached"
```
Error: Proposal not yet executable
Solution: Current ledger < target ledger. Wait for more blocks.
```

### "WASM hash mismatch"
```
Error: Installed WASM hash does not match proposal
Solution: Re-run stellar contract install with exact same binary
```

### "Admin not authorized"
```
Error: Caller not admin
Solution: Sign with correct admin key, check contract has correct admin set
```

## Mainnet Considerations

For mainnet deployments:

1. **Extend timelock delay:** Use `UPGRADE_DELAY_LEDGERS = 17_280` (1 day @ 5-second blocks)
2. **Governance approval:** Require multisig consensus before proposing
3. **Public announcement:** Notify users of upgrade schedule
4. **Parallel testing:** Deploy to testnet first, run for 24+ hours
5. **Monitoring:** Set up alerts for success/failure
6. **Post-upgrade audit:** Have third party verify new code

## References

- [Soroban Contract Upgrades](https://soroban.stellar.org/docs/learn/upgrading-contracts)
- [Stellar CLI Docs](https://stellar.org/docs)
- [Timelock Contract Reference](./contract-api.md#timelock-contract)
