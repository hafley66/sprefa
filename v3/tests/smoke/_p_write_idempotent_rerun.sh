#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P5: idempotency on re-run, both within one process
#  and across two separate processes.
#  Will fail until aggregation + invalidation land cleanly.
# ============================================================
#
# Goal: running the same write-shaped pipeline twice produces
# the same file content as running it once. No churn, no
# duplication, no drift.
#
# Two passes:
#   (a) two pipes in one sprefa-run invocation. The second pipe
#       reads after the first writes; both arrive at the same
#       end state.
#   (b) two separate sprefa-run invocations. The second is a
#       no-op write (content unchanged) and the file is byte-
#       identical to after the first invocation.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p5_idempotent.XXXX)"
trap 'rm -rf "$work"' EXIT

cat >"$work/code.ts" <<'EOF'
import { useAlphaMutation, useBetaMutation } from './hooks';
EOF

cat >"$work/notes.md" <<'EOF'
<!-- @sprf-begin hooks -->
<!-- @sprf-end hooks -->
EOF

sprf="$work/idemp.sprf"
cat >"$sprf" <<'EOF'
fs(glob(code.ts)) > ast[ts](use${HOOK?}Mutation) > tag(:hooks, ${HOOK});
fs(glob(notes.md)) > comment("@sprf-begin hooks", ${SCOPE?}, "@sprf-end hooks") > tag(:hooks, ${HOOK?}) > render[plain](${SCOPE}, "- ${HOOK}\n") > write_cursor(${SCOPE}, :replace)
EOF

# Pass (a): single invocation should already be stable. Write
# the same pipeline twice in one .sprf and verify convergence.
sprf_double="$work/idemp_double.sprf"
cat "$sprf" >"$sprf_double"
echo ';' >>"$sprf_double"
cat "$sprf" >>"$sprf_double"

(cd "$work" && "$bin" "$sprf_double" --root .) >/dev/null
inner_a="$(awk '/@sprf-begin hooks/{f=1;next} /@sprf-end hooks/{f=0} f' "$work/notes.md")"
sha_a="$(printf '%s' "$inner_a" | shasum | awk '{print $1}')"

# Reset notes.md to clean slate for (b).
cat >"$work/notes.md" <<'EOF'
<!-- @sprf-begin hooks -->
<!-- @sprf-end hooks -->
EOF

(cd "$work" && "$bin" "$sprf" --root .) >/dev/null
inner_b1="$(awk '/@sprf-begin hooks/{f=1;next} /@sprf-end hooks/{f=0} f' "$work/notes.md")"

# Second invocation, kill server in between to be sure.
pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"
(cd "$work" && "$bin" "$sprf" --root .) >/dev/null
inner_b2="$(awk '/@sprf-begin hooks/{f=1;next} /@sprf-end hooks/{f=0} f' "$work/notes.md")"

if [ "$inner_b1" != "$inner_b2" ]; then
    echo "FAIL: cross-process re-run drifted" >&2
    diff <(printf '%s' "$inner_b1") <(printf '%s' "$inner_b2") >&2 || true
    exit 1
fi

# Sanity: pass (a) and pass (b) should also converge.
sha_b="$(printf '%s' "$inner_b1" | shasum | awk '{print $1}')"
if [ "$sha_a" != "$sha_b" ]; then
    echo "FAIL: in-process double-run output != single-run output" >&2
    exit 1
fi

echo "OK write pipeline is idempotent in-process and cross-process"
