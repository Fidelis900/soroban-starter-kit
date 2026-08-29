# ADR-0003: Admin Model

- **Status**: Accepted (amended 2026-08-29)
- **Date**: 2024-04-24

## Context

Both contracts need a privileged role that can perform sensitive operations (minting tokens, resolving disputes). The design must:

- Be simple to reason about.
- Avoid a single point of failure where possible.
- Leverage Soroban's native auth framework rather than rolling a custom signature scheme.

## Decision

### Token contract — single admin address

A single `Admin` address is stored in instance storage under `AdminKey::Admin` (defined in the shared `soroban-common` crate).

```rust
// soroban-common
pub fn try_get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&AdminKey::Admin)
}

// token/src/admin.rs
pub fn require_admin(env: &Env) -> Result<Address, TokenError> {
    soroban_common::try_get_admin(env).ok_or(TokenError::NotInitialized)
}
```

The admin address is set once during `initialize` and can be a regular account or a multisig contract address — the token contract is agnostic to the type of address.

Privileged operations (mint, burn, set_admin) call `admin.require_auth()`, delegating all authentication to the Soroban host.

**Two-step admin transfer (added post-initial ADR):** To prevent permanent admin lockout caused by a typo'd address, the token contract implements a propose / accept protocol in addition to the deprecated single-step `set_admin`:

```rust
pub fn propose_admin(env: Env, new_admin: Address) -> Result<(), TokenError> { ... }
pub fn accept_admin(env: Env) -> Result<(), TokenError> { ... }
pub fn cancel_admin_proposal(env: Env) -> Result<(), TokenError> { ... }
```

`propose_admin` stores the candidate in `DataKey::PendingAdmin`. The candidate must call `accept_admin` to complete the transfer; until then the current admin retains control and can call `cancel_admin_proposal` to abort. `set_admin` (single-step, no confirmation) is deprecated and retained only for backward compatibility.

### Escrow contract — role-based parties, no global admin

The escrow has no "admin" in the traditional sense. Instead, three distinct roles are established at initialisation:

| Role | Stored key | Privileged operations |
|------|-----------|----------------------|
| `buyer` | `Buyer` | `fund`, `approve_delivery`, `request_refund`, `release_partial`, `cancel` |
| `seller` | `Seller` | `mark_delivered` |
| `arbiter` | `Arbiter` | `resolve_dispute` |

Each function reads the relevant address from storage and calls `.require_auth()` on it. There is no super-admin that can override all roles.

### Shared admin utilities in `soroban-common`

To avoid duplicating admin-read logic, the `soroban-common` crate exposes `get_admin` / `try_get_admin`. This is the single source of truth for the `AdminKey` enum and its storage layout.

## Consequences

- The token admin is a single address; operators who need multi-party control should deploy a multisig contract as the admin address.
- The escrow arbiter provides dispute resolution without granting blanket admin power.
- Admin transfer for the token contract uses a **two-step propose / accept flow** to guard against permanent lockout from a typo'd address:
  - `propose_admin(new_admin)` — current admin nominates a successor and stores the pending address.
  - `accept_admin()` — the nominated address must actively accept, confirming it controls the key.
  - `cancel_admin_proposal()` — current admin can retract a pending proposal before it is accepted.
  - The older `set_admin(new_admin)` function (single-step, send-and-done) is **deprecated**; it remains for backward compatibility but should not be used in new integrations. A typo'd address passed to `set_admin` would permanently lock the admin role with no recovery path — the two-step flow was introduced specifically to eliminate this footgun.
- Removing the admin from the token contract (setting it to a burn address via `set_admin`) effectively makes the token immutable.
