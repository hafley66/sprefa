#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P1: multi-cursor → same-file stale-write fan-out.
#  Will fail until the WriteTarget aggregation layer lands
#  (see write_cursor design report, Phase 2).
# ============================================================
#
# Goal: prove that N cursors writing distinct ranges into the
# same file produce a coherent merged result, not a clobbered
# one. Today, splices apply sequentially under a per-sink mutex
# and each splice re-reads the file fresh, so subsequent ranges
# index into already-mutated bytes and corrupt the output.
#
# Setup: a single line file with 4 lowercase triples separated
# by dashes. Pipe matches each triple via re(${MATCH?}) and
# replaces it with `${MATCH}-NEW`.
#
# Expected post-aggregation: exactly four matches replaced, in
# order, with no offset corruption.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p1_aggregate.XXXX)"
trap 'rm -rf "$work"' EXIT

target="$work/multi.txt"
printf 'aaa-bbb-ccc-ddd\n' >"$target"

sprf="$work/aggregate.sprf"
cat >"$sprf" <<'EOF'
fs(glob(multi.txt)) > re((?P<MATCH>[a-z]{3})) > render[plain](${MATCH}-NEW) > write_cursor(${MATCH}, :replace)
EOF

(cd "$work" && "$bin" "$sprf" --root .) >/dev/null

after="$(cat "$target")"
echo "----- multi.txt after drain -----"
echo "$after"
echo "---------------------------------"

expected='aaa-NEW-bbb-NEW-ccc-NEW-ddd-NEW'
if [ "$after" != "$expected" ]; then
    echo "FAIL: aggregated splice did not produce expected output" >&2
    echo "  got:      $after" >&2
    echo "  expected: $expected" >&2
    exit 1
fi

echo "OK multi-cursor write aggregates into one merged splice"
