#!/usr/bin/env bash
# monitor-auction.sh — display a human-readable auction status summary
#
# Usage:
#   ./scripts/monitor-auction.sh [network] <CONTRACT_ID>
#   ./scripts/monitor-auction.sh testnet CABC...
#   ./scripts/monitor-auction.sh local CABC...
#
# Environment overrides:
#   STELLAR_RPC_URL            Override the default RPC endpoint
#   STELLAR_NETWORK_PASSPHRASE Override the default network passphrase
#
# What it shows:
#   - Seller, token, start price, min increment, reserve price, extension window
#   - Current highest bidder and bid
#   - Deadline ledger and approximate time remaining
#   - Settlement / cancelled status
#   - WARNING banners for: overdue-but-unsettled, reserve not met, cancelled

set -euo pipefail

# ── color codes ──────────────────────────────────────────────────────────────
RED='\033[0;31m'
YELLOW='\033[0;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# ── argument parsing ─────────────────────────────────────────────────────────
if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 [network] <CONTRACT_ID>" >&2
  echo "  network: testnet (default) | mainnet | local" >&2
  exit 1
fi

if [[ $# -eq 2 ]]; then
  NETWORK="$1"
  CONTRACT_ID="$2"
else
  NETWORK="testnet"
  CONTRACT_ID="$1"
fi

case "$NETWORK" in
  testnet)
    RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
    PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
    ;;
  mainnet)
    RPC_URL="${STELLAR_RPC_URL:-https://soroban.stellar.org}"
    PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
    ;;
  local)
    RPC_URL="${STELLAR_RPC_URL:-http://localhost:${LOCAL_RPC_PORT:-8000}}"
    PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
    ;;
  *)
    echo "Unknown network: $NETWORK (use testnet|mainnet|local)" >&2
    exit 1
    ;;
esac

# ── helpers ──────────────────────────────────────────────────────────────────
STELLAR_ARGS=(
  --id "$CONTRACT_ID"
  --rpc-url "$RPC_URL"
  --network-passphrase "$PASSPHRASE"
  --network "$NETWORK"
)

invoke() {
  stellar contract invoke "${STELLAR_ARGS[@]}" -- "$@" 2>/dev/null || echo "N/A"
}

# Strip surrounding quotes from stellar CLI output
strip_quotes() {
  echo "$1" | tr -d '"'
}

# Convert a raw ledger count (positive = remaining, negative = overdue) to a
# human-readable string. Assumes ~5 seconds per ledger.
ledgers_to_time() {
  local ledgers="$1"
  if [[ "$ledgers" == "N/A" ]]; then
    echo "N/A"
    return
  fi
  local abs_ledgers="${ledgers#-}"
  local total_seconds=$(( abs_ledgers * 5 ))
  local days=$(( total_seconds / 86400 ))
  local hours=$(( (total_seconds % 86400) / 3600 ))
  local minutes=$(( (total_seconds % 3600) / 60 ))

  if [[ "$ledgers" -lt 0 ]]; then
    echo "${days}d ${hours}h ${minutes}m ago (OVERDUE)"
  elif [[ "$days" -gt 0 ]]; then
    echo "${days}d ${hours}h ${minutes}m remaining"
  elif [[ "$hours" -gt 0 ]]; then
    echo "${hours}h ${minutes}m remaining"
  else
    echo "${minutes}m remaining"
  fi
}

# Fetch the current ledger sequence from the RPC endpoint.
get_current_ledger() {
  local response
  response=$(curl -sf --max-time 10 \
    -X POST "$RPC_URL" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger","params":{}}' \
    2>/dev/null || echo "")
  if [[ -n "$response" ]]; then
    echo "$response" | grep -o '"sequence":[0-9]*' | head -1 | cut -d':' -f2
  else
    echo "0"
  fi
}

# ── fetch auction data ───────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}Auction Monitor — ${CYAN}${CONTRACT_ID}${RESET}"
echo -e "Network: ${BOLD}${NETWORK}${RESET}  RPC: ${RPC_URL}"
echo "────────────────────────────────────────────────────"

# get_info returns a struct; parse the JSON-like output for each field.
INFO_RAW=$(stellar contract invoke "${STELLAR_ARGS[@]}" -- get_info 2>/dev/null || echo "N/A")

if [[ "$INFO_RAW" == "N/A" ]]; then
  echo -e "${RED}Contract not initialized or unreachable.${RESET}"
  echo "────────────────────────────────────────────────────"
  echo ""
  exit 0
fi

# Extract individual fields from the struct output.
# The Soroban CLI serialises contract return values as JSON.
parse_field() {
  local field="$1"
  local raw="$2"
  echo "$raw" | grep -o "\"${field}\":[^,}]*" | head -1 | cut -d':' -f2- | tr -d '" '
}

parse_optional_field() {
  local field="$1"
  local raw="$2"
  local val
  val=$(echo "$raw" | grep -o "\"${field}\":[^,}]*" | head -1 | cut -d':' -f2- | tr -d ' ')
  # Soroban encodes Option<T> as {"Some": value} or {"None": null}
  if echo "$val" | grep -q '"None"'; then
    echo "none"
  else
    echo "$val" | grep -o '"Some":[^}]*' | cut -d':' -f2- | tr -d '"{}' || echo "$val"
  fi
}

SELLER=$(parse_field "seller" "$INFO_RAW")
TOKEN=$(parse_field "token" "$INFO_RAW")
START_PRICE=$(parse_field "start_price" "$INFO_RAW")
MIN_INCREMENT=$(parse_field "min_increment" "$INFO_RAW")
DEADLINE=$(parse_field "deadline" "$INFO_RAW")
HIGHEST_BID=$(parse_field "highest_bid" "$INFO_RAW")
SETTLED=$(parse_field "settled" "$INFO_RAW")
RESERVE_PRICE=$(parse_optional_field "reserve_price" "$INFO_RAW")
EXTENSION_WINDOW=$(parse_field "extension_window" "$INFO_RAW")

# highest_bidder is Option<Address>
HIGHEST_BIDDER=$(parse_optional_field "highest_bidder" "$INFO_RAW")
if [[ "$HIGHEST_BIDDER" == "none" || -z "$HIGHEST_BIDDER" ]]; then
  HIGHEST_BIDDER="(no bids yet)"
fi

# Determine cancelled state via the read-only is_cancelled() query.
# This reads DataKey::Cancelled directly — no auth required, no transaction
# submitted. Calling the real end() function here (the previous approach) was
# dangerous: end() is permissionless and moves funds, so invoking it as a
# status probe would inadvertently settle any overdue auction it was run against.
CANCELLED_RAW=$(stellar contract invoke "${STELLAR_ARGS[@]}" -- is_cancelled 2>&1)
CANCELLED_EXIT=$?
if [[ $CANCELLED_EXIT -ne 0 ]] || echo "$CANCELLED_RAW" | grep -qi "error\|not found\|unknown\|invalid"; then
  IS_CANCELLED="unknown"
elif echo "$CANCELLED_RAW" | grep -qi "true"; then
  IS_CANCELLED="true"
else
  IS_CANCELLED="false"
fi

# Compute remaining ledgers
CURRENT_LEDGER=$(get_current_ledger)
if [[ -n "$DEADLINE" && "$DEADLINE" != "N/A" && "$CURRENT_LEDGER" -gt 0 ]]; then
  REMAINING=$(( DEADLINE - CURRENT_LEDGER ))
else
  REMAINING="N/A"
fi
TIME_DISPLAY=$(ledgers_to_time "$REMAINING")

# ── status label ─────────────────────────────────────────────────────────────
if [[ "$IS_CANCELLED" == "true" ]]; then
  STATUS="${RED}Cancelled${RESET}"
elif [[ "$IS_CANCELLED" == "unknown" ]]; then
  STATUS="${YELLOW}Unknown${RESET}"
elif [[ "$SETTLED" == "true" ]]; then
  STATUS="${GREEN}Settled${RESET}"
elif [[ "$REMAINING" != "N/A" && "$REMAINING" -lt 0 ]]; then
  STATUS="${YELLOW}Ended (awaiting settlement)${RESET}"
else
  STATUS="${CYAN}Active${RESET}"
fi

# ── print summary ─────────────────────────────────────────────────────────────
echo -e "Status:            ${STATUS}"
echo ""
echo -e "Seller:            ${BOLD}${SELLER:-N/A}${RESET}"
echo -e "Token:             ${BOLD}${TOKEN:-N/A}${RESET}"
echo ""
echo -e "Start price:       ${BOLD}${START_PRICE:-N/A}${RESET} (base units)"
echo -e "Min increment:     ${BOLD}${MIN_INCREMENT:-N/A}${RESET} (base units)"

if [[ "$RESERVE_PRICE" == "none" || -z "$RESERVE_PRICE" ]]; then
  echo -e "Reserve price:     ${BOLD}none${RESET}"
else
  echo -e "Reserve price:     ${BOLD}${RESERVE_PRICE}${RESET} (base units)"
fi

if [[ "$EXTENSION_WINDOW" == "0" || -z "$EXTENSION_WINDOW" ]]; then
  echo -e "Anti-snipe window: ${BOLD}disabled${RESET}"
else
  echo -e "Anti-snipe window: ${BOLD}${EXTENSION_WINDOW}${RESET} ledgers"
fi

echo ""
echo -e "Highest bid:       ${BOLD}${HIGHEST_BID:-N/A}${RESET} (base units)"
echo -e "Highest bidder:    ${BOLD}${HIGHEST_BIDDER}${RESET}"
echo ""
echo -e "Deadline:          ledger ${BOLD}${DEADLINE:-N/A}${RESET}"
echo -e "Current ledger:    ${BOLD}${CURRENT_LEDGER}${RESET}"
echo -e "Time remaining:    ${BOLD}${TIME_DISPLAY}${RESET}"

# ── warning banners ───────────────────────────────────────────────────────────
if [[ "$IS_CANCELLED" == "unknown" ]]; then
  echo ""
  echo -e "${YELLOW}${BOLD}WARNING: Could not determine cancelled state (is_cancelled query failed). Contract may be an older deployment.${RESET}"
elif [[ "$IS_CANCELLED" == "true" ]]; then
  echo ""
  echo -e "${RED}${BOLD}INFO: Auction was cancelled by the seller (no bids had been placed).${RESET}"
elif [[ "$SETTLED" == "true" ]]; then
  echo ""
  echo -e "${GREEN}${BOLD}Auction has been settled. Winner: ${HIGHEST_BIDDER}${RESET}"
  # Warn if reserve was not met (highest_bid < reserve_price at settlement)
  if [[ "$RESERVE_PRICE" != "none" && -n "$RESERVE_PRICE" && "$HIGHEST_BID" != "N/A" ]]; then
    if (( HIGHEST_BID < RESERVE_PRICE )); then
      echo -e "${YELLOW}${BOLD}NOTE: Reserve price was not met. Highest bidder's funds were returned.${RESET}"
    fi
  fi
elif [[ "$REMAINING" != "N/A" && "$REMAINING" -lt 0 ]]; then
  echo ""
  echo -e "${YELLOW}${BOLD}WARNING: Deadline has passed. Anyone may call \`end\` to settle the auction.${RESET}"
  if [[ "$RESERVE_PRICE" != "none" && -n "$RESERVE_PRICE" && "$HIGHEST_BID" != "N/A" ]]; then
    if (( HIGHEST_BID < RESERVE_PRICE )); then
      echo -e "${RED}${BOLD}WARNING: Highest bid (${HIGHEST_BID}) is below reserve price (${RESERVE_PRICE}). Bidder will be refunded.${RESET}"
    fi
  fi
fi

echo "────────────────────────────────────────────────────"
echo ""
