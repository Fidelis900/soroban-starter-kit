# Soroban Storage Rent, TTL Management, and State Archival Runbook

This runbook is the operational reference for teams building and running Soroban smart contracts. It explains how Soroban storage tiers interact with time-to-live (TTL) semantics, how storage rent and eviction work, how to keep state alive with automated TTL bumping, how to estimate rent costs, and how to restore archived state using keeper processes.

If you are new to Soroban, read this document end-to-end before deploying a contract that holds long-lived state. Storage misconfiguration is one of the most common causes of production incidents on Soroban.

---

## Table of Contents

1. [Storage Tiers and TTL Semantics](#1-storage-tiers-and-ttl-semantics)
2. [Storage Rent and Eviction Behavior](#2-storage-rent-and-eviction-behavior)
3. [Automated TTL Bumping](#3-automated-ttl-bumping)
4. [Rent Cost Estimation](#4-rent-cost-estimation)
5. [Archive Restoration with Keepers](#5-archive-restoration-with-keepers)
6. [Operational Checklists](#6-operational-checklists)
7. [Related Documentation](#7-related-documentation)

---

## 1. Storage Tiers and TTL Semantics

Soroban exposes three storage tiers. Each has different lifetime, cost, and access semantics. Choosing the wrong tier is the root cause of most "my data disappeared" incidents.

| Tier | Scope | Default TTL | Max TTL | Cost | Use for |
|------|-------|-------------|---------|------|---------|
| **Temporary** | Per-contract, per-key | Short (ledger-bounded) | Same as persistent | Cheapest | Ephemeral data: nonces, caches, in-flight state, short-lived locks |
| **Instance** | Per-contract (single shared entry) | Tied to contract instance | Same as persistent | Moderate | Contract configuration, admin keys, global counters, upgrade metadata |
| **Persistent** | Per-contract, per-key | Long | Same as persistent | Most expensive | User balances, ownership records, long-lived business data |

### 1.1 TTL units

TTLs are expressed in **ledgers**, not seconds. On Stellar mainnet a ledger closes roughly every 5 seconds, so:

- 1 day ≈ 17,280 ledgers
- 30 days ≈ 518,400 ledgers
- 1 year ≈ 6,307,200 ledgers

Always compute TTLs from ledger counts, and derive them from a constant rather than hard-coding magic numbers.

### 1.2 Key semantics

- **Temporary entries** are removed (not archived) when their TTL expires. There is no restoration path. Treat temporary storage as a cache: if it is gone, your contract must be able to rebuild it.
- **Instance entries** share a single TTL with the contract instance. If the instance TTL lapses, the entire contract becomes inaccessible until restored. Every call that touches the contract requires the instance to be live.
- **Persistent entries** are archived (not deleted) when their TTL expires. Archived entries can be restored by submitting a `RestoreFootprint` operation. Until restored, any contract call that reads or writes the archived key will fail.

### 1.3 The critical rule

> **Instance TTL must always be at least as long as the longest-lived persistent key you depend on.** If the instance expires, you cannot even reach the code that would restore your persistent data.

### 1.4 Reading TTLs from a contract

```rust
use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol};

#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Symbol),
}

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    /// Returns the remaining TTL (in ledgers) for the instance entry.
    pub fn instance_ttl(env: Env) -> u32 {
        env.storage().instance().get_ttl()
    }

    /// Returns the remaining TTL (in ledgers) for a persistent key.
    pub fn persistent_ttl(env: Env, who: Symbol) -> u32 {
        env.storage().persistent().get_ttl(&DataKey::Balance(who))
    }
}
```

`get_ttl()` returns the number of ledgers remaining before expiry. A value of `0` means the entry is already expired (persistent) or gone (temporary).

---

## 2. Storage Rent and Eviction Behavior

Soroban storage is not free. Every ledger entry occupies space in the ledger, and that space is rented from the network. Rent is paid up front as part of the transaction that creates or extends an entry.

### 2.1 How rent is charged

- When you **write** a new entry, the transaction pays rent for the entry's initial TTL.
- When you **extend** an entry's TTL, the transaction pays rent for the additional ledgers.
- When an entry's TTL **lapses**, the network evicts it according to its tier (see below).
- Rent is charged in stroops (1 XLM = 10,000,000 stroops) and is a function of entry size and TTL length.

### 2.2 Eviction behavior by tier

| Tier | On TTL expiry | Recoverable? |
|------|---------------|--------------|
| Temporary | Entry is deleted from the ledger | No |
| Instance | Contract instance is archived; contract is unreachable | Yes, via `RestoreFootprint` |
| Persistent | Entry is archived to cold storage | Yes, via `RestoreFootprint` |

### 2.3 What "archived" means

Archived entries are not lost. They are moved out of the live ledger into the archive, and the network retains a cryptographic commitment to them. To bring them back you submit a `RestoreFootprint` operation that names the archived keys. The operation pays rent to re-establish the entry in the live ledger with a fresh TTL.

### 2.4 Failure modes to watch for

- **Instance expiry cascade**: If the instance TTL lapses, every persistent key becomes unreachable at once. Restoring the instance is a prerequisite for restoring anything else.
- **Silent temporary loss**: Temporary entries vanish without error. Contracts that assume a temporary key still exists will panic or misbehave.
- **Rent spikes**: Extending a large persistent entry for a long TTL can be expensive. Budget for it and monitor it.
- **Restoration storms**: If many entries expire at once (e.g., a batch of user records created in the same ledger), restoration can be costly and slow. Stagger TTLs.

---

## 3. Automated TTL Bumping

The safest pattern is to **bump TTLs proactively** on every state-touching call, and to run an **off-chain keeper** that bumps entries that are not touched by user traffic.

### 3.1 Bumping the instance TTL

Bump the instance TTL on every entry point. This is cheap and guarantees the contract stays reachable.

```rust
use soroban_sdk::{contract, contractimpl, Env};

const INSTANCE_TTL_THRESHOLD: u32 = 17_280 * 7;   // bump if < 7 days remain
const INSTANCE_TTL_EXTEND_TO: u32 = 17_280 * 30;  // extend to 30 days

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
    }

    pub fn deposit(env: Env, who: soroban_sdk::Symbol, amount: i128) {
        Self::bump_instance(&env);
        // ... deposit logic ...
    }
}
```

`extend_ttl(threshold, extend_to)` is a no-op if the current TTL is already greater than `threshold`. This makes it safe to call on every invocation.

### 3.2 Bumping persistent keys

Bump persistent keys whenever they are read or written. Use a threshold that is comfortably larger than your keeper's polling interval so that a missed keeper run does not cause expiry.

```rust
use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol};

const PERSISTENT_TTL_THRESHOLD: u32 = 17_280 * 14;  // bump if < 14 days remain
const PERSISTENT_TTL_EXTEND_TO: u32 = 17_280 * 60;  // extend to 60 days

#[contracttype]
pub enum DataKey {
    Balance(Symbol),
}

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    fn bump_persistent(env: &Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
    }

    pub fn balance(env: Env, who: Symbol) -> i128 {
        let key = DataKey::Balance(who.clone());
        Self::bump_persistent(&env, &key);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    pub fn set_balance(env: Env, who: Symbol, amount: i128) {
        let key = DataKey::Balance(who);
        env.storage().persistent().set(&key, &amount);
        Self::bump_persistent(&env, &key);
    }
}
```

### 3.3 Bumping temporary keys

Temporary entries are cheap and short-lived. Bump them only if you actually need them to survive longer than the default.

```rust
const TEMP_TTL_THRESHOLD: u32 = 17_280;      // 1 day
const TEMP_TTL_EXTEND_TO: u32 = 17_280 * 3;  // 3 days

fn bump_temporary(env: &Env, key: &DataKey) {
    env.storage()
        .temporary()
        .extend_ttl(key, TEMP_TTL_THRESHOLD, TEMP_TTL_EXTEND_TO);
}
```

### 3.4 Batch bumping with a keeper entry point

Expose a permissioned entry point that bumps a batch of keys. This lets an off-chain keeper renew many entries in a single transaction.

```rust
use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol, Vec};

#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Symbol),
}

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    fn require_admin(env: &Env) {
        let admin: Symbol = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
    }

    /// Keeper entry point: bump TTLs for a batch of persistent keys.
    pub fn bump_batch(env: Env, keys: Vec<Symbol>) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
        for who in keys.iter() {
            let key = DataKey::Balance(who);
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
        }
    }
}
```

### 3.5 Off-chain keeper loop (pseudo-code)

```text
every POLL_INTERVAL (e.g. 6 hours):
    keys = indexer.list_keys_with_ttl_below(PERSISTENT_TTL_THRESHOLD)
    for batch in chunk(keys, MAX_BATCH_SIZE):
        submit bump_batch(batch) signed by keeper key
    if instance_ttl < INSTANCE_TTL_THRESHOLD:
        submit bump_instance() signed by keeper key
```

Keep the keeper key in a secrets manager, not in source control. Rotate it on a schedule and on any suspected compromise.

### 3.6 Choosing thresholds

| Parameter | Guidance |
|-----------|----------|
| `threshold` | At least 2× your keeper polling interval, plus a safety margin for failed runs |
| `extend_to` | At least 2× `threshold`, so a single successful bump buys several keeper cycles |
| Instance `extend_to` | Must be ≥ the longest persistent `extend_to` you rely on |

---

## 4. Rent Cost Estimation

Rent is a function of **entry size** and **TTL length**. Exact rates are set by the network and can change via protocol upgrades, so always estimate against current network parameters rather than hard-coding rates.

### 4.1 Estimation procedure

1. Determine the serialized size of each entry (in bytes). Use the Soroban XDR size of the key plus value.
2. Determine the desired TTL in ledgers.
3. Multiply size × TTL × the current rent rate per byte-ledger.
4. Sum across all entries the transaction creates or extends.
5. Add a buffer (recommend 20–30%) for protocol parameter changes.

### 4.2 Practical guidance

- **Estimate before you deploy.** Run a dry-run simulation (`stellar contract invoke --simulate` or the SDK's simulation API) and inspect the resource footprint and fee.
- **Prefer smaller keys.** Use compact key encodings (e.g., `u64` IDs instead of long strings) to reduce per-entry rent.
- **Stagger TTLs.** Do not create thousands of entries with identical TTLs; they will all expire together and create a restoration storm.
- **Budget for renewal, not just creation.** A contract that lives for years will pay rent many times over. Model total cost of ownership, not just the first write.
- **Monitor rent spend.** Emit events on TTL bumps and track spend per contract in your observability stack.

### 4.3 Simulation example

```bash
# Simulate a call and inspect the resource footprint and fee
stellar contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  --source $KEEPER_KEY \
  --simulate \
  -- bump_batch --keys '["alice","bob"]'
```

The simulation output includes the ledger footprint (which entries are read/written) and the fee. Multiply the fee by your expected call frequency to project monthly rent spend.

---

## 5. Archive Restoration with Keepers

When persistent or instance entries expire, they are archived. Restoration is a two-step process: **detect** the archived state, then **submit a `RestoreFootprint` operation** naming the archived keys.

### 5.1 Detection

Your keeper should detect archived state by attempting a read and catching the "entry archived" error, or by querying an indexer that tracks TTLs. Do not rely on user reports.

```text
every POLL_INTERVAL:
    for key in indexer.list_keys():
        if indexer.ttl(key) == 0:
            queue_for_restoration(key)
    if indexer.instance_ttl(contract) == 0:
        queue_for_restoration(contract_instance)
```

### 5.2 Restoration order

> **Always restore the instance first.** If the instance is archived, the contract is unreachable and you cannot restore persistent keys through contract calls. Restore the instance footprint, then restore persistent keys.

### 5.3 Restoration operation

Restoration is performed by submitting a `RestoreFootprint` operation with the archived ledger keys. The operation pays rent to re-establish the entries with a fresh TTL.

```rust
// Conceptual flow for a keeper written against the Soroban RPC / SDK.
// The exact SDK call names vary by language; the semantics are stable.

async fn restore_archived(contract_id: &str, keys: &[LedgerKey]) -> Result<()> {
    // 1. Build the restore footprint from the archived keys.
    let footprint = build_restore_footprint(keys);

    // 2. Simulate to obtain the rent fee and confirm the keys are archived.
    let sim = simulate_restore(contract_id, &footprint).await?;
    if !sim.is_archived() {
        return Ok(()); // already live; nothing to do
    }

    // 3. Submit the RestoreFootprint operation.
    let tx = build_restore_tx(contract_id, &footprint, sim.min_resource_fee);
    let result = submit_and_confirm(tx).await?;

    // 4. Verify the entries are live and have a non-zero TTL.
    for key in keys {
        let ttl = query_ttl(contract_id, key).await?;
        if ttl == 0 {
            return Err(anyhow!("restoration failed for key {:?}", key));
        }
    }
    Ok(())
}
```

### 5.4 Restoration runbook

1. **Confirm the incident.** Verify the entry is archived (TTL == 0) and not merely missing due to a logic bug.
2. **Freeze writes.** Pause any keeper or user traffic that would write to the affected contract to avoid partial state.
3. **Restore the instance first** if it is archived. Confirm the contract is reachable.
4. **Restore persistent keys in batches.** Keep batches small enough to fit in a single transaction's resource limits.
5. **Verify TTLs** on every restored key. Confirm they are above your threshold.
6. **Resume traffic** and let the normal TTL-bumping path take over.
7. **Post-incident review.** Identify why the keeper missed the entry and adjust thresholds or polling frequency.

### 5.5 Preventing restoration incidents

- Run the keeper with **redundancy** (at least two independent instances) and alert on missed runs.
- Alert when any entry's TTL drops below `2 × threshold`.
- Alert when the instance TTL drops below `2 × INSTANCE_TTL_THRESHOLD`.
- Test restoration in a staging network before relying on it in production.

---

## 6. Operational Checklists

### 6.1 Pre-deployment checklist

- [ ] Every storage key is assigned to the correct tier (temporary / instance / persistent).
- [ ] Instance TTL `extend_to` ≥ the longest persistent `extend_to` you rely on.
- [ ] Every state-touching entry point bumps the instance TTL.
- [ ] Every persistent read/write bumps the persistent key TTL.
- [ ] Temporary keys are treated as caches and can be rebuilt if lost.
- [ ] Rent cost has been estimated via simulation and budgeted.
- [ ] A keeper is configured with thresholds ≥ 2× its polling interval.
- [ ] Keeper key is stored in a secrets manager and has a rotation policy.
- [ ] Alerts are configured for TTL below `2 × threshold` and for missed keeper runs.
- [ ] Restoration procedure has been tested on a staging network.

### 6.2 Routine operations checklist (weekly)

- [ ] Review TTL dashboards for all contracts.
- [ ] Confirm keeper runs succeeded and no batches were skipped.
- [ ] Review rent spend against budget.
- [ ] Confirm no entries are within `2 × threshold` of expiry without a scheduled bump.

### 6.3 Incident checklist (archived state detected)

- [ ] Confirm the entry is archived (TTL == 0), not missing due to a bug.
- [ ] Freeze writes to the affected contract.
- [ ] Restore the instance footprint first if archived.
- [ ] Restore persistent keys in small batches.
- [ ] Verify TTLs on all restored keys.
- [ ] Resume traffic and confirm normal TTL bumping resumes.
- [ ] File a post-incident review with root cause and threshold adjustments.

---

## 7. Related Documentation

- [Soroban Documentation](https://developers.stellar.org/docs/smart-contracts) — official Soroban reference.
- [Stellar Developer Docs — State Archival](https://developers.stellar.org/docs/learn/encyclopedia/storage/state-archival) — network-level archival semantics.
- [Contract Deployment Guide](./deployment.md) — deployment and upgrade procedures.
- [Monitoring and Alerting](./monitoring.md) — observability setup for keepers and TTL dashboards.
- [Incident Response Runbook](./incident-response.md) — general incident handling.
- [Security Best Practices](./security.md) — key management and access control.

---

*Maintained by the platform team. Open an issue or PR against this repository to propose changes.*