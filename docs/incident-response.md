# Incident Response Playbook

This guide covers the steps to take when a deployed contract is compromised, a critical bug is discovered, or an admin key is exposed.

---

## 1. Pause the Contract Immediately

If the contract was deployed with the `pausable` feature, halt all operations instantly:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- pause
```

Verify the paused state:

```bash
# Escrow contract — use get_state (returns the full escrow state including paused flag):
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- get_state

# Token contract — there is currently no dedicated read-only is_paused query.
# Use stellar contract info to inspect on-chain state, or check your off-chain
# monitoring (see scripts/monitor-token.sh) for the paused flag.
# A universal is_paused / get_state function across all contracts is a known gap
# (tracked separately) — the exact verification command differs per contract.
```

> If the contract does not support pausing, proceed immediately to upgrading to a patched WASM (§3).

---

## 2. Rotate the Admin Key

Use the two-step admin transfer to hand control to a fresh, uncompromised key.

**Step 1 — Propose the new admin** (from the current, still-accessible key):

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <CURRENT_ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- propose_admin \
  --new_admin <NEW_ADMIN_ADDRESS>
```

**Step 2 — Accept from the new key:**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <NEW_ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- accept_admin
```

> If the current admin key is already compromised and an attacker could front-run the proposal, deploy a new contract instance instead of rotating in place.

Revoke the old key from all CI secrets, `.env` files, and key-management systems immediately.

---

## 3. Upgrade to a Patched WASM

Applies to contracts built with the `upgradeable` / `pausable` feature flag. A 24-hour timelock (`UPGRADE_DELAY_LEDGERS = 17 280`) is enforced between proposing and executing.

**Step 1 — Build and upload the patched WASM:**

```bash
cd contracts/<contract-name>
stellar contract build
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/<contract>.wasm \
  --source-account <ADMIN_KEY> \
  --network <testnet|mainnet>
# Note the returned WASM hash
```

**Step 2 — Propose the upgrade:**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- propose_upgrade \
  --wasm_hash <NEW_WASM_HASH>
```

**Step 3 — Wait for the timelock, then execute:**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_KEY> \
  --network <testnet|mainnet> \
  -- execute_upgrade
```

**Step 4 — Verify the new WASM hash:**

```bash
stellar contract info --id <CONTRACT_ID> --network <testnet|mainnet>
```

> The timelock cannot be bypassed on mainnet. If immediate mitigation is needed, pause the contract and communicate clearly while the timelock elapses.

---

## 4. Communicate with Affected Users

Timely, honest communication reduces harm and preserves trust.

1. **Announce immediately** — Post on all official channels (Discord, Twitter/X, project website) that an incident has been detected and the contract has been paused while a fix is prepared.
2. **Describe the impact** — State clearly which operations are affected, whether funds are at risk, and what users should or should not do (e.g., "do not attempt new deposits").
3. **Publish a timeline** — Share the expected time for the fix to be deployed, accounting for the upgrade timelock.
4. **Confirm resolution** — After the upgrade is executed, post a follow-up confirming the contract is back online and describing what was fixed.
5. **On-chain notice** — If possible, emit a contract event or update contract metadata to record the incident reference.

---

## 5. Post-Incident Review Checklist

Complete this within 48 hours of resolution.

- [ ] Root cause identified and documented
- [ ] Affected users and funds quantified
- [ ] Patched WASM hash recorded and verified on-chain
- [ ] Old admin key revoked from all systems (CI secrets, `.env`, key managers)
- [ ] New admin key stored securely (hardware wallet or multi-sig for mainnet)
- [ ] Upgrade timelock confirmed working as expected
- [ ] Incident timeline written up and published
- [ ] Security fix back-ported to all active contract versions
- [ ] Test coverage added to prevent regression
- [ ] `docs/security.md` updated if threat model changed
- [ ] Team retrospective completed; action items assigned and tracked
