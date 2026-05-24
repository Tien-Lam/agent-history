#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Run repeatable local search/index performance checks.

Usage: scripts/bench-search.sh [OPTIONS]

Options:
    --recall-only       Run only the search recall/MRR benchmark.
    --performance-only  Run only the broader performance smoke tests.
    --optimized         Run tests with the optimized bench profile for realistic timings.
    --write-report      Write docs/SEARCH_BENCH.md from the recall benchmark.
    -h, --help          Show this help.

The report file is ignored by git; it is for local comparison only.
EOF
}

RUN_RECALL=1
RUN_PERFORMANCE=1
OPTIMIZED=0
WRITE_REPORT=0

while [ $# -gt 0 ]; do
    case "$1" in
        --recall-only)
            RUN_RECALL=1
            RUN_PERFORMANCE=0
            shift
            ;;
        --performance-only)
            RUN_RECALL=0
            RUN_PERFORMANCE=1
            shift
            ;;
        --optimized)
            OPTIMIZED=1
            shift
            ;;
        --write-report)
            WRITE_REPORT=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CARGO_PROFILE_ARGS=()
if [ "$OPTIMIZED" -eq 1 ]; then
    CARGO_PROFILE_ARGS+=(--profile bench-opt)
fi

if [ "$RUN_RECALL" -eq 1 ]; then
    if [ "$WRITE_REPORT" -eq 1 ]; then
        AGHIST_BENCH_WRITE_REPORT=1 cargo test "${CARGO_PROFILE_ARGS[@]}" --test recall_bench -- --nocapture
    else
        cargo test "${CARGO_PROFILE_ARGS[@]}" --test recall_bench -- --nocapture
    fi
fi

if [ "$RUN_PERFORMANCE" -eq 1 ]; then
    cargo test "${CARGO_PROFILE_ARGS[@]}" --test performance -- --nocapture
fi
