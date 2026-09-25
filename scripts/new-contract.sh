#!/usr/bin/env bash
# new-contract.sh — scaffold a new Soroban contract from the common skeleton
# Usage: ./scripts/new-contract.sh <contract-name>
#   <contract-name>  Kebab-case name (e.g. "my-token", "price-feed")
#                    Creates contracts/<contract-name>/ and registers it in the workspace.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log()  { echo -e "\033[1;34m[new-contract]\033[0m $*"; }
ok()   { echo -e "\033[1;32m[ok]\033[0m $*"; }
die()  { echo -e "\033[1;31m[error]\033[0m $*" >&2; exit 1; }

# ── Argument validation ───────────────────────────────────────────────────────
[[ $# -ge 1 ]] || die "Usage: $0 <contract-name>  (e.g. my-token)"

NAME="$1"

# Must be lowercase letters/digits/hyphens, starting with a letter
[[ "$NAME" =~ ^[a-z][a-z0-9-]*$ ]] \
  || die "Name must be lowercase alphanumeric with hyphens and start with a letter (got: '$NAME')"

DEST="$ROOT/contracts/$NAME"

[[ ! -e "$DEST" ]] \
  || die "contracts/$NAME already exists — choose a different name or remove it first"

# ── Scaffold ──────────────────────────────────────────────────────────────────
log "Scaffolding contracts/$NAME ..."

mkdir -p "$DEST/src"

# Cargo.toml — cdylib for WASM deployment, rlib for test-linking
cat > "$DEST/Cargo.toml" <<TOML
[package]
name = "soroban-${NAME}"
version = "0.1.0"
edition = "2024"
authors.workspace = true
description = "${NAME} Soroban contract"
license.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
soroban-sdk = { workspace = true }
soroban-common = { workspace = true }

[dev-dependencies]
soroban-sdk = { workspace = true, features = ["testutils"] }

[lints]
workspace = true
TOML

ok "Wrote contracts/$NAME/Cargo.toml"

# src/lib.rs — minimal compilable contract; replace hello() with real logic
cat > "$DEST/src/lib.rs" <<'RUST'
#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Env, Symbol};

#[contract]
pub struct Contract;

#[contractimpl]
impl Contract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("hello")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn hello_returns_symbol() {
        let env = Env::default();
        let id = env.register(Contract, ());
        let client = ContractClient::new(&env, &id);
        assert_eq!(client.hello(), symbol_short!("hello"));
    }
}
RUST

ok "Wrote contracts/$NAME/src/lib.rs"

# ── Register in workspace ─────────────────────────────────────────────────────
log "Adding contracts/$NAME to workspace ..."

# Insert the new member before "tests" (portable awk, works on Linux and macOS)
awk -v name="$NAME" '
  /^  "tests",$/ { print "  \"contracts/" name "\"," }
  { print }
' "$ROOT/Cargo.toml" > "$ROOT/Cargo.toml.tmp" \
  && mv "$ROOT/Cargo.toml.tmp" "$ROOT/Cargo.toml"

ok "Updated Cargo.toml workspace members"

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
ok "Contract scaffolded at contracts/$NAME"
echo ""
echo "  Next steps:"
echo "    cargo check -p soroban-${NAME}                  — verify it compiles"
echo "    cargo test  -p soroban-${NAME}                  — run tests"
echo "    cd contracts/${NAME} && stellar contract build   — build WASM"
echo ""
