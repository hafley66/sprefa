#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P3: end-of-pipe fs cache invalidation post-write.
#  Will fail / pass partially until narrowed per-key
#  invalidation lands (design report Phase 4) but should
#  pass with current coarse domain-drop if the boundary is
#  honored.
# ============================================================
#
# Goal: in one sprefa-run invocation, pipe A writes new bytes
# to a file. Pipe B reads the same file and prints. Pipe B
# MUST observe the post-write bytes; if it observes the cached
# pre-write bytes, the test fails.
#
# Boundary contract: end-of-pipe flush + invalidation. Pipe A
# runs to completion and flushes its writes; THEN pipe B runs
# and sees fresh bytes.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p3_invalidate.XXXX)"
trap 'rm -rf "$work"' EXIT

target="$work/before.txt"
printf 'OLD\n' >"$target"

# Two pipes. Pipe 1 replaces OLD with NEW. Pipe 2 reads the
# whole file and prints (cursor.active() == file body after
# read).
sprf="$work/invalidate.sprf"
cat >"$sprf" <<'EOF'
fs(glob(before.txt)) > re((?P<M>OLD)) > render[plain](NEW) > write_cursor(${M}, :replace);
fs(glob(before.txt)) > read > print(:body)
EOF

out="$work/run.out"
(cd "$work" && "$bin" "$sprf" --root .) >"$out" 2>&1

# Pipe 2's print should have emitted NEW, not OLD.
if ! grep -q 'body: NEW' "$out"; then
    echo "FAIL: pipe 2 did not observe post-write bytes (no 'body: NEW' line)" >&2
    echo "----- run output -----" >&2
    cat "$out" >&2
    exit 1
fi
if grep -q 'body: OLD' "$out"; then
    echo "FAIL: pipe 2 saw stale 'OLD' from cache" >&2
    cat "$out" >&2
    exit 1
fi

# Disk reflects the write.
final="$(cat "$target")"
if [ "$final" != "NEW" ]; then
    echo "FAIL: file on disk does not match expected post-write content" >&2
    echo "  got: $final" >&2
    exit 1
fi

echo "OK end-of-pipe write flushes fs cache before next pipe reads"
