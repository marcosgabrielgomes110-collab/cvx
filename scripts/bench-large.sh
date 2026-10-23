#!/usr/bin/env bash
set -euo pipefail

# Opt-in large-repository benchmark with a memory ceiling.
# Example: CVX_BENCH_LARGE=1 CVX_BENCH_MEMORY_KB=4194304 scripts/bench-large.sh
if [[ "${CVX_BENCH_LARGE:-0}" != "1" && "${CVX_BENCH_1M:-0}" != "1" && -z "${CVX_BENCH_FILES:-}" ]]; then
    echo "set CVX_BENCH_LARGE=1 for 100k files, CVX_BENCH_1M=1 for 1M, or CVX_BENCH_FILES=<n>"
    exit 0
fi

if [[ -n "${CVX_BENCH_MEMORY_KB:-}" ]]; then
    ulimit -v "${CVX_BENCH_MEMORY_KB}" 2>/dev/null || \
        echo "warning: this platform does not support ulimit -v" >&2
fi

cargo bench -p cvx-cli --bench repository -- --quick
