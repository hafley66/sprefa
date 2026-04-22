#!/usr/bin/env bash
# Smoke 2: exercise `rule(name){ pipe; pipe }` — brace body lowers to
# N sub-pipes under one rule name, each with its own header + rows.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/rule_smoke.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# Two headers, both tagged with the rule name.
pipe0=$(echo "$out" | grep -c '^rule sources pipe 0 — ' || true)
pipe1=$(echo "$out" | grep -c '^rule sources pipe 1 — ' || true)
[ "$pipe0" -eq 1 ] || { echo "FAIL: missing rule-pipe-0 header" >&2; exit 1; }
[ "$pipe1" -eq 1 ] || { echo "FAIL: missing rule-pipe-1 header" >&2; exit 1; }

# At least one .rs row and exactly one Cargo.toml row.
echo "$out" | grep -qE '^  server@HEAD src/.*\.rs R=server$' \
    || { echo "FAIL: no .rs rows under rule" >&2; exit 1; }
echo "$out" | grep -qE '^  server@HEAD Cargo\.toml R=server$' \
    || { echo "FAIL: no Cargo.toml row under rule" >&2; exit 1; }

echo "OK"
