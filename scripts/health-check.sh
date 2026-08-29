#!/usr/bin/env bash
# health-check.sh — verify deployed Soroban contracts respond to read-only calls.
#
# Supported input formats (one contract per line):
#   name=CONTRACT_ID|METHOD
#   name=CONTRACT_ID METHOD
#
# If METHOD is omitted, HEALTH_CHECK_METHOD (default: contract_version) is used.
# contract_version is a universal read-only query present on every contract in
# this workspace — it requires no auth and returns a u32, confirming the
# contract is alive. Override with HEALTH_CHECK_METHOD for contracts that expose
# a cheaper or more representative read-only entry point.
#
# Usage:
#   ./scripts/health-check.sh [.contract-ids]
#   CONTRACT_IDS='token=C...|contract_version,escrow=D...|get_info' \
#     ./scripts/health-check.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NETWORK="${STELLAR_NETWORK:-testnet}"
DEFAULT_METHOD="${HEALTH_CHECK_METHOD:-contract_version}"
TIMEOUT_SECONDS="${HEALTH_CHECK_TIMEOUT:-15}"
INPUT_FILE="${1:-${CONTRACT_IDS_FILE:-$ROOT/.contract-ids}}"

if ! command -v stellar >/dev/null 2>&1; then
  echo "ERROR: stellar CLI is required but was not found in PATH." >&2
  exit 2
fi

if ! [[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]]; then
  echo "ERROR: HEALTH_CHECK_TIMEOUT must be a positive integer." >&2
  exit 2
fi

entries=()
if [[ -n "${CONTRACT_IDS:-}" ]]; then
  IFS=',' read -r -a entries <<< "$CONTRACT_IDS"
elif [[ -f "$INPUT_FILE" ]]; then
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%#*}"
    [[ -z "${line//[[:space:]]/}" ]] && continue
    entries+=("$line")
  done < "$INPUT_FILE"
else
  echo "ERROR: no contract definitions found. Provide $INPUT_FILE or CONTRACT_IDS." >&2
  exit 2
fi

if [[ ${#entries[@]} -eq 0 ]]; then
  echo "ERROR: no contract definitions found." >&2
  exit 2
fi

up=0
down=0
printf 'Contract health check (%s)\n' "$NETWORK"
printf '%-24s %-58s %-24s %s\n' "NAME" "CONTRACT ID" "METHOD" "STATUS"
printf '%-24s %-58s %-24s %s\n' "----" "-----------" "------" "------"

for entry in "${entries[@]}"; do
  entry="$(printf '%s' "$entry" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
  if [[ "$entry" != *=* ]]; then
    echo "WARN: ignoring malformed entry (expected name=id|method): $entry" >&2
    ((down++)) || true
    continue
  fi

  name="${entry%%=*}"
  definition="${entry#*=}"
  if [[ "$definition" == *'|'* ]]; then
    contract_id="${definition%%|*}"
    method="${definition#*|}"
  else
    contract_id="${definition%%[[:space:]]*}"
    method="${definition#"$contract_id"}"
    method="$(printf '%s' "$method" | sed -E 's/^[[:space:]]+//')"
  fi
  method="${method:-$DEFAULT_METHOD}"
  name="${name//[[:space:]]/}"
  contract_id="${contract_id//[[:space:]]/}"

  if [[ -z "$name" || -z "$contract_id" || -z "$method" ]]; then
    echo "WARN: ignoring incomplete entry: $entry" >&2
    ((down++)) || true
    continue
  fi

  cmd=(stellar contract invoke --id "$contract_id" --network "$NETWORK")
  [[ -n "${STELLAR_RPC_URL:-}" ]] && cmd+=(--rpc-url "$STELLAR_RPC_URL")
  [[ -n "${STELLAR_NETWORK_PASSPHRASE:-}" ]] && cmd+=(--network-passphrase "$STELLAR_NETWORK_PASSPHRASE")
  cmd+=(-- "$method")

  if output=$(timeout "${TIMEOUT_SECONDS}s" "${cmd[@]}" 2>&1); then
    printf '%-24s %-58s %-24s %s\n' "$name" "$contract_id" "$method" "UP"
    ((up++)) || true
  else
    printf '%-24s %-58s %-24s %s\n' "$name" "$contract_id" "$method" "DOWN"
    printf '  %s\n' "$output" >&2
    ((down++)) || true
  fi
done

printf '\nSummary: %d up, %d down\n' "$up" "$down"
(( down == 0 ))
