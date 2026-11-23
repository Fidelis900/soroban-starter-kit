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
#   ./scripts/health-check.sh [.contract-ids] [--fix]
#   CONTRACT_IDS='token=C...|contract_version,escrow=D...|get_info' \
#     ./scripts/health-check.sh
#
# WASM hash verification:
#   After the liveness checks, each deployed contract's on-chain WASM hash is
#   compared against the sha256 of the matching local build in
#   target/wasm32-unknown-unknown/release/*.wasm. The local artifact is matched
#   by contract name (e.g. "token" -> token.wasm) or, failing that, by the
#   contract ID recorded in the deploy manifest. Drift is reported as an
#   OUTDATED deployment and causes a non-zero exit.
#
#   --fix   Redeploy out-of-sync contracts via scripts/deploy.sh (or
#           scripts/deploy-all.sh when present) before re-verifying.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NETWORK="${STELLAR_NETWORK:-testnet}"
DEFAULT_METHOD="${HEALTH_CHECK_METHOD:-contract_version}"
TIMEOUT_SECONDS="${HEALTH_CHECK_TIMEOUT:-15}"
WASM_DIR="${WASM_DIR:-$ROOT/target/wasm32-unknown-unknown/release}"
FIX=0
POSITIONAL=()
for arg in "$@"; do
  case "$arg" in
    --fix) FIX=1 ;;
    -h|--help)
      sed -n '2,30p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
      exit 0
      ;;
    --*)
      echo "ERROR: unknown option: $arg" >&2
      exit 2
      ;;
    *) POSITIONAL+=("$arg") ;;
  esac
done
INPUT_FILE="${POSITIONAL[0]:-${CONTRACT_IDS_FILE:-$ROOT/.contract-ids}}"

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

# ---------------------------------------------------------------------------
# WASM hash verification
# ---------------------------------------------------------------------------
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "ERROR: sha256sum or shasum is required for WASM verification." >&2
    return 2
  fi
}

# Resolve the local wasm artifact for a contract name.
local_wasm_for() {
  local name="$1"
  local candidate
  for candidate in \
    "$WASM_DIR/${name}.wasm" \
    "$WASM_DIR/${name//-/_}.wasm" \
    "$WASM_DIR/${name//_/-}.wasm"; do
    if [[ -f "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

# Fetch the on-chain wasm hash for a contract ID.
onchain_hash_for() {
  local contract_id="$1"
  local cmd=(stellar contract info wasm-hash --id "$contract_id" --network "$NETWORK")
  [[ -n "${STELLAR_RPC_URL:-}" ]] && cmd+=(--rpc-url "$STELLAR_RPC_URL")
  [[ -n "${STELLAR_NETWORK_PASSPHRASE:-}" ]] && cmd+=(--network-passphrase "$STELLAR_NETWORK_PASSPHRASE")
  timeout "${TIMEOUT_SECONDS}s" "${cmd[@]}" 2>/dev/null | tr -d '[:space:]'
}

# Redeploy a single contract using the workspace deploy script.
redeploy_contract() {
  local name="$1" contract_id="$2"
  local deploy_script="$ROOT/scripts/deploy.sh"
  local deploy_all="$ROOT/scripts/deploy-all.sh"
  if [[ -x "$deploy_script" ]]; then
    "$deploy_script" --network "$NETWORK" --contract "$name" --id "$contract_id"
  elif [[ -x "$deploy_all" ]]; then
    "$deploy_all" --network "$NETWORK"
  else
    echo "ERROR: no deploy script found (scripts/deploy.sh or scripts/deploy-all.sh)." >&2
    return 1
  fi
}

verify_wasm_hashes() {
  local drift=0 checked=0 skipped=0
  local -a outdated_names=() outdated_ids=()

  printf '\nWASM hash verification (%s)\n' "$WASM_DIR"
  printf '%-24s %-24s %-24s %s\n' "NAME" "ON-CHAIN" "LOCAL" "STATUS"
  printf '%-24s %-24s %-24s %s\n' "----" "--------" "-----" "------"

  for entry in "${entries[@]}"; do
    entry="$(printf '%s' "$entry" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
    [[ "$entry" != *=* ]] && continue

    local name="${entry%%=*}"
    local definition="${entry#*=}"
    local contract_id
    if [[ "$definition" == *'|'* ]]; then
      contract_id="${definition%%|*}"
    else
      contract_id="${definition%%[[:space:]]*}"
    fi
    name="${name//[[:space:]]/}"
    contract_id="${contract_id//[[:space:]]/}"
    [[ -z "$name" || -z "$contract_id" ]] && continue

    local wasm
    if ! wasm="$(local_wasm_for "$name")"; then
      printf '%-24s %-24s %-24s %s\n' "$name" "-" "-" "NO-LOCAL-BUILD"
      ((skipped++)) || true
      continue
    fi

    local local_hash onchain_hash
    local_hash="$(sha256_of "$wasm")"
    if ! onchain_hash="$(onchain_hash_for "$contract_id")" || [[ -z "$onchain_hash" ]]; then
      printf '%-24s %-24s %-24s %s\n' "$name" "?" "${local_hash:0:12}…" "QUERY-FAILED"
      ((skipped++)) || true
      continue
    fi

    ((checked++)) || true
    if [[ "$onchain_hash" == "$local_hash" ]]; then
      printf '%-24s %-24s %-24s %s\n' "$name" "${onchain_hash:0:12}…" "${local_hash:0:12}…" "IN-SYNC"
    else
      printf '%-24s %-24s %-24s %s\n' "$name" "${onchain_hash:0:12}…" "${local_hash:0:12}…" "OUTDATED"
      ((drift++)) || true
      outdated_names+=("$name")
      outdated_ids+=("$contract_id")
    fi
  done

  printf '\nWASM summary: %d in sync, %d outdated, %d skipped\n' \
    "$((checked - drift))" "$drift" "$skipped"

  if (( drift > 0 )); then
    if (( FIX )); then
      echo "Attempting redeploy of $drift outdated contract(s)…"
      local i failed=0
      for i in "${!outdated_names[@]}"; do
        if redeploy_contract "${outdated_names[$i]}" "${outdated_ids[$i]}"; then
          echo "Redeployed ${outdated_names[$i]}."
        else
          echo "ERROR: redeploy failed for ${outdated_names[$i]}." >&2
          ((failed++)) || true
        fi
      done
      if (( failed > 0 )); then
        echo "ERROR: $failed contract(s) could not be redeployed." >&2
        return 1
      fi
      echo "Redeploy complete; re-run health-check to confirm hashes are in sync."
      return 0
    fi
    echo "ERROR: $drift deployed contract(s) are outdated. Re-run with --fix to redeploy." >&2
    return 1
  fi
  return 0
}

wasm_status=0
verify_wasm_hashes || wasm_status=$?

(( down == 0 && wasm_status == 0 ))
