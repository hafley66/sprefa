#!/usr/bin/env bash
# Smoke g (sprefa-3vs): rev(:wt) renders identically to rev(:HEAD) under
# the current DiskFileSource (which ignores rev and serves the worktree).
# This pins the "rev label is cosmetic until git2 lands" contract.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

base_fixture="crates/server/fixtures/copy_audit.sprf"
root="crates/server"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

wt_fixture="$tmpdir/copy_audit_wt.sprf"
sed 's/rev(:HEAD)/rev(:wt)/g' "$base_fixture" > "$wt_fixture"

# Run baseline (rev HEAD). The fixture text has rev(:HEAD); CLI --rev
# matches the atom so cursors flow.
head_rows="$("$bin" "$base_fixture" --root "$root" --rev HEAD --no-cache \
    | grep -cE '^\s+[a-z]+@' || true)"

wt_rows="$("$bin" "$wt_fixture" --root "$root" --rev wt --no-cache \
    | grep -cE '^\s+[a-z]+@' || true)"

echo "HEAD rows: $head_rows"
echo "wt   rows: $wt_rows"

if [ "$head_rows" -ne "$wt_rows" ]; then
    echo "FAIL: rev(:wt) row count differs from rev(:HEAD)" >&2
    exit 1
fi

if [ "$wt_rows" -lt 1 ]; then
    echo "FAIL: zero rows from rev(:wt) — fixture may not match anything" >&2
    exit 1
fi

echo "OK ($wt_rows rows match between rev(:HEAD) and rev(:wt))"
