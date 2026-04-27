#!/usr/bin/env bash
# Smoke E: drive sprefa-lsp via stdio and request hover on an
# `ast[rs](pattern)` op in copy_audit.sprf. Asserts the server returns
# a non-null hover within a deadline. Catches:
#   - missing effect-kind registrations in backend.rs (panic on hover)
#   - regressions where the per-seed pipe drain stalls
#   - hover budget blowup vs prior runs
#
# Server is killed on any exit path (success, failure, ^C, timeout).
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

# Pick the binary profile via env. SPREFA_LSP_PROFILE=release picks the
# release-mode binary (ast-grep/tree-sitter are 75-450x faster in release;
# the smoke is meaningless on debug numbers).
profile="${SPREFA_LSP_PROFILE:-debug}"
bin="./target/$profile/sprefa-server"
if [ ! -x "$bin" ]; then
    echo "building sprefa-server ($profile)..." >&2
    if [ "$profile" = "release" ]; then
        cargo build -p server --bin sprefa-server --release >&2
    else
        cargo build -p server --bin sprefa-server >&2
    fi
fi
# --lsp-stdio: bypass HTTP and run tower-lsp directly on stdio (the
# old sprefa-lsp binary was a thin wrapper for this).
LSP_ARGS=("--lsp-stdio")

# Fixture + hover position can be overridden so this script doubles as
# an ad-hoc "probe any position in any file" tool. Defaults exercise
# `ast[rs](format!(${X?}))` on copy_audit.sprf line 11.
fixture="${SPREFA_LSP_FIXTURE:-$v3_dir/crates/server/fixtures/copy_audit.sprf}"
HOVER_LINE="${SPREFA_LSP_LINE:-10}"
HOVER_CHAR="${SPREFA_LSP_CHAR:-48}"
uri="file://$fixture"
text="$(cat "$fixture")"

# Per-test deadline. Hover currently runs the full prefix pipe end to end
# (see backend.rs render_enriched_hover); a fresh cold cache on the v3
# tree should be well under this. Tune down once hover budgets land.
DEADLINE_S="${SPREFA_LSP_HOVER_DEADLINE_S:-20}"

OUT="$(mktemp -t sprefa_lsp_out.XXXX)"
ERR="$(mktemp -t sprefa_lsp_err.XXXX)"
PIPE="$(mktemp -u -t sprefa_lsp_in.XXXX)"
mkfifo "$PIPE"
PID=""
WRITER_FD=9

cleanup() {
    if [ -n "${WRITER_OPEN:-}" ]; then
        eval "exec ${WRITER_FD}>&-" || true
    fi
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        kill -TERM "$PID" 2>/dev/null || true
        sleep 0.3
        kill -KILL "$PID" 2>/dev/null || true
    fi
    rm -f "$PIPE"
    if [ "${VERBOSE:-0}" = "1" ]; then
        echo "--- stdout ---" >&2; cat "$OUT" >&2 || true
        echo "--- stderr ---" >&2; cat "$ERR" >&2 || true
    fi
    rm -f "$OUT" "$ERR"
}
trap cleanup EXIT INT TERM

send() {
    local body="$1"
    local len=${#body}
    printf 'Content-Length: %d\r\n\r\n%s' "$len" "$body" >&${WRITER_FD}
}

text_json="$(python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))' <<<"$text")"
init='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":"file://'"$v3_dir"'","capabilities":{}}}'
inited='{"jsonrpc":"2.0","method":"initialized","params":{}}'
didopen='{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"'"$uri"'","languageId":"sprefa","version":1,"text":'"$text_json"'}}}'

# Hover target: copy_audit.sprf line 11 ('ast' in
# `... > ast[rs](format!(${X?}));`). LSP positions are 0-indexed.
hover='{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"'"$uri"'"},"position":{"line":'"$HOVER_LINE"',"character":'"$HOVER_CHAR"'}}}'

"$bin" "${LSP_ARGS[@]}" <"$PIPE" >"$OUT" 2>"$ERR" &
PID=$!

eval "exec ${WRITER_FD}>\"$PIPE\""
WRITER_OPEN=1

send "$init"
sleep 0.3
send "$inited"
send "$didopen"
sleep 0.3
t_start=$SECONDS
send "$hover"

deadline=$((SECONDS + DEADLINE_S))
ok=0
while (( SECONDS < deadline )); do
    # Server crashed or exited mid-flight.
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "FAIL: lsp server exited before responding" >&2
        echo "--- stderr ---" >&2; cat "$ERR" >&2 || true
        exit 1
    fi
    if grep -q '"id":2' "$OUT" 2>/dev/null; then
        ok=1
        break
    fi
    sleep 0.2
done

if [ "$ok" -ne 1 ]; then
    echo "FAIL: hover did not respond within ${DEADLINE_S}s" >&2
    echo "--- stderr (tail) ---" >&2; tail -40 "$ERR" >&2 || true
    exit 1
fi

elapsed=$((SECONDS - t_start))

# Reject explicit error envelope.
if grep -q '"id":2,"error"' "$OUT" 2>/dev/null; then
    echo "FAIL: hover returned an error envelope" >&2
    grep -o '"id":2[^}]*"error"[^}]*}' "$OUT" >&2 || true
    exit 1
fi

# Reject a null result (means hover plan resolved to nothing).
if grep -q '"result":null,"id":2' "$OUT" 2>/dev/null; then
    echo "FAIL: hover returned null result for ast[rs] op" >&2
    exit 1
fi

# Reject a panic on stderr even if some response leaked through.
if grep -q "panicked at" "$ERR" 2>/dev/null; then
    echo "FAIL: server panicked during hover" >&2
    grep "panicked at" "$ERR" >&2 || true
    exit 1
fi

# Sanity: response carries a hover contents payload.
if ! grep -q '"contents"' "$OUT" 2>/dev/null; then
    echo "FAIL: hover response present but missing contents" >&2
    exit 1
fi

echo "OK (hover returned in ${elapsed}s, deadline ${DEADLINE_S}s)"
