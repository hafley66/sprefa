#!/usr/bin/env bash
# Shared helpers for sprefa server smoke scripts.
#
# State lives under v2/tests/smoke/.state/ (gitignored). Each script points
# SPREFA_SERVER_INFO / SPREFA_HTTP_UNIX / SPREFA_STORE_PATH / SPREFA_LOG_PATH
# inside that dir so no XDG machinery is needed and the scripts are
# self-contained.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V2_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_ROOT="$(cd "$V2_DIR/.." && pwd)"
STATE_DIR="$SCRIPT_DIR/.state"

SPREFA_PROFILE="${SPREFA_PROFILE:-debug}"
SPREFA_SERVER_BIN="$V2_DIR/target/$SPREFA_PROFILE/sprefa-server"
SPREFA_CLI_BIN="$V2_DIR/target/$SPREFA_PROFILE/sprefa"
SPREFA_LSP_BIN="$V2_DIR/target/$SPREFA_PROFILE/sprefa-lsp"

export SPREFA_SERVER_INFO="$STATE_DIR/server.json"
export SPREFA_HTTP_UNIX="$STATE_DIR/sprefa.sock"
export SPREFA_STORE_PATH="$STATE_DIR/store.sqlite"
export SPREFA_LOG_PATH="$STATE_DIR/server.log"

reset_state() {
    rm -rf "$STATE_DIR"
    mkdir -p "$STATE_DIR"
}

# Milliseconds since epoch. Bash 5 EPOCHREALTIME has µs; trim to ms.
now_ms() {
    local t="${EPOCHREALTIME:-}"
    if [ -n "$t" ]; then
        # "1713380000.123456" -> "1713380000123"
        echo "${t%.*}${t#*.}" | cut -c1-13
    else
        # Fallback: seconds only.
        echo "$(date +%s)000"
    fi
}

# Run a command, prefix its wall time. Usage: `timed LABEL cmd args...`
timed() {
    local label="$1"; shift
    local t0 t1
    t0="$(now_ms)"
    "$@"
    local rc=$?
    t1="$(now_ms)"
    printf '  [%-24s] %5d ms\n' "$label" "$((t1 - t0))" >&2
    return "$rc"
}

require_bins() {
    for bin in "$SPREFA_SERVER_BIN" "$SPREFA_CLI_BIN" "$SPREFA_LSP_BIN"; do
        [ -x "$bin" ] || { echo "missing binary: $bin — run \`cargo build\` in v2/" >&2; exit 2; }
    done
}

start_server() {
    local t0 t1
    t0="$(now_ms)"
    "$SPREFA_SERVER_BIN" --no-tcp --foreground \
        > "$STATE_DIR/server.stderr" 2>&1 &
    SRV_PID=$!
    for _ in $(seq 1 50); do
        if [ -e "$SPREFA_SERVER_INFO" ] && [ -e "$SPREFA_HTTP_UNIX" ]; then
            t1="$(now_ms)"
            printf '  [%-24s] %5d ms\n' "start_server" "$((t1 - t0))" >&2
            return 0
        fi
        sleep 0.1
    done
    echo "server failed to come up; stderr:" >&2
    cat "$STATE_DIR/server.stderr" >&2
    return 1
}

stop_server() {
    # In-band: POST /shutdown via `sprefa stop`. Falls back to SIGTERM if the
    # CLI can't reach the server (e.g. startup failure).
    if [ -n "${SRV_PID:-}" ]; then
        local t0 t1
        t0="$(now_ms)"
        "$SPREFA_CLI_BIN" stop > /dev/null 2>&1 \
            || kill -TERM "$SRV_PID" 2>/dev/null \
            || true
        wait "$SRV_PID" 2>/dev/null || true
        t1="$(now_ms)"
        printf '  [%-24s] %5d ms\n' "stop_server" "$((t1 - t0))" >&2
    fi
}

trap stop_server EXIT
