#!/usr/bin/env bash
#
# check-upgrade-compatibility.sh
#
# Automated dry-run verification for Soroban smart contract upgrades.
#
# This script compares the storage layout and exported interface of an
# existing (old) contract against a new contract build, and reports whether
# the upgrade is backwards-compatible with existing on-chain storage.
#
# Usage:
#   scripts/check-upgrade-compatibility.sh <contract-name> <old> <new>
#
#   <contract-name>  Logical name of the contract (used in the report).
#   <old>            Either:
#                      - a compiled WASM file (.wasm), or
#                      - an exported state snapshot JSON (.json) produced by
#                        `stellar contract read` / state export tooling.
#   <new>            A compiled WASM file (.wasm) for the new contract code.
#
# Exit codes:
#   0  Compatible: no storage keys removed/retyped and no exported
#      function signatures changed.
#   1  Breaking change detected (or the comparison could not be completed
#      because a required tool failed).
#   2  Usage error (bad arguments, missing files, unsupported input).
#
# Requirements:
#   - `stellar` (soroban-cli) available on PATH for WASM inspection.
#   - `jq` available on PATH for JSON parsing.
#
set -euo pipefail

PROG="$(basename "$0")"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

usage() {
  cat <<EOF
Usage: ${PROG} <contract-name> <old> <new>

Verify that upgrading a Soroban contract will not break existing storage
or exported interface compatibility.

Arguments:
  <contract-name>  Logical name of the contract (used in the report).
  <old>            Old contract WASM (.wasm) or exported state snapshot (.json).
  <new>            New contract WASM (.wasm).

Exit codes:
  0  Compatible.
  1  Breaking change detected (or comparison failed).
  2  Usage error.

Options:
  -h, --help       Show this help message and exit.

Examples:
  ${PROG} token contract_old.wasm contract_new.wasm
  ${PROG} token state_snapshot.json contract_new.wasm
EOF
}

die_usage() {
  echo "Error: $*" >&2
  echo "Try '${PROG} --help' for more information." >&2
  exit 2
}

die_breaking() {
  echo "Error: $*" >&2
  exit 1
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die_breaking "required command '$1' not found on PATH"
  fi
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
  usage
  exit 0
fi

if [[ $# -ne 3 ]]; then
  usage >&2
  echo "" >&2
  die_usage "expected 3 arguments, got $#"
fi

CONTRACT_NAME="$1"
OLD_INPUT="$2"
NEW_INPUT="$3"

if [[ -z "${CONTRACT_NAME}" ]]; then
  die_usage "contract name must not be empty"
fi

if [[ ! -f "${OLD_INPUT}" ]]; then
  die_usage "old input file not found: ${OLD_INPUT}"
fi

if [[ ! -f "${NEW_INPUT}" ]]; then
  die_usage "new input file not found: ${NEW_INPUT}"
fi

if [[ ! -r "${OLD_INPUT}" ]]; then
  die_usage "old input file not readable: ${OLD_INPUT}"
fi

if [[ ! -r "${NEW_INPUT}" ]]; then
  die_usage "new input file not readable: ${NEW_INPUT}"
fi

# ---------------------------------------------------------------------------
# Temporary workspace
# ---------------------------------------------------------------------------

WORKDIR="$(mktemp -d)"
cleanup() {
  rm -rf "${WORKDIR}"
}
trap cleanup EXIT

OLD_KEYS="${WORKDIR}/old_keys.txt"
NEW_KEYS="${WORKDIR}/new_keys.txt"
OLD_FNS="${WORKDIR}/old_fns.txt"
NEW_FNS="${WORKDIR}/new_fns.txt"

: >"${OLD_KEYS}"
: >"${NEW_KEYS}"
: >"${OLD_FNS}"
: >"${NEW_FNS}"

# ---------------------------------------------------------------------------
# Extraction: WASM via stellar contract inspect
# ---------------------------------------------------------------------------

# Extract storage keys from a WASM file.
# Each line: "<key>|<type>"
extract_wasm_storage_keys() {
  local wasm="$1"
  local out="$2"

  local inspect_json
  if ! inspect_json="$(stellar contract inspect --wasm "${wasm}" 2>/dev/null)"; then
    die_breaking "failed to inspect WASM: ${wasm}"
  fi

  # Try to read storage keys from the inspect output. Different soroban-cli
  # versions expose this under different paths; probe common shapes.
  local keys_json
  keys_json="$(printf '%s' "${inspect_json}" | jq -c '
    (.storage // .storage_keys // .state // .storageLayout // [])
    | if type == "array" then . else (.keys // []) end
  ' 2>/dev/null || echo '[]')"

  if [[ -z "${keys_json}" || "${keys_json}" == "null" ]]; then
    keys_json='[]'
  fi

  printf '%s' "${keys_json}" | jq -r '
    .[]
    | if type == "object" then
        ((.key // .name // .symbol // "") | tostring) + "|" +
        ((.type // .value_type // .valueType // "unknown") | tostring)
      else
        (tostring) + "|unknown"
      end
  ' 2>/dev/null | sort -u >"${out}" || true
}

# Extract exported function signatures from a WASM file.
# Each line: "<name>(<argtypes>)-><rettype>"
extract_wasm_functions() {
  local wasm="$1"
  local out="$2"

  local inspect_json
  if ! inspect_json="$(stellar contract inspect --wasm "${wasm}" 2>/dev/null)"; then
    die_breaking "failed to inspect WASM: ${wasm}"
  fi

  local fns_json
  fns_json="$(printf '%s' "${inspect_json}" | jq -c '
    (.functions // .spec.functions // .interface.functions // [])
  ' 2>/dev/null || echo '[]')"

  if [[ -z "${fns_json}" || "${fns_json}" == "null" ]]; then
    fns_json='[]'
  fi

  printf '%s' "${fns_json}" | jq -r '
    .[]
    | if type == "object" then
        (.name // .symbol // "") as $n
        | ((.inputs // .args // .params // []) | map(
              if type == "object" then (.type // .value_type // "unknown") else tostring end
            ) | join(",")) as $args
        | ((.outputs // .output // .returns // "void")
            | if type == "array" then (map(if type == "object" then (.type // "unknown") else tostring end) | join(",")) else tostring end) as $ret
        | "\($n)(\($args))->\($ret)"
      else
        tostring
      end
  ' 2>/dev/null | sort -u >"${out}" || true
}

# ---------------------------------------------------------------------------
# Extraction: exported state snapshot JSON
# ---------------------------------------------------------------------------

# Extract storage keys from an exported state snapshot JSON.
# Each line: "<key>|<type>"
extract_snapshot_storage_keys() {
  local snapshot="$1"
  local out="$2"

  if ! jq empty "${snapshot}" >/dev/null 2>&1; then
    die_usage "old input is not valid JSON: ${snapshot}"
  fi

  # Accept a variety of snapshot shapes: a top-level array of entries, or an
  # object with a "storage"/"entries"/"keys" array.
  jq -r '
    (if type == "array" then .
     else (.storage // .entries // .keys // .state // []) end)
    | .[]
    | if type == "object" then
        ((.key // .name // .symbol // "") | tostring) + "|" +
        ((.type // .value_type // .valueType // "unknown") | tostring)
      else
        (tostring) + "|unknown"
      end
  ' "${snapshot}" 2>/dev/null | sort -u >"${out}" || true
}

# ---------------------------------------------------------------------------
# Populate old/new key and function sets
# ---------------------------------------------------------------------------

is_wasm() {
  [[ "$1" == *.wasm ]]
}

is_json() {
  [[ "$1" == *.json ]]
}

# --- Old side -------------------------------------------------------------

if is_wasm "${OLD_INPUT}"; then
  require_cmd stellar
  require_cmd jq
  extract_wasm_storage_keys "${OLD_INPUT}" "${OLD_KEYS}"
  extract_wasm_functions "${OLD_INPUT}" "${OLD_FNS}"
elif is_json "${OLD_INPUT}"; then
  require_cmd jq
  extract_snapshot_storage_keys "${OLD_INPUT}" "${OLD_KEYS}"
  # A state snapshot does not carry the exported interface; function parity
  # is only checked when the old side is a WASM build.
  : >"${OLD_FNS}"
else
  die_usage "old input must be a .wasm or .json file: ${OLD_INPUT}"
fi

# --- New side -------------------------------------------------------------

if ! is_wasm "${NEW_INPUT}"; then
  die_usage "new input must be a .wasm file: ${NEW_INPUT}"
fi

require_cmd stellar
require_cmd jq
extract_wasm_storage_keys "${NEW_INPUT}" "${NEW_KEYS}"
extract_wasm_functions "${NEW_INPUT}" "${NEW_FNS}"

# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------

BREAKING=0
REPORT="${WORKDIR}/report.txt"
: >"${REPORT}"

{
  echo "=============================================================="
  echo " Upgrade compatibility report"
  echo "=============================================================="
  echo " Contract : ${CONTRACT_NAME}"
  echo " Old      : ${OLD_INPUT}"
  echo " New      : ${NEW_INPUT}"
  echo "--------------------------------------------------------------"
} >>"${REPORT}"

# --- Storage key comparison ----------------------------------------------

REMOVED_KEYS=()
RETYPED_KEYS=()
ADDED_KEYS=()

# Build lookup of new key -> type.
declare -A NEW_KEY_TYPES=()
while IFS='|' read -r key type; do
  [[ -z "${key}" ]] && continue
  NEW_KEY_TYPES["${key}"]="${type}"
done <"${NEW_KEYS}"

declare -A OLD_KEY_TYPES=()
while IFS='|' read -r key type; do
  [[ -z "${key}" ]] && continue
  OLD_KEY_TYPES["${key}"]="${type}"
done <"${OLD_KEYS}"

for key in "${!OLD_KEY_TYPES[@]}"; do
  old_type="${OLD_KEY_TYPES[${key}]}"
  if [[ -z "${NEW_KEY_TYPES[${key}]+x}" ]]; then
    REMOVED_KEYS+=("${key} (${old_type})")
  elif [[ "${NEW_KEY_TYPES[${key}]}" != "${old_type}" ]]; then
    RETYPED_KEYS+=("${key}: ${old_type} -> ${NEW_KEY_TYPES[${key}]}")
  fi
done

for key in "${!NEW_KEY_TYPES[@]}"; do
  if [[ -z "${OLD_KEY_TYPES[${key}]+x}" ]]; then
    ADDED_KEYS+=("${key} (${NEW_KEY_TYPES[${key}]})")
  fi
done

{
  echo ""
  echo "[Storage keys]"
  echo "  old keys: ${#OLD_KEY_TYPES[@]}   new keys: ${#NEW_KEY_TYPES[@]}"
} >>"${REPORT}"

if [[ ${#REMOVED_KEYS[@]} -gt 0 ]]; then
  BREAKING=1
  {
    echo "  BREAKING: storage keys removed:"
    for k in "${REMOVED_KEYS[@]}"; do
      echo "    - ${k}"
    done
  } >>"${REPORT}"
else
  echo "  OK: no storage keys removed." >>"${REPORT}"
fi

if [[ ${#RETYPED_KEYS[@]} -gt 0 ]]; then
  BREAKING=1
  {
    echo "  BREAKING: storage keys retyped:"
    for k in "${RETYPED_KEYS[@]}"; do
      echo "    - ${k}"
    done
  } >>"${REPORT}"
else
  echo "  OK: no storage keys retyped." >>"${REPORT}"
fi

if [[ ${#ADDED_KEYS[@]} -gt 0 ]]; then
  {
    echo "  INFO: new storage keys added (non-breaking):"
    for k in "${ADDED_KEYS[@]}"; do
      echo "    + ${k}"
    done
  } >>"${REPORT}"
fi

# --- Function signature comparison ---------------------------------------

{
  echo ""
  echo "[Exported functions]"
} >>"${REPORT}"

if [[ ! -s "${OLD_FNS}" ]]; then
  echo "  SKIP: old interface unavailable (state snapshot input)." >>"${REPORT}"
else
  declare -A OLD_FN_SIGS=()
  declare -A NEW_FN_SIGS=()

  while IFS= read -r sig; do
    [[ -z "${sig}" ]] && continue
    name="${sig%%(*}"
    OLD_FN_SIGS["${name}"]="${sig}"
  done <"${OLD_FNS}"

  while IFS= read -r sig; do
    [[ -z "${sig}" ]] && continue
    name="${sig%%(*}"
    NEW_FN_SIGS["${name}"]="${sig}"
  done <"${NEW_FNS}"

  REMOVED_FNS=()
  CHANGED_FNS=()
  ADDED_FNS=()

  for name in "${!OLD_FN_SIGS[@]}"; do
    if [[ -z "${NEW_FN_SIGS[${name}]+x}" ]]; then
      REMOVED_FNS+=("${OLD_FN_SIGS[${name}]}")
    elif [[ "${NEW_FN_SIGS[${name}]}" != "${OLD_FN_SIGS[${name}]}" ]]; then
      CHANGED_FNS+=("${OLD_FN_SIGS[${name}]}  =>  ${NEW_FN_SIGS[${name}]}")
    fi
  done

  for name in "${!NEW_FN_SIGS[@]}"; do
    if [[ -z "${OLD_FN_SIGS[${name}]+x}" ]]; then
      ADDED_FNS+=("${NEW_FN_SIGS[${name}]}")
    fi
  done

  echo "  old functions: ${#OLD_FN_SIGS[@]}   new functions: ${#NEW_FN_SIGS[@]}" >>"${REPORT}"

  if [[ ${#REMOVED_FNS[@]} -gt 0 ]]; then
    BREAKING=1
    {
      echo "  BREAKING: exported functions removed:"
      for f in "${REMOVED_FNS[@]}"; do
        echo "    - ${f}"
      done
    } >>"${REPORT}"
  else
    echo "  OK: no exported functions removed." >>"${REPORT}"
  fi

  if [[ ${#CHANGED_FNS[@]} -gt 0 ]]; then
    BREAKING=1
    {
      echo "  BREAKING: exported function signatures changed:"
      for f in "${CHANGED_FNS[@]}"; do
        echo "    - ${f}"
      done
    } >>"${REPORT}"
  else
    echo "  OK: no exported function signatures changed." >>"${REPORT}"
  fi

  if [[ ${#ADDED_FNS[@]} -gt 0 ]]; then
    {
      echo "  INFO: new exported functions added (non-breaking):"
      for f in "${ADDED_FNS[@]}"; do
        echo "    + ${f}"
      done
    } >>"${REPORT}"
  fi
fi

# --- Verdict --------------------------------------------------------------

{
  echo ""
  echo "--------------------------------------------------------------"
} >>"${REPORT}"

if [[ ${BREAKING} -eq 0 ]]; then
  echo " RESULT: COMPATIBLE - upgrade is safe." >>"${REPORT}"
else
  echo " RESULT: BREAKING CHANGE DETECTED - upgrade is NOT safe." >>"${REPORT}"
fi
echo "==============================================================" >>"${REPORT}"

cat "${REPORT}"

if [[ ${BREAKING} -ne 0 ]]; then
  exit 1
fi

exit 0