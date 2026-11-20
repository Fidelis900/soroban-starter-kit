import json
import os
import sys

DEFAULT_BUDGET = 65536  # 64KB uncompressed default budget for standard templates


def _resolve_entry(entry):
    """Normalize a baseline entry into (baseline_size, budget).

    Supports plain integer baselines (backward-compatible) as well as
    dict entries like {"baseline": 12345, "budget": 65536}.
    """
    if isinstance(entry, dict):
        baseline_size = entry.get('baseline')
        budget = entry.get('budget', DEFAULT_BUDGET)
        return baseline_size, budget
    # Plain integer (or numeric) baseline
    return entry, DEFAULT_BUDGET


def check_wasm_sizes(baseline_file, growth_factor):
    with open(baseline_file, 'r') as f:
        baseline_sizes = json.load(f)

    wasm_dir = 'target/wasm32-unknown-unknown/release'
    if not os.path.isdir(wasm_dir):
        print(f'ERROR: WASM output directory not found: {wasm_dir}')
        sys.exit(1)

    failed = False
    rows = []

    for wasm_file in sorted(os.listdir(wasm_dir)):
        if not wasm_file.endswith('.wasm'):
            continue

        contract_name = wasm_file.replace('.wasm', '')
        current_size = os.path.getsize(os.path.join(wasm_dir, wasm_file))

        if contract_name in baseline_sizes:
            baseline_size, budget = _resolve_entry(baseline_sizes[contract_name])
        else:
            baseline_size, budget = None, DEFAULT_BUDGET

        # Growth-factor check (existing behavior, only when a baseline exists)
        growth_ok = True
        max_size = None
        if baseline_size is not None:
            max_size = baseline_size * float(growth_factor)
            if current_size > max_size:
                growth_ok = False

        # Budget check (new strict gate)
        budget_ok = current_size <= budget

        if not growth_ok or not budget_ok:
            failed = True

        rows.append({
            'contract': contract_name,
            'baseline': baseline_size,
            'current': current_size,
            'max_growth': max_size,
            'budget': budget,
            'growth_ok': growth_ok,
            'budget_ok': budget_ok,
        })

    # Print comparison table
    header = (
        f"{'Contract':<28} {'Baseline':>10} {'Current':>10} "
        f"{'MaxGrowth':>12} {'Budget':>10} {'Status':>10}"
    )
    print(header)
    print('-' * len(header))
    for row in rows:
        baseline_str = str(row['baseline']) if row['baseline'] is not None else 'n/a'
        max_growth_str = (
            f"{row['max_growth']:.0f}" if row['max_growth'] is not None else 'n/a'
        )
        if row['growth_ok'] and row['budget_ok']:
            status = 'OK'
        elif not row['budget_ok']:
            status = 'OVER BUDGET'
        else:
            status = 'GROWTH FAIL'
        print(
            f"{row['contract']:<28} {baseline_str:>10} {row['current']:>10} "
            f"{max_growth_str:>12} {row['budget']:>10} {status:>10}"
        )

    # Detailed failure messages
    for row in rows:
        if not row['budget_ok']:
            print(
                f"ERROR: {row['contract']} size {row['current']} bytes exceeds "
                f"budget of {row['budget']} bytes"
            )
        if not row['growth_ok']:
            print(
                f"ERROR: {row['contract']} has grown beyond the allowed growth "
                f"factor (current={row['current']}, max={row['max_growth']:.0f})"
            )

    if failed:
        sys.exit(1)


if __name__ == '__main__':
    if len(sys.argv) < 3:
        print('Usage: check_wasm_sizes.py <baseline_file> <growth_factor>')
        sys.exit(2)
    check_wasm_sizes(sys.argv[1], sys.argv[2])