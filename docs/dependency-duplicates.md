# Duplicate Dependency Audit

This document records the audit of duplicated transitive crates flagged by
`cargo tree -d`, why each remains, and how the duplicate gate is enforced.

Regenerate the raw report at any time with:

```bash
cargo tree -d
```

The duplicate gate itself is enforced in CI by the `deny` job
(`cargo-deny`, `[bans] multiple-versions = "deny"` in `deny.toml`).

## Audit method

A crate is only a *real* duplicate when two **different versions** are linked
into the same build graph. `cargo tree -d` also lists same-version entries that
appear twice because the crate is compiled for two targets (the host
build-script / proc-macro graph **and** the `wasm32-unknown-unknown` contract
target). Those are not version conflicts and cannot — and should not — be
"unified".

| Crate | Same version twice? | Cause | Action |
|-------|--------------------|-------|--------|
| `serde 1.0.228` | yes | host vs wasm target split | none — not a conflict |
| `num-integer 0.1.46` | yes | host vs wasm target split | none — not a conflict |
| `num-traits 0.2.19` | yes | host vs wasm target split | none — not a conflict |
| `serde_with 3.21.0` | yes | feature/target resolution | none — not a conflict |
| `soroban-env-common 21.2.1` | yes | feature/target resolution | none — not a conflict |
| `stellar-xdr 21.2.0` | yes | feature/target resolution | none — not a conflict |

## Real version conflicts

Three crate families are linked at multiple incompatible major versions. In
every case **one side is pinned by the `soroban-sdk` subtree**, which is locked
to an exact version (`soroban-sdk = "=21.7.7"`) for reproducible on-chain
builds, and the other side is required by a major-incompatible dev/build-time
tool. None can be unified without an upstream release of `soroban-sdk` (or a
major bump of the dev tooling), so each is recorded as an intentional,
justified skip in `deny.toml`.

| Crate | Versions | Pinned by (cannot move) | Other consumer | Scope |
|-------|----------|-------------------------|----------------|-------|
| `darling` (+ `darling_core`, `darling_macro`) | `0.20.11` / `0.23.0` | `soroban-sdk-macros` → `soroban-sdk` (needs 0.20) | `serde_with_macros` (needs 0.23) | build/proc-macro |
| `getrandom` | `0.2.17` / `0.3.4` / `0.4.3` | `k256` → `soroban-env-host` → `soroban-sdk` (needs 0.2 via `rand_core 0.6`) | `jobserver 0.1.34` (needs 0.3, build-time) and `tempfile` → `proptest` (needs 0.4, dev-only) | runtime (host) / build / dev |
| `itertools` | `0.10.5` / `0.11.0` | `soroban-builtin-sdk-macros` → `soroban-env-host` → `soroban-sdk` (needs 0.11) | `criterion` / `criterion-plot` (need 0.10) | build / dev (benches) |

### Why these cannot be unified

- **`darling`** — `soroban-sdk-macros` requires `0.20`; `serde_with_macros`
  (itself a transitive dep of `soroban-sdk`) requires `0.23`. Both live under
  the pinned `soroban-sdk` tree; neither can move independently.
- **`getrandom`** — the `0.2` line is reached through `soroban-env-host`'s
  cryptography stack (`k256` → `rand_core 0.6`). The `0.3` line is pulled in by
  `jobserver 0.1.34` (a build-time dependency of the `cc` crate used by native
  build scripts); it never ships in the WASM contract artifact. The `0.4` line
  only appears in dev/test builds via `tempfile` (a `proptest` dependency); it
  likewise never ships in the WASM contract artifact. All three are pinned by
  their respective upstreams and cannot be aligned without a release that
  moves `jobserver`/`cc` and `tempfile`/`proptest` onto the same `getrandom`
  major as the SDK's `rand_core 0.6` chain.
- **`itertools`** — the `0.11` line is required by `soroban-builtin-sdk-macros`
  inside the pinned SDK tree; the `0.10` line comes only from the `criterion`
  benchmark harness (`benches`, dev-only). Bumping `criterion` to a release that
  uses `itertools 0.11` is the only future path to unify, and is tracked as
  routine dev-tooling maintenance.

## Impact

All three conflicts are confined to **build-time, host-side, or dev/test**
dependency graphs. The deployed `wasm32-unknown-unknown` contract artifacts are
unaffected, so on-chain binary size does not change. The skips keep the
`multiple-versions = "deny"` gate green and intentional rather than silenced.

## Re-checking after an SDK upgrade

When `soroban-sdk` is upgraded (see
[CONTRIBUTING.md → Upgrading soroban-sdk](../CONTRIBUTING.md#upgrading-soroban-sdk)),
re-run `cargo tree -d` and prune any skip in `deny.toml` that the new SDK has
resolved.
