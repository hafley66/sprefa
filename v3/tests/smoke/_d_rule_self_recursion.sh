#!/usr/bin/env bash
# Smoke D (sprefa-1sg): rule self-recursion is forbidden.
# Expected: sprefa-run exits non-zero with a `RecursionForbidden` parse
# diagnostic that points at the offending invocation byte range.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    cargo build -p server --bin sprefa-run >&2
fi

fixture="crates/server/fixtures/rule_self_recursion_smoke.sprf"
set +e
err="$(timeout 3 "$bin" "$fixture" --root crates/server --rev HEAD 2>&1 1>/dev/null)"
rc=$?
set -e

if [ "$rc" -eq 0 ]; then
    echo "FAIL: expected non-zero exit; got 0" >&2
    echo "$err" >&2
    exit 1
fi

echo "$err" | grep -q 'RecursionForbidden' \
    || { echo "FAIL: missing RecursionForbidden diagnostic" >&2; echo "$err" >&2; exit 1; }
echo "$err" | grep -q '"foo"' \
    || { echo "FAIL: diag should name the offending rule" >&2; echo "$err" >&2; exit 1; }

echo "OK (rule self-recursion outlawed at parse/lower time)"
