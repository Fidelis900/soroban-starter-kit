#!/usr/bin/env bash
# Generates TypeScript bindings from deployed Soroban contracts using the
# Stellar CLI and writes them to the sdk/ directory.
#
# Reads contract IDs from .contract-ids (format written by deploy.sh):
#   name: CONTRACT_ID
set -euo pipefail

CONTRACT_IDS_FILE="${1:-.contract-ids}"
NETWORK="${STELLAR_NETWORK:-testnet}"
OUTPUT_DIR="sdk"

if [[ ! -f "$CONTRACT_IDS_FILE" ]]; then
  echo "ERROR: $CONTRACT_IDS_FILE not found. Deploy contracts first." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

# deploy.sh writes "name: CONTRACT_ID" (colon-space separated).
# Use IFS=': ' so that $name and $contract_id split correctly on that format.
while IFS=': ' read -r name contract_id || [[ -n "$name" ]]; do
  [[ "$name" =~ ^#.*$ || -z "$name" ]] && continue
  # Trim any residual whitespace that IFS may leave.
  name="${name#"${name%%[![:space:]]*}"}"
  name="${name%"${name##*[![:space:]]}"}"
  contract_id="${contract_id#"${contract_id%%[![:space:]]*}"}"
  contract_id="${contract_id%"${contract_id##*[![:space:]]}"}"

  [[ -z "$contract_id" ]] && continue

  echo "Generating TypeScript bindings for $name ($contract_id)..."

  stellar contract bindings typescript \
    --network "$NETWORK" \
    --id "$contract_id" \
    --output-dir "$OUTPUT_DIR/$name"

  echo "  → $OUTPUT_DIR/$name"
done < "$CONTRACT_IDS_FILE"

echo ""
echo "TypeScript bindings written to ./$OUTPUT_DIR/"
echo "Add 'import * as <name> from \"./$OUTPUT_DIR/<name>\"' in your project."
