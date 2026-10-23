#!/usr/bin/env bash
set -euo pipefail

# Multi-process-friendly repository soak test.
# Usage: CVX_SOAK_ITERATIONS=100 CVX_SOAK_JOBS=4 scripts/soak.sh
CVX_BIN=${CVX_BIN:-target/debug/cvx}
if [[ "$CVX_BIN" != /* ]]; then
    CVX_BIN="$(cd "$(dirname "$CVX_BIN")" && pwd)/$(basename "$CVX_BIN")"
fi
ITERATIONS=${CVX_SOAK_ITERATIONS:-100}
JOBS=${CVX_SOAK_JOBS:-1}
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT

export CVX_AUTHOR_NAME=${CVX_AUTHOR_NAME:-Soak Test}
export CVX_AUTHOR_EMAIL=${CVX_AUTHOR_EMAIL:-soak@example.com}
export CVX_COMMITTER_NAME=${CVX_COMMITTER_NAME:-Soak Test}
export CVX_COMMITTER_EMAIL=${CVX_COMMITTER_EMAIL:-soak@example.com}

"$CVX_BIN" init -q "$ROOT/repo"
cd "$ROOT/repo"

for index in $(seq 1 "$ITERATIONS"); do
    printf 'iteration %s\n' "$index" > "file-$((index % 16)).txt"
    "$CVX_BIN" add .
    "$CVX_BIN" commit -m "soak $index" >/dev/null
    if (( index % 25 == 0 )); then
        for job in $(seq 1 "$JOBS"); do
            "$CVX_BIN" status --porcelain >/dev/null &
        done
        wait
    fi
done

"$CVX_BIN" fsck >/dev/null
"$CVX_BIN" gc --prune >/dev/null
"$CVX_BIN" fsck >/dev/null
if command -v git >/dev/null 2>&1; then
    git --git-dir=.cvx --work-tree=. fsck --strict
fi
echo "soak ok: $ITERATIONS commits, $JOBS concurrent status readers"
