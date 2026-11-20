#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LIMIT_BYTES=65536
JSON_OUT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --limit-bytes) [[ $# -ge 2 ]] || { echo "--limit-bytes requires a value" >&2; exit 2; }; LIMIT_BYTES="$2"; shift 2 ;;
    --json-out) [[ $# -ge 2 ]] || { echo "--json-out requires a path" >&2; exit 2; }; JSON_OUT="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ "$LIMIT_BYTES" =~ ^[1-9][0-9]*$ ]] || { echo "Invalid byte limit: $LIMIT_BYTES" >&2; exit 2; }
for tool in stellar cargo rustup python3; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 1; }
done

TARGET="wasm32v1-none"
installed_targets="$(rustup target list --installed)"
grep -Fxq "$TARGET" <<< "$installed_targets" || {
  echo "Rust target $TARGET is not installed" >&2
  exit 1
}

metadata="$(cargo metadata --format-version 1 --no-deps)"
mapfile -t contracts < <(python3 -c '
import json, sys
data = json.load(sys.stdin)
root = data["workspace_root"] + "/contracts/"
for package in sorted(data["packages"], key=lambda p: p["name"]):
    if not package["manifest_path"].startswith(root):
        continue
    target = next((t for t in package["targets"] if "cdylib" in t["crate_types"]), None)
    if target:
        print(package["manifest_path"] + "\t" + target["name"] + "\t" + package["name"])
' <<< "$metadata")

if (( ${#contracts[@]} == 0 )); then
  echo "No contract cdylib targets found under contracts/" >&2
  exit 1
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
results="$tmpdir/results.tsv"
: > "$results"
failed=0

printf '%-32s %12s %12s\n' "PACKAGE" "OPTIMIZED" "BUDGET"
for entry in "${contracts[@]}"; do
  IFS=$'\t' read -r manifest target package <<< "$entry"
  echo "::group::Build and optimize $package"
  if ! stellar contract build --manifest-path "$manifest"; then
    echo "ERROR: contract build failed for $package" >&2
    failed=1
    echo "::endgroup::"
    continue
  fi

  wasm="$ROOT/target/$TARGET/release/$target.wasm"
  if [[ ! -s "$wasm" ]]; then
    echo "ERROR: expected WASM artifact missing or empty: $wasm" >&2
    failed=1
    echo "::endgroup::"
    continue
  fi

  optimized="$tmpdir/$target.optimized.wasm"
  if ! stellar contract optimize --wasm "$wasm" --wasm-out "$optimized"; then
    echo "ERROR: WASM optimization failed for $package" >&2
    failed=1
    echo "::endgroup::"
    continue
  fi
  if [[ ! -s "$optimized" ]]; then
    echo "ERROR: optimizer produced no artifact for $package" >&2
    failed=1
    echo "::endgroup::"
    continue
  fi

  bytes="$(wc -c < "$optimized" | tr -d ' ')"
  printf '%s\t%s\n' "$package" "$bytes" >> "$results"
  printf '%-32s %9s B %9s B\n' "$package" "$bytes" "$LIMIT_BYTES"
  if (( bytes > LIMIT_BYTES )); then
    echo "ERROR: $package optimized WASM is $bytes bytes; maximum is $LIMIT_BYTES bytes" >&2
    failed=1
  fi
  echo "::endgroup::"
done

if [[ -n "$JSON_OUT" ]]; then
  python3 - "$JSON_OUT" "$LIMIT_BYTES" "$results" <<'PY'
import json
import sys
path, limit, result_path = sys.argv[1:]
contracts = {}
with open(result_path, encoding="utf-8") as source:
    for line in source:
        name, size = line.rstrip("\n").split("\t")
        contracts[name] = int(size)
with open(path, "w", encoding="utf-8") as output:
    json.dump({"limit_bytes": int(limit), "contracts": contracts}, output, indent=2, sort_keys=True)
    output.write("\n")
PY
fi

if (( failed != 0 )); then
  echo "Contract size check failed." >&2
  exit 1
fi

echo "All ${#contracts[@]} optimized contract artifacts are within the ${LIMIT_BYTES}-byte budget."
