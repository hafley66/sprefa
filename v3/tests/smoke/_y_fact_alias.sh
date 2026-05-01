#!/usr/bin/env bash
# Smoke Y (sprefa-4m7.7.14): alias period — tag(...) still parses and runs
# but emits diag tag/renamed-to-fact; fact(...) parses cleanly with no diag.
#
# Uses tag_mvp_smoke.sprf (legacy name) to confirm the alias period works.
# Parses fact_mvp_smoke.sprf to confirm the canonical name is clean.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

bin="./target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    cargo build -p server --bin sprefa-run >&2
fi

root="crates/server"

# --- alias form: tag(...) still produces output + emits rename diag on stderr
alias_out="$(timeout 3 "$bin" "crates/server/fixtures/tag_mvp_smoke.sprf" \
    --root "$root" 2>&1 || true)"

echo "=== alias (tag) stdout+stderr ==="
echo "$alias_out"

echo "$alias_out" | grep -q 'A=alpha' \
    || { echo "FAIL: alias form missing A=alpha" >&2; exit 1; }
echo "$alias_out" | grep -q 'A=beta' \
    || { echo "FAIL: alias form missing A=beta" >&2; exit 1; }
echo "$alias_out" | grep -qi 'tag/renamed-to-fact\|renamed.*fact\|renamed-to-fact' \
    || { echo "FAIL: tag/renamed-to-fact diag not emitted for alias form" >&2; exit 1; }

# --- canonical form: fact(...) produces same output, no rename diag on stderr
canon_stderr="$(timeout 3 "$bin" "crates/server/fixtures/fact_mvp_smoke.sprf" \
    --root "$root" 2>&1 1>/dev/null || true)"
canon_out="$(timeout 3 "$bin" "crates/server/fixtures/fact_mvp_smoke.sprf" \
    --root "$root" 2>/dev/null || true)"

echo "=== canonical (fact) stdout ==="
echo "$canon_out"
echo "=== canonical (fact) stderr ==="
echo "$canon_stderr"

echo "$canon_out" | grep -q 'A=alpha' \
    || { echo "FAIL: canonical form missing A=alpha" >&2; exit 1; }
echo "$canon_out" | grep -q 'A=beta' \
    || { echo "FAIL: canonical form missing A=beta" >&2; exit 1; }

if echo "$canon_stderr" | grep -q 'tag/renamed-to-fact'; then
    echo "FAIL: canonical fact(...) should not emit tag/renamed-to-fact diag" >&2
    exit 1
fi

echo "OK (alias period: tag→fact diag emitted; fact parses cleanly)"
