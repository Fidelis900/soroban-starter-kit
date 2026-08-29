#!/usr/bin/env bash
# Reads .contract-ids and checks whether each deployed contract is alive,
# expired (TTL), or unreachable.
#
# .contract-ids format (written by scripts/deploy.sh):
#   <name>: <CONTRACT_ID>
#
# The health probe calls `contract_version`, which is present on every contract
# in this workspace. It is a read-only query that requires no auth and returns
# a u32, so a successful response confirms the contract is alive and its
# instance storage has not expired.
set -euo pipefail

CONTRACT_IDS_FILE="${1:-.contract-ids}"
NETWORK="${STELLAR_NETWORK:-testnet}"

if [[ ! -f "$CONTRACT_IDS_FILE" ]]; then
  echo "ERROR: $CONTRACT_IDS_FILE not found." >&2
  exit 1
fi

alive=()
expired=()
unreachable=()

# deploy.sh writes "name: CONTRACT_ID" (colon-space separated).
# Use IFS=': ' so that $name and $contract_id split correctly on that format.
while IFS=': ' read -r name contract_id || [[ -n "$name" ]]; do
  [[ "$name" =~ ^#.*$ || -z "$name" ]] && continue
  # Trim any residual whitespace that IFS may leave.
  name="${name#"${name%%[![:space:]]*}"}"
  name="${name%"${name##*[![:space:]]}"}"
  contract_id="${contract_id#"${contract_id%%[![:space:]]*}"}"
  contract_id="${contract_id%"${contract_id##*[![:space:]]}"}"

  # Skip lines where the contract ID was not parsed (e.g. blank lines).
  [[ -z "$contract_id" ]] && continue

  echo "Checking $name ($contract_id)..."

  # contract_version is a universal read-only query present on every contract
  # in this workspace. It requires no auth and returns a u32 version counter,
  # confirming the contract is initialized and its instance TTL has not expired.
  output=$(stellar contract invoke \
    --id "$contract_id" \
    --network "$NETWORK" \
    -- contract_version 2>&1 || true)

  if echo "$output" | grep -qi "ttl\|expired\|entry has expired"; then
    expired+=("$name ($contract_id)")
  elif echo "$output" | grep -qi "error\|not found\|unreachable\|failed\|unknown function\|invalid"; then
    unreachable+=("$name ($contract_id)")
  else
    alive+=("$name ($contract_id)")
  fi
done < "$CONTRACT_IDS_FILE"

echo ""
echo "=== Contract Health Report ==="
echo ""
echo "ALIVE (${#alive[@]}):"
for c in "${alive[@]+"${alive[@]}"}"; do echo "  ✓ $c"; done

echo ""
echo "EXPIRED TTL (${#expired[@]}):"
for c in "${expired[@]+"${expired[@]}"}"; do echo "  ⚠ $c"; done

echo ""
echo "UNREACHABLE (${#unreachable[@]}):"
for c in "${unreachable[@]+"${unreachable[@]}"}"; do echo "  ✗ $c"; done

if [[ ${#expired[@]} -gt 0 || ${#unreachable[@]} -gt 0 ]]; then
  exit 1
fi
