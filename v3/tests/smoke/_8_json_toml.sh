#!/usr/bin/env bash
# Smoke 8 (sprefa-1vi): json() dispatches on file extension.
# Exercises:
#   fs(glob(**/Cargo.toml)) ↦ TOML file cursor
#   json({ package: { name: ${N?}, version: ${V?} } }) ↦ TomlNode walk
# Validates: parse_by_ext routes .toml through TomlNode; brace walker
# stays format-agnostic.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/json_yaml_toml_smoke.sprf"
root="."
rev="HEAD"

out="$("$bin" "$fixture" --root "$root" --rev "$rev")"
echo "$out"

echo "$out" | grep -qE '^pipe 0 — [0-9]+ rows$' \
    || { echo "FAIL: missing pipe header" >&2; exit 1; }

rows="$(echo "$out" | grep -cE 'N=.+ V=' || true)"
if [ "$rows" -lt 1 ]; then
    echo "FAIL: zero rows with both N= and V= captures bound" >&2
    exit 1
fi

echo "$out" | grep -qE 'N="tree-sitter-sprefa"' \
    || { echo "FAIL: expected N=\"tree-sitter-sprefa\" capture from Cargo.toml" >&2; exit 1; }

echo "OK ($rows toml rows)"
