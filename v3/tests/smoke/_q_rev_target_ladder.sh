#!/usr/bin/env bash
# ============================================================
#  DRAFT — DOES NOT PASS YET
#  Tests P6: rev-target ladder for writes.
#  Sub-tests:
#    6a rev(:wt)    — write WT directly
#    6b rev(:HEAD)  — write WT, emit write/head-warning diag
#    6c rev(branch) — auto-worktree, emit write/orphan-worktree
#    6e rev(<sha>)  — auto-worktree, emit write/orphan-worktree
#  (6d immutable-tag dropped — sprefa treats branches/tags
#   uniformly; mutation always lands at a sha.)
# ============================================================

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
bin="$v3_dir/target/debug/sprefa-run"

pkill -f sprefa-server >/dev/null 2>&1 || true
rm -f "$HOME/.cache/sprefa/server.json"

if [ ! -x "$bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

work="$(mktemp -d -t sprefa_p6_ladder.XXXX)"
trap 'rm -rf "$work"' EXIT

repo="$work/repo"
mkdir -p "$repo"
(
    cd "$repo"
    git init -q -b main
    git config user.email t@t
    git config user.name t
    printf 'OLDFOO\n' >target.txt
    git add target.txt
    git commit -qm v1
    git checkout -q -b feature
    printf 'OLDFOO\n' >target.txt
    git add target.txt
    git commit -qm v2
    git checkout -q main
)
sha_main="$(git -C "$repo" rev-parse main)"

run_pipe() {
    local rev_label="$1"
    local out="$2"
    local sprf="$work/$3.sprf"
    cat >"$sprf" <<EOF
rev(${rev_label}) > fs(glob(target.txt)) > re((?P<MATCH>OLDFOO)) > render[plain](NEWFOO) > write_cursor(\${MATCH}, :replace)
EOF
    (cd "$repo" && "$bin" "$sprf" --root .) >"$out" 2>&1 || true
}

# ---- 6a rev(:wt) ----
out_a="$work/a.log"
run_pipe ':wt' "$out_a" 'a'
if ! grep -q 'NEWFOO' "$repo/target.txt"; then
    echo "FAIL 6a: rev(:wt) did not write WT" >&2
    cat "$out_a" >&2
    exit 1
fi
# Reset
git -C "$repo" checkout -q -- target.txt

# ---- 6b rev(:HEAD) ----
out_b="$work/b.log"
run_pipe ':HEAD' "$out_b" 'b'
if ! grep -q 'NEWFOO' "$repo/target.txt"; then
    echo "FAIL 6b: rev(:HEAD) did not write WT" >&2
    cat "$out_b" >&2
    exit 1
fi
if ! grep -q 'write/head-warning' "$out_b"; then
    echo "FAIL 6b: missing write/head-warning diag" >&2
    cat "$out_b" >&2
    exit 1
fi
git -C "$repo" checkout -q -- target.txt

# ---- 6c rev(feature) (branch != HEAD) ----
out_c="$work/c.log"
run_pipe 'feature' "$out_c" 'c'
if grep -q 'NEWFOO' "$repo/target.txt"; then
    echo "FAIL 6c: source WT was mutated when targeting branch 'feature'" >&2
    exit 1
fi
if ! grep -q 'write/orphan-worktree' "$out_c"; then
    echo "FAIL 6c: missing write/orphan-worktree diag" >&2
    cat "$out_c" >&2
    exit 1
fi
cache_root="$HOME/.cache/sprefa/wt"
if ! grep -rl 'NEWFOO' "$cache_root" 2>/dev/null | grep -q .; then
    echo "FAIL 6c: cross-branch write did not land in any auto-worktree" >&2
    exit 1
fi

# ---- 6e rev(<sha>) ----
out_e="$work/e.log"
run_pipe "$sha_main" "$out_e" 'e'
if grep -q 'NEWFOO' "$repo/target.txt"; then
    echo "FAIL 6e: source WT was mutated when targeting sha" >&2
    exit 1
fi
if ! grep -q 'write/orphan-worktree' "$out_e"; then
    echo "FAIL 6e: missing write/orphan-worktree diag for sha" >&2
    cat "$out_e" >&2
    exit 1
fi

echo "OK rev-target ladder: :wt, :HEAD, branch, sha"
