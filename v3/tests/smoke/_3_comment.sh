#!/usr/bin/env bash
# Smoke 3: fs > read > comment > print end-to-end. Validates the content
# contract across four ops: fs enumerates, read loads bytes, comment
# narrows byte_range to marker-bounded regions, print emits one line per
# cursor via PrintEffect.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/comment_smoke.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# At least one match line prefixed with "match: " from the print op.
matches="$(echo "$out" | grep -c '^match: ' || true)"
if [ "$matches" -lt 1 ]; then
    echo "FAIL: zero match lines" >&2
    exit 1
fi

# The sprefa-run module-doc line is the obvious positive.
echo "$out" | grep -qE '^match: //! sprefa-run' \
    || { echo "FAIL: expected match inside sprefa-run doc comment" >&2; exit 1; }

echo "OK ($matches matches)"
