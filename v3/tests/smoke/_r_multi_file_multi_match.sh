#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P7: M matches across F files → F merged edits, no
#  cross-file contamination.
#  Will fail until aggregation lands per (repo, rev, fs).
# ============================================================
#
# Setup: 3 source files, each with 2 distinct hooks. 3 sibling
# md files, each with one anchored region. Pipeline must:
#   - extract hooks from each source file
#   - bind each hook to its source-file identity
#   - render-aggregate per md file (matched by basename or
#     companion convention)
#   - write the per-file aggregated blob into that md's SCOPE
#
# Cross-file invariant: file_A's hooks must not appear in
# file_B's notes, and vice versa.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p7_multifile.XXXX)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/src" "$work/notes"

cat >"$work/src/alpha.ts" <<'EOF'
import { useA1Mutation, useA2Mutation } from './hooks';
EOF
cat >"$work/src/bravo.ts" <<'EOF'
import { useB1Mutation, useB2Mutation } from './hooks';
EOF
cat >"$work/src/charlie.ts" <<'EOF'
import { useC1Mutation, useC2Mutation } from './hooks';
EOF

for name in alpha bravo charlie; do
    cat >"$work/notes/${name}.md" <<EOF
<!-- @sprf-begin hooks -->
<!-- @sprf-end hooks -->
EOF
done

# Pair source -> notes by basename. Tag carries (BASE, HOOK)
# so the join in pipe 2 is bound on BASE and projecting HOOK
# — semi-join subscribe (mixed = select + where, post-cleanup
# semantics; today's grammar errors on raw mixed without `?`).
sprf="$work/multi.sprf"
cat >"$sprf" <<'EOF'
fs(glob(src/${BASE?}.ts)) > ast[ts](use${HOOK?}Mutation) > tag(:hooks, ${BASE}, ${HOOK});
fs(glob(notes/${BASE?}.md)) > comment("@sprf-begin hooks", ${SCOPE?}, "@sprf-end hooks") > tag(:hooks, ${BASE}, ${HOOK?}) > render[plain](${SCOPE}, "- ${HOOK}\n") > write_cursor(${SCOPE}, :replace)
EOF

(cd "$work" && "$bin" "$sprf" --root .) >/dev/null

assert_only() {
    local md="$1"
    shift
    local want=("$@")
    local inner
    inner="$(awk '/@sprf-begin hooks/{f=1;next} /@sprf-end hooks/{f=0} f' "$md")"
    local count
    count="$(printf '%s' "$inner" | grep -c '^- ' || true)"
    if [ "$count" -ne 2 ]; then
        echo "FAIL: $md should have 2 hook lines, got $count" >&2
        echo "$inner" >&2
        exit 1
    fi
    for h in "${want[@]}"; do
        grep -q "^- $h$" <<<"$inner" || { echo "FAIL: $md missing $h" >&2; exit 1; }
    done
    # Negative: no hooks from siblings.
    for h in A1 A2 B1 B2 C1 C2; do
        if [[ ! " ${want[*]} " =~ " $h " ]]; then
            grep -q "^- $h$" <<<"$inner" && {
                echo "FAIL: $md leaked sibling hook $h" >&2
                exit 1
            }
        fi
    done
}

assert_only "$work/notes/alpha.md"   A1 A2
assert_only "$work/notes/bravo.md"   B1 B2
assert_only "$work/notes/charlie.md" C1 C2

echo "OK 6 cursors → 3 merged edits, one per file, no leakage"
