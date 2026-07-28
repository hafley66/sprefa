#!/usr/bin/env bash
# Runs the marker -> trace -> diag demo one-shot against a scratch db.
# NEVER starts the daemon (--no-daemon), per the self-diagnosis standing law:
# `dl daemon why` must be able to explain daemon activity from the on-disk
# trail before anything starts the daemon casually, so ad hoc demos stay
# one-shot forever, not just today.
#
# Usage: examples/trace-diag-demo/run-demo.sh [--check]
#   (no args)  runs the program and prints every queried rel, including the
#              info-severity `diag` and multi-line `hover_note` rows.
#   --check    runs the same program under --check (CLI-rendered diagnostics,
#              proves an info-severity row exits 0 — never fails CI).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

dl_bin="${DL_BIN:-$HOME/.cargo/bin/dl}"
scratch_db="$(mktemp -t trace-diag-demo.XXXXXX).sqlite"
rm -f "$scratch_db"

if [[ "${1:-}" == "--check" ]]; then
  "$dl_bin" examples/trace-diag-demo/trace-diag.dl --no-daemon --db "$scratch_db" --check
  echo "exit=$?"
else
  "$dl_bin" examples/trace-diag-demo/trace-diag.dl --no-daemon --db "$scratch_db"
fi

rm -f "$scratch_db" "$scratch_db-wal" "$scratch_db-shm"
