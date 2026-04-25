#!/usr/bin/env bash
# Smoke 7 (sprefa-4m7.2.11): ast[lang](pattern) — bracket-arg + ast-grep-core.
# Exercises:
#   fs(glob(**/*.rs)) ↦ rust file cursor
#   ast[rust](fn $N($$$ARGS) { $$$BODY }) ↦ one cursor per fn
# Validates: bracket lowering, parse_lang dispatch, ast-grep find_all,
# native metavar capture binding, AstGrepOp::from_paren_node route.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/ast_grep_smoke.sprf"
root="crates/effect_runtime"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# Pipe header present.
echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: missing pipe header" >&2; exit 1; }

# At least one row should bind $N to a function name.
rows="$(echo "$out" | grep -cE 'N=[A-Za-z_][A-Za-z0-9_]*' || true)"
if [ "$rows" -lt 1 ]; then
    echo "FAIL: zero rows with N= function name binding" >&2
    exit 1
fi

echo "OK ($rows ast[rust] fn rows)"
