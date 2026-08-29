#!/usr/bin/env bash
# initialize.sh — run post-deploy initialization on deployed Soroban contracts
# Usage: ./scripts/initialize.sh [testnet|mainnet|local]
#
# Reads contract IDs from .contract-ids (format written by deploy.sh):
#   name: CONTRACT_ID
# then calls `stellar contract invoke --id <id> -- initialize` for each.
#
# Override per-contract init args via env:  INIT_ARGS_<NAME>="--arg val"
# Set INIT_FN to change the function name (default: initialize).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACT_IDS_FILE="${CONTRACT_IDS_FILE:-$ROOT/.contract-ids}"
NETWORK="${1:-testnet}"
INIT_FN="${INIT_FN:-initialize}"

case "$NETWORK" in
  testnet)
    RPC_URL="https://soroban-testnet.stellar.org"
    PASSPHRASE="Test SDF Network ; September 2015"
    ;;
  mainnet)
    RPC_URL="https://soroban.stellar.org"
    PASSPHRASE="Public Global Stellar Network ; September 2015"
    ;;
  local)
    RPC_URL="http://localhost:${LOCAL_RPC_PORT:-8000}"
    PASSPHRASE="Standalone Network ; February 2017"
    ;;
  *)
    echo "Unknown network: $NETWORK (use testnet|mainnet|local)" >&2
    exit 1
    ;;
esac

if [[ ! -f "$CONTRACT_IDS_FILE" ]]; then
  echo "No .contract-ids file found at $CONTRACT_IDS_FILE — nothing to initialize." >&2
  exit 1
fi

INITIALIZED=0
SKIPPED=0
FAILED=0

while IFS=': ' read -r name contract_id || [[ -n "$name" ]]; do
  # skip blank lines and comments
  [[ -z "$name" || "$name" == \#* ]] && continue
  # Trim any residual whitespace that IFS may leave.
  name="${name#"${name%%[![:space:]]*}"}"
  name="${name%"${name##*[![:space:]]}"}"
  contract_id="${contract_id#"${contract_id%%[![:space:]]*}"}"
  contract_id="${contract_id%"${contract_id##*[![:space:]]}"}"
  contract_id="${contract_id:-}"
  if [[ -z "$contract_id" ]]; then
    echo "WARN: no contract ID for '$name', skipping."
    ((SKIPPED++)) || true
    continue
  fi

  # per-contract extra args, e.g. INIT_ARGS_TOKEN="--admin GABC..."
  extra_var="INIT_ARGS_$(echo "$name" | tr '[:lower:]' '[:upper:]' | tr '-' '_')"
  extra_args="${!extra_var:-}"

  echo "── Initializing $name ($contract_id) ──"
  if stellar contract invoke \
      --id "$contract_id" \
      --rpc-url "$RPC_URL" \
      --network-passphrase "$PASSPHRASE" \
      --source-account "${SOURCE_ACCOUNT:-default}" \
      -- "$INIT_FN" $extra_args 2>&1 | tee /tmp/init_output.txt; then
    ((INITIALIZED++)) || true
  else
    # treat "already initialized" style errors as non-fatal
    if grep -qiE "already.init|AlreadyInitialized|HostError.*1" /tmp/init_output.txt; then
      echo "  → $name already initialized, skipping."
      ((SKIPPED++)) || true
    else
      echo "ERROR: initialization of $name failed." >&2
      ((FAILED++)) || true
    fi
  fi
done < "$CONTRACT_IDS_FILE"

echo ""
echo "Done — initialized: $INITIALIZED  skipped: $SKIPPED  failed: $FAILED"
[[ "$FAILED" -eq 0 ]] || exit 1
