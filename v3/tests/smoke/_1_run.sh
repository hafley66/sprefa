#!/usr/bin/env bash
# Smoke 1: run the sprefa-run CLI against fixtures/smoke.sprf, assert
# per-row shape. Same spirit as v2 tests/smoke/_1_run.sh but without
# the HTTP server — sprefa-run is a one-shot CLI.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/smoke.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# Header line present.
echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: missing pipe header" >&2; exit 1; }

# At least one row with server@HEAD and R=server.
echo "$out" | grep -qE '^\s+server@HEAD .* R=server' \
    || { echo "FAIL: missing server@HEAD row with R=server" >&2; exit 1; }

# Count rows; should be > 0.
rows="$(echo "$out" | grep -c '^  server@HEAD' || true)"
if [ "$rows" -lt 1 ]; then
    echo "FAIL: zero rows emitted" >&2
    exit 1
fi

echo "OK ($rows rows)"
