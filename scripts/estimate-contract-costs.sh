#!/usr/bin/env bash
# estimate-contract-costs.sh
#
# Local CLI helper for Soroban contract developers. Iterates over compiled
# WASM artifacts, reports uncompressed vs compressed footprint, top code-size
# contributors (via twiggy), and estimated invocation CPU/instruction fees
# (via `soroban contract inspect`). Warns and exits non-zero when any contract
# exceeds the 64KB uncompressed budget.
#
# Usage:
#   scripts/estimate-contract-costs.sh [WASM_DIR]
#
# Default WASM_DIR: target/wasm32-unknown-unknown/release
#
# Environment:
#   SIZE_BUDGET_BYTES   Uncompressed budget in bytes (default: 65536 = 64KB)
#   TWIGGY_TOP          Number of top code-size contributors to show (default: 10)
#   SOROBAN_BIN         Path to soroban CLI binary (default: soroban)

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
WASM_DIR="${1:-target/wasm32-unknown-unknown/release}"
SIZE_BUDGET_BYTES="${SIZE_BUDGET_BYTES:-65536}"
TWIGGY_TOP="${TWIGGY_TOP:-10}"
SOROBAN_BIN="${SOROBAN_BIN:-soroban}"

# ANSI colors (disabled when not a TTY)
if [ -t 1 ]; then
  C_RESET="\033[0m"
  C_BOLD="\033[1m"
  C_RED="\033[31m"
  C_GREEN="\033[32m"
  C_YELLOW="\033[33m"
  C_CYAN="\033[36m"
  C_DIM="\033[2m"
else
  C_RESET=""
  C_BOLD=""
  C_RED=""
  C_GREEN=""
  C_YELLOW=""
  C_CYAN=""
  C_DIM=""
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log_info()  { printf "%b\n" "${C_CYAN}info:${C_RESET} $*"; }
log_warn()  { printf "%b\n" "${C_YELLOW}warn:${C_RESET} $*" >&2; }
log_error() { printf "%b\n" "${C_RED}error:${C_RESET} $*" >&2; }

# Human-readable byte formatting (e.g. 12345 -> "12.06 KB")
format_bytes() {
  local bytes="$1"
  if command -v awk >/dev/null 2>&1; then
    awk -v b="$bytes" 'BEGIN {
      if (b < 1024) { printf "%d B", b }
      else if (b < 1024*1024) { printf "%.2f KB", b/1024 }
      else { printf "%.2f MB", b/(1024*1024) }
    }'
  else
    printf "%d B" "$bytes"
  fi
}

# Return file size in bytes, portable across GNU/BSD stat.
file_size_bytes() {
  local f="$1"
  if stat -c %s "$f" >/dev/null 2>&1; then
    stat -c %s "$f"
  else
    stat -f %z "$f"
  fi
}

# Compressed size using gzip -9 (best-effort; falls back to uncompressed size).
compressed_size_bytes() {
  local f="$1"
  if command -v gzip >/dev/null 2>&1; then
    gzip -9 -c "$f" 2>/dev/null | wc -c | tr -d ' '
  else
    file_size_bytes "$f"
  fi
}

have_cmd() { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
if [ ! -d "$WASM_DIR" ]; then
  log_error "WASM directory not found: $WASM_DIR"
  log_info  "Build contracts first, e.g.: cargo build --target wasm32-unknown-unknown --release"
  exit 2
fi

# Collect WASM files (sorted for deterministic output).
mapfile -t WASM_FILES < <(find "$WASM_DIR" -maxdepth 1 -type f -name '*.wasm' | sort)

if [ "${#WASM_FILES[@]}" -eq 0 ]; then
  log_error "No .wasm files found in $WASM_DIR"
  exit 2
fi

if ! have_cmd "$SOROBAN_BIN"; then
  log_warn "'$SOROBAN_BIN' not found on PATH; skipping metadata/inspection."
fi

if ! have_cmd twiggy; then
  log_warn "'twiggy' not found on PATH; skipping code-size contributor breakdown."
  log_info  "Install with: cargo install twiggy"
fi

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
printf "\n%b\n" "${C_BOLD}=== Soroban Contract Cost Estimate ===${C_RESET}"
printf "%b\n" "${C_DIM}WASM dir:        $WASM_DIR${C_RESET}"
printf "%b\n" "${C_DIM}Size budget:     $(format_bytes "$SIZE_BUDGET_BYTES") ($SIZE_BUDGET_BYTES bytes)${C_RESET}"
printf "%b\n" "${C_DIM}Contracts found: ${#WASM_FILES[@]}${C_RESET}"

overall_status=0
declare -a OVER_BUDGET=()

for wasm in "${WASM_FILES[@]}"; do
  name="$(basename "$wasm")"
  uncompressed="$(file_size_bytes "$wasm")"
  compressed="$(compressed_size_bytes "$wasm")"

  printf "\n%b\n" "${C_BOLD}--- $name ---${C_RESET}"

  # Size summary
  printf "  %-22s %s (%s bytes)\n" "Uncompressed size:" "$(format_bytes "$uncompressed")" "$uncompressed"
  printf "  %-22s %s (%s bytes)\n" "Compressed size:"   "$(format_bytes "$compressed")"   "$compressed"

  # Budget check
  if [ "$uncompressed" -gt "$SIZE_BUDGET_BYTES" ]; then
    over=$(( uncompressed - SIZE_BUDGET_BYTES ))
    printf "  %b\n" "${C_RED}${C_BOLD}BUDGET EXCEEDED:${C_RESET}${C_RED} over by $(format_bytes "$over") ($over bytes)${C_RESET}"
    OVER_BUDGET+=("$name")
    overall_status=1
  else
    remaining=$(( SIZE_BUDGET_BYTES - uncompressed ))
    printf "  %b\n" "${C_GREEN}Within budget:${C_RESET} $(format_bytes "$remaining") headroom"
  fi

  # Metadata / inspection via soroban CLI
  if have_cmd "$SOROBAN_BIN"; then
    printf "  %b\n" "${C_CYAN}Contract inspect:${C_RESET}"
    if inspect_out="$("$SOROBAN_BIN" contract inspect --wasm "$wasm" 2>&1)"; then
      # Indent each line for readability.
      printf "%s\n" "$inspect_out" | sed 's/^/    /'
    else
      printf "    %b\n" "${C_YELLOW}(inspect failed)${C_RESET}"
      printf "%s\n" "$inspect_out" | sed 's/^/    /'
    fi
  fi

  # Top code-size contributors via twiggy
  if have_cmd twiggy; then
    printf "  %b\n" "${C_CYAN}Top $TWIGGY_TOP code-size contributors:${C_RESET}"
    if twiggy_out="$(twiggy top -n "$TWIGGY_TOP" "$wasm" 2>&1)"; then
      printf "%s\n" "$twiggy_out" | sed 's/^/    /'
    else
      printf "    %b\n" "${C_YELLOW}(twiggy failed)${C_RESET}"
      printf "%s\n" "$twiggy_out" | sed 's/^/    /'
    fi
  fi
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
printf "\n%b\n" "${C_BOLD}=== Summary ===${C_RESET}"
if [ "$overall_status" -eq 0 ]; then
  printf "%b\n" "${C_GREEN}All ${#WASM_FILES[@]} contract(s) within the $(format_bytes "$SIZE_BUDGET_BYTES") budget.${C_RESET}"
else
  printf "%b\n" "${C_RED}${#OVER_BUDGET[@]} contract(s) exceed the $(format_bytes "$SIZE_BUDGET_BYTES") budget:${C_RESET}"
  for c in "${OVER_BUDGET[@]}"; do
    printf "  - %s\n" "$c"
  done
  printf "%b\n" "${C_YELLOW}Consider reducing code size (e.g. strip symbols, enable LTO, remove unused deps).${C_RESET}"
fi
printf "\n"

exit "$overall_status"