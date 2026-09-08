#!/usr/bin/env bash
set -euo pipefail
exec target/release/examples/perf_report --shared "$SQLITE_CASCADE" "$1" "$2" "${BENCH_BACK_STRIDE:-0}"
