#!/usr/bin/env bash
# Smoke 4 (sprefa-4m7.14): fs(glob(**/*.rs)) — nested op_invocation in
# arg position. Validates the full lowering path: host parse → nested
# op_invocation node → lowerer emits Value::Pattern(Pattern::Glob(_)) →
# FsOp::from_values → filter mode → cursor fan-out over .rs files.
#
# PRE-CONDITION: requires the Rust sonnet's 14-landing (FsOp::from_values).
# Until that merges, the binary will error on the fixture source with a
# "unknown arg shape" or "parse error" message. The test will FAIL at the
# row-count check below, not silently pass.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/fs_glob_nested_smoke.sprf"
root="crates/server"
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

# At least one .rs row should be emitted (the server crate has Rust sources).
rows="$(echo "$out" | grep -cE '\.rs' || true)"
if [ "$rows" -lt 1 ]; then
    echo "FAIL: zero .rs rows emitted; fs(glob(**/*.rs)) lowering likely broken" >&2
    exit 1
fi

# The pipe header must be present (basic smoke sanity).
echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: missing pipe header" >&2; exit 1; }

echo "OK ($rows .rs rows)"
