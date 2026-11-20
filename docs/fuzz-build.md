# Building the Fuzz Crate

Issue: [#1363](https://github.com/Fidelis900/soroban-starter-kit/issues/1363)

## Status

**The path in the issue is fixed.** In `fuzz/Cargo.toml`, the `multisig` dependency used to point at
`path = ".."`, the workspace root. That directory holds a virtual manifest, so Cargo failed with:

```
error: failed to get `multisig` as a dependency ... found a virtual manifest instead of a package manifest
```

Commit `9acb5bd3` changed it to:

```toml
[dependencies.soroban-multisig-template]
path = "../contracts/multisig"
```

All contract dependencies of `fuzz` now point at `../contracts/<name>`.

## Still open: workspace membership

`fuzz` is not in the root `Cargo.toml` `workspace.members`, and it isn't excluded either. Running
Cargo inside `fuzz/` still fails:

```
$ cd fuzz && cargo metadata --no-deps
error: current package believes it's in a workspace when it's not
...
this may be fixable by adding `fuzz` to the `workspace.members` array ...
```

`fuzz` uses `soroban-sdk = { workspace = true }`, so it must join the workspace. Giving it its own
empty `[workspace]` table would not work. The fix:

```toml
# Cargo.toml (root)
[workspace]
members = [
  # ...
  "fuzz",
]
```

`cargo-fuzz` builds with nightly and sanitizer flags. Joining the workspace doesn't change that,
because `cargo fuzz run <target>` still drives the build.

## Acceptance check

```sh
cargo check --package fuzz          # resolves dependencies and type-checks
cargo +nightly fuzz build           # optional, requires cargo-fuzz
```

> Note: `fuzz` also depends on `crowdfund`, which doesn't compile on `main` right now (see the
> "Known gaps" section of [ADR 0014](adr/0014-crowdfund-soft-hard-caps.md)). `cargo check --package fuzz`
> fails on that until the crowdfund crate is fixed, even with the manifest correct.
