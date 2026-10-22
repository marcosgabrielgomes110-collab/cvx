#!/usr/bin/env bash
set -euo pipefail

# Opt-in large-repository benchmark with a memory ceiling.
# Example: CVX_BENCH_LARGE=1 CVX_BENCH_MEMORY_KB=4194304 scripts/bench-large.sh
if [[ "${CVX_BENCH_LARGE:-0}" != "1" ]]; then
    echo "set CVX_BENCH_LARGE=1 to run the 100k-file benchmark"
    exit 0
fi

if [[ -n "${CVX_BENCH_MEMORY_KB:-}" ]]; then
    ulimit -v "${CVX_BENCH_MEMORY_KB}" 2>/dev/null || \
        echo "warning: this platform does not support ulimit -v" >&2
fi

cargo bench -p cvx-cli --bench repository -- --quick
