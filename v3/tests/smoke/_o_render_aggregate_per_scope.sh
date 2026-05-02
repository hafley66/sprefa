#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P4: render aggregation by (fs, SCOPE) — one rendered
#  blob per group, not N identical writes.
#  Will fail until render gains an aggregation/reducer mode
#  (design Phase 1.2: render-as-reducer).
# ============================================================
#
# OPEN INVARIANT (per author): when N HOOK cursors meet 1 SCOPE
# cursor (or vice versa), is the join cartesian, semi-joined by
# (fs), or cut/reduced? This test assumes per-(fs, SCOPE)
# aggregation with HOOK cursors collapsed via render reducer.
# Adjust expected behavior when invariant is nailed down.
#
# Setup: code.ts has 3 distinct RTKQ Mutation hook calls.
# notes.md has an empty anchored region <!-- @sprf-begin hooks -->
# ... <!-- @sprf-end hooks -->. Pipeline matches each hook,
# joins to the SCOPE cursor of notes.md, render-aggregates
# with template "- ${HOOK}\n", writes the joined blob into
# the SCOPE byte_range.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p4_aggregate.XXXX)"
trap 'rm -rf "$work"' EXIT

cat >"$work/code.ts" <<'EOF'
import { useFooMutation, useBarMutation, useBazMutation } from './hooks';
const A = useFooMutation();
const B = useBarMutation();
const C = useBazMutation();
EOF

cat >"$work/notes.md" <<'EOF'
# Notes

<!-- @sprf-begin hooks -->
<!-- @sprf-end hooks -->

end
EOF

# Cross-pipe join: pipe 1 writes hooks via tag(:hooks, ${HOOK}).
# pipe 2 reads notes.md, narrows to SCOPE, queries tag(:hooks, ${HOOK?}),
# render-aggregates per SCOPE, writes once.
#
# Tentative syntax for aggregation: render[plain](${SCOPE}, "...")
# — first arg is the aggregation key (Term in Read mode), second
# arg is the per-cursor template. Adjust when the design lands.
sprf="$work/aggregate.sprf"
cat >"$sprf" <<'EOF'
fs(glob(code.ts)) > ast[ts](use${HOOK?}Mutation) > tag(:hooks, ${HOOK});
fs(glob(notes.md)) > comment("@sprf-begin hooks", ${SCOPE?}, "@sprf-end hooks") > tag(:hooks, ${HOOK?}) > render[plain](${SCOPE}, "- ${HOOK}\n") > write_cursor(${SCOPE}, :replace)
EOF

(cd "$work" && "$bin" "$sprf" --root .) >/dev/null

after="$(cat "$work/notes.md")"
echo "----- notes.md after drain -----"
echo "$after"
echo "--------------------------------"

# Inner content between the markers should be exactly 3 lines.
inner="$(awk '/@sprf-begin hooks/{f=1;next} /@sprf-end hooks/{f=0} f' <<<"$after")"
line_count=$(printf '%s' "$inner" | grep -c '^- ' || true)

if [ "$line_count" -ne 3 ]; then
    echo "FAIL: expected 3 hook lines, got $line_count" >&2
    echo "$inner" >&2
    exit 1
fi

for hook in Foo Bar Baz; do
    if ! grep -q "^- $hook$" <<<"$inner"; then
        echo "FAIL: missing hook '$hook' in rendered region" >&2
        exit 1
    fi
done

# Markers preserved.
grep -qF '<!-- @sprf-begin hooks -->' <<<"$after" || { echo "FAIL: open marker lost" >&2; exit 1; }
grep -qF '<!-- @sprf-end hooks -->'   <<<"$after" || { echo "FAIL: close marker lost" >&2; exit 1; }

echo "OK render aggregation produced 3 distinct hook lines in one SCOPE"
