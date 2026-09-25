#!/usr/bin/env bash
# Verifies every contract crate declares the same Rust edition, so scaffolding
# drift (e.g. scripts/new-contract.sh emitting a stale default) is caught in CI
# instead of surfacing as a per-contract inconsistency later.
set -euo pipefail

EXPECTED_EDITION="2024"
mismatches=()

for toml in contracts/*/Cargo.toml; do
  name=$(basename "$(dirname "$toml")")
  edition=$(grep -m1 '^edition' "$toml" | sed -E 's/edition\s*=\s*"([^"]+)"/\1/')
  if [[ "$edition" != "$EXPECTED_EDITION" ]]; then
    mismatches+=("$name: edition = \"$edition\" (expected \"$EXPECTED_EDITION\")")
  fi
done

if [[ ${#mismatches[@]} -gt 0 ]]; then
  echo "ERROR: Rust edition mismatch across contract workspace:" >&2
  for m in "${mismatches[@]}"; do echo "  - $m" >&2; done
  exit 1
fi

echo "All contract crates declare edition = \"$EXPECTED_EDITION\"."
