#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P2: cross-rev write (writing to a non-current rev).
#  Will fail until WriteTarget {repo, rev, fs} + auto-worktree
#  resolver land (write_cursor design report, Phases 1+3).
# ============================================================
#
# Goal: writing under `rev(v1.0)` materializes a worktree at
#   ~/.cache/sprefa/wt/<repo>/v1.0/
# and splices into the file there, leaving the source repo's
# working tree untouched. A `write/orphan-worktree` diag is
# emitted naming the dir.
#
# Setup: a temp git repo with a v1.0 tag at HEAD~1 and a
# different `target.txt` at HEAD vs HEAD~1. Pipe scopes to
# rev(v1.0), reads target.txt, replaces a regex match.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p2_crossrev.XXXX)"
trap 'rm -rf "$work"' EXIT

repo="$work/repo"
mkdir -p "$repo"
(
    cd "$repo"
    git init -q
    git config user.email t@t
    git config user.name t
    printf 'OLD foo OLD\n' >target.txt
    git add target.txt
    git commit -qm v1
    git tag v1.0
    printf 'NEW foo NEW\n' >target.txt
    git add target.txt
    git commit -qm v2
)

# Repo slug seeds. We want the runner to recognize this repo
# under some slug. Today's conventions: --root pegs to repo
# root and slug derives from path. Adjust if needed.
sprf="$work/crossrev.sprf"
cat >"$sprf" <<'EOF'
rev(v1.0) > fs(glob(target.txt)) > re((?P<MATCH>foo)) > render[plain](BAR) > write_cursor(${MATCH}, :replace)
EOF

run_log="$work/run.log"
(cd "$repo" && "$bin" "$sprf" --root .) >"$run_log" 2>&1 || true

# The source repo's working tree must be untouched.
wt_after="$(cat "$repo/target.txt")"
if [ "$wt_after" != "NEW foo NEW" ]; then
    echo "FAIL: source working tree was mutated; should have been untouched" >&2
    echo "  got: $wt_after" >&2
    exit 1
fi

# A worktree directory must exist under the cache location and
# contain the mutated bytes.
cache_root="$HOME/.cache/sprefa/wt"
if [ ! -d "$cache_root" ]; then
    echo "FAIL: no auto-worktree cache dir created at $cache_root" >&2
    exit 1
fi

found="$(grep -rl 'BAR' "$cache_root" 2>/dev/null | head -1 || true)"
if [ -z "$found" ]; then
    echo "FAIL: cross-rev write did not land in any auto-worktree" >&2
    exit 1
fi

# Diag asserted via run log.
if ! grep -q 'write/orphan-worktree' "$run_log"; then
    echo "FAIL: expected write/orphan-worktree diag not emitted" >&2
    cat "$run_log" >&2
    exit 1
fi

echo "OK cross-rev write landed in auto-worktree, source WT clean"
