#!/usr/bin/env bash
# Smoke e: brace-fork as a pipe step (sprefa-4v3). One shared upstream
# (`fs(glob(**/*.rs))`) feeds N arms; results union into one pipe header
# with rows from every arm.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/copy_audit_fork.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# One pipe header, no parse errors leaked above it.
echo "$out" | grep -q '^pipe 0$' \
    || { echo "FAIL: missing pipe 0 header" >&2; exit 1; }
if echo "$out" | grep -q '^parse: '; then
    echo "FAIL: parse errors surfaced" >&2; exit 1
fi

# Multi-arm fan-out should produce more than one row total. The
# 4-arm fork over the server crate matches at least the .to_owned()
# arm (cap.byte_range case in backend.rs / sprefa-run.rs).
rows=$(echo "$out" | grep -cE '^  server@HEAD .*R=server' || true)
[ "$rows" -ge 2 ] || { echo "FAIL: expected multi-row fork output, got $rows" >&2; exit 1; }

# At least one row carrying X= (an ast capture) — proves an arm fired.
echo "$out" | grep -qE '^  server@HEAD .* X=' \
    || { echo "FAIL: no ast-arm rows with X capture" >&2; exit 1; }

echo "OK"
