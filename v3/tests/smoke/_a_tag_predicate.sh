#!/usr/bin/env bash
# Smoke A (sprefa-kcl): fact? predicate forms — Probe Static + JoinOp +
# all-unbound Query. Three writes seed (alpha,A1), (beta,B1), (gamma,G1).
# Then:
#   - fact?(:rel, "alpha", "A1") strict probe → HIT once.
#   - fact?(:rel, "missing")     strict probe → 0 rows (drop).
#   - fact?(:rel, "beta", ${V?}) join          → JOIN with V=B1 once.
#   - fact?(:rel, ${X?}, ${V?})  drain         → 3 rows then parks.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/fact_predicate_smoke.sprf"
out="$(timeout 3 "$bin" "$fixture" --root crates/server --rev HEAD 2>/dev/null || true)"
echo "$out"

echo "$out" | grep -q '^HIT: $' \
    || { echo "FAIL: probe HIT line missing" >&2; exit 1; }
echo "$out" | grep -qE '^pipe 4 — 0 rows$' \
    || { echo "FAIL: probe miss should yield 0 rows" >&2; exit 1; }
echo "$out" | grep -q 'V=B1' \
    || { echo "FAIL: JOIN V=B1 missing" >&2; exit 1; }
echo "$out" | grep -q 'X=alpha V=A1' \
    || { echo "FAIL: DRAIN X=alpha V=A1 missing" >&2; exit 1; }
echo "$out" | grep -q 'X=beta V=B1' \
    || { echo "FAIL: DRAIN X=beta V=B1 missing" >&2; exit 1; }
echo "$out" | grep -q 'X=gamma V=G1' \
    || { echo "FAIL: DRAIN X=gamma V=G1 missing" >&2; exit 1; }

echo "OK (fact? probe + join + drain via shared RelationStore)"
