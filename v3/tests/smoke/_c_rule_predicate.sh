#!/usr/bin/env bash
# Smoke C (sprefa-2sp): rule? predicate forms.
# Define rule(:user, name, role); body emits one row per call. Three
# explicit calls populate rule_rows[:user] with (alpha,admin), (beta,user),
# (gamma,user). Then exercise probe/join/query:
#   - user?("alpha", "admin")      strict probe  → HIT once
#   - user?("missing", "user")     strict probe  → 0 rows (drop)
#   - user?(${N?}, ${R?})          all-unbound  → query 3 known rows
#                                                  + auto-fire empty row
#   - user?("alpha", ${R?})        mixed         → JOIN R=admin once
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/rule_predicate_smoke.sprf"
out="$(timeout 3 "$bin" "$fixture" --root crates/server --rev HEAD 2>/dev/null || true)"
echo "$out"

echo "$out" | grep -q '^HIT: $' \
    || { echo "FAIL: probe HIT line missing" >&2; exit 1; }
echo "$out" | grep -qE '^pipe 5 — 0 rows$' \
    || { echo "FAIL: probe miss should yield 0 rows" >&2; exit 1; }
echo "$out" | grep -q 'N=alpha R=admin' \
    || { echo "FAIL: QUERY N=alpha R=admin missing" >&2; exit 1; }
echo "$out" | grep -q 'N=beta R=user' \
    || { echo "FAIL: QUERY N=beta R=user missing" >&2; exit 1; }
echo "$out" | grep -q 'N=gamma R=user' \
    || { echo "FAIL: QUERY N=gamma R=user missing" >&2; exit 1; }
echo "$out" | grep -qE '^pipe 7 — 1 rows$' \
    || { echo "FAIL: JOIN should yield 1 row" >&2; exit 1; }
echo "$out" | grep -q '  server@HEAD - R=admin' \
    || { echo "FAIL: JOIN R=admin projection missing" >&2; exit 1; }

echo "OK (rule? probe + join + query over rule_rows)"
