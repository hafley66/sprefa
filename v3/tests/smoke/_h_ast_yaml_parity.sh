#!/usr/bin/env bash
# Smoke H (sprefa-5qb): ast vs ast_yaml parity. Same Rust pattern (`fn $N($$$ARGS) { $$$BODY }`)
# expressed two ways:
#   pipe 0: ast[rs](fn ${N?}($$${ARGS?}) { $$${BODY?} })
#   pipe 1: ast_yaml[rs](pattern: "fn ${N?}($$${ARGS?}) { $$${BODY?} }")
# Asserts row counts agree and N captures match across pipes. If this drifts,
# ast_yaml lowering has stopped matching the same surface as the ast op.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/ast_yaml_parity.sprf"
root="crates/effect_runtime"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

ast_count="$(echo "$out" | awk '/^pipe 0 — / {print $4}')"
yaml_count="$(echo "$out" | awk '/^pipe 1 — / {print $4}')"

if [ -z "$ast_count" ] || [ -z "$yaml_count" ]; then
    echo "FAIL: missing pipe row counts (ast=$ast_count yaml=$yaml_count)" >&2
    exit 1
fi
if [ "$ast_count" -lt 1 ]; then
    echo "FAIL: ast pipe matched zero rows" >&2
    exit 1
fi
if [ "$ast_count" != "$yaml_count" ]; then
    echo "FAIL: row count mismatch (ast=$ast_count vs ast_yaml=$yaml_count)" >&2
    exit 1
fi

# Capture set parity: collect every N=... binding from each pipe section.
ast_ns="$(echo "$out" | awk '/^pipe 0$/,/^pipe 0 — /' | grep -oE 'N=[A-Za-z_][A-Za-z0-9_]*' | sort -u)"
yaml_ns="$(echo "$out" | awk '/^pipe 1$/,/^pipe 1 — /' | grep -oE 'N=[A-Za-z_][A-Za-z0-9_]*' | sort -u)"

if [ "$ast_ns" != "$yaml_ns" ]; then
    echo "FAIL: N capture set differs across ast vs ast_yaml" >&2
    diff <(echo "$ast_ns") <(echo "$yaml_ns") >&2 || true
    exit 1
fi

echo "OK (ast=$ast_count rows, ast_yaml=$yaml_count rows, N captures parity)"
