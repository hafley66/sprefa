#!/usr/bin/env bash
# Smoke 9 (sprefa-944): fact(:name, ...) MVP — write 2 rows, read drains
# both. Validates: positional all-string columns, atom name lowering,
# Term::Read vs Term::Unbound classification (write vs read), RelationStore
# shared across pipes in one seed RtCtx, snapshot batch emits per row,
# and SubjectRegistry::subscribe atomicity (no lost wakes between pipes).
#
# The read pipe is perpetual — `timeout` bounds the run; we assert that
# both row values surface in stdout before timeout fires.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/fact_mvp_smoke.sprf"
root="crates/server"
rev="HEAD"

# Bound the run; fact-read parks on subscription after snapshot.
out="$(timeout 3 "$bin" "$fixture" --root "$root" --rev "$rev" 2>/dev/null || true)"
echo "$out"

# Both writers' rows must appear, bound under capture A by the reader.
echo "$out" | grep -q 'A=alpha' \
    || { echo "FAIL: missing A=alpha in read output" >&2; exit 1; }
echo "$out" | grep -q 'A=beta' \
    || { echo "FAIL: missing A=beta in read output" >&2; exit 1; }

# Pipe 0 + 1 are writes; their trailing row-count line should print
# (each write pipe is finite — passes one cursor through).
echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: pipe 0 (write) did not finalize" >&2; exit 1; }
echo "$out" | grep -qE '^pipe 1 — [0-9]+ rows$' \
    || { echo "FAIL: pipe 1 (write) did not finalize" >&2; exit 1; }

echo "OK (fact write+read roundtrip via shared RelationStore)"
