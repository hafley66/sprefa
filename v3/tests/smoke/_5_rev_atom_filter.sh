#!/usr/bin/env bash
# Smoke 5 (sprefa-4m7.14): rev(:HEAD) — atom literal in arg position.
# Validates the lowering path: host parse → atom_literal node → lowerer
# emits Value::Atom("HEAD") → RevOp::from_values → filter mode → cursors
# whose rev field equals "HEAD" pass through.
#
# The fixture uses rev(:HEAD) rather than rev(:main) so the test is
# self-standing against any branch: the --rev HEAD flag matches the atom.
#
# PRE-CONDITION: requires the Rust sonnet's 14-landing (RevOp::from_values).
# Until that merges, the binary will emit a diagnostic on `:HEAD` and the
# row-count check below will fail.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/rev_atom_filter_smoke.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# At least one .rs row should appear (server crate has Rust sources and
# rev(:HEAD) should pass cursors with rev=HEAD through).
rows="$(echo "$out" | grep -cE '\.rs' || true)"
if [ "$rows" -lt 1 ]; then
    echo "FAIL: zero rows; rev(:HEAD) atom filter may have dropped all cursors" >&2
    exit 1
fi

# Sanity: pipe header present.
echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: missing pipe header" >&2; exit 1; }

echo "OK ($rows rows through rev(:HEAD) filter)"
