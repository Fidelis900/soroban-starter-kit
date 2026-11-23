#!/usr/bin/env bash
# bench-all.sh – run Criterion benchmarks for all benchmarked contracts and
# print a consolidated summary table.
#
# Usage:
#   ./scripts/bench-all.sh [-- <extra cargo bench args>]
#
# Closes #849
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULTS_DIR="${REPO_ROOT}/target/bench-results"
mkdir -p "$RESULTS_DIR"

# ── list of (bench-name  package) pairs ────────────────────────────────────
declare -a BENCHES=(
    "escrow_ops        contract-benchmarks"
    "token_ops         contract-benchmarks"
    "merkle_ops        contract-benchmarks"
    "swap_ops          contract-benchmarks"
    "staking_ops       contract-benchmarks"
    "vesting_ops       contract-benchmarks"
    "dao_ops           contract-benchmarks"
    "multisig_ops      contract-benchmarks"
    "marketplace_ops   contract-benchmarks"
    "auction_ops       contract-benchmarks"
    "airdrop_ops       contract-benchmarks"
    "ballot_ops        contract-benchmarks"
    "bonding_curve_ops contract-benchmarks"
    "crowdfund_ops     contract-benchmarks"
    "lottery_ops       contract-benchmarks"
    "nft_ops           contract-benchmarks"
    "oracle_ops        contract-benchmarks"
    "subscription_ops  contract-benchmarks"
    "timelock_ops      contract-benchmarks"
)

PASS=0
FAIL=0
declare -a SUMMARY_ROWS=()

# extra args forwarded after `--`
EXTRA_ARGS=("${@}")

run_bench() {
    local bench="$1"
    local pkg="$2"
    local log="${RESULTS_DIR}/${bench}.txt"

    echo "── running bench: ${bench} (${pkg}) ──"
    if cargo bench \
        --package "${pkg}" \
        --bench   "${bench}" \
        -- "${EXTRA_ARGS[@]}" 2>&1 | tee "${log}"; then
        PASS=$((PASS + 1))
        STATUS="PASS"
    else
        FAIL=$((FAIL + 1))
        STATUS="FAIL"
    fi

    # Collect criterion "time:" summary lines, e.g.:
    #   escrow::initialize   time:   [1.2345 µs 1.2500 µs 1.2678 µs]
    while IFS= read -r line; do
        SUMMARY_ROWS+=("  ${STATUS}  ${bench}  ${line}")
    done < <(grep -E "time:\s+\[" "${log}" || true)
}

echo "═══════════════════════════════════════════════════════"
echo "  Soroban Criterion Benchmark Suite"
echo "═══════════════════════════════════════════════════════"

cd "$REPO_ROOT"

for entry in "${BENCHES[@]}"; do
    bench=$(echo "$entry" | awk '{print $1}')
    pkg=$(echo "$entry"   | awk '{print $2}')
    run_bench "$bench" "$pkg"
    echo ""
done

# ── consolidated summary table ──────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════"
echo "  Benchmark Summary"
echo "═══════════════════════════════════════════════════════"
printf "  %-6s  %-16s  %s\n" "STATUS" "BENCH" "RESULT"
echo "  ──────  ────────────────  ────────────────────────────────────────"

if [ ${#SUMMARY_ROWS[@]} -eq 0 ]; then
    echo "  (no criterion output collected)"
else
    for row in "${SUMMARY_ROWS[@]}"; do
        echo "$row"
    done
fi

echo ""
echo "  Passed: ${PASS}  |  Failed: ${FAIL}"
echo "═══════════════════════════════════════════════════════"

[ "$FAIL" -eq 0 ]
