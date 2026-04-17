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

SPREFA_SERVER_BIN="$V2_DIR/target/debug/sprefa-server"
SPREFA_CLI_BIN="$V2_DIR/target/debug/sprefa"
SPREFA_LSP_BIN="$V2_DIR/target/debug/sprefa-lsp"

export SPREFA_SERVER_INFO="$STATE_DIR/server.json"
export SPREFA_HTTP_UNIX="$STATE_DIR/sprefa.sock"
export SPREFA_STORE_PATH="$STATE_DIR/store.sqlite"
export SPREFA_LOG_PATH="$STATE_DIR/server.log"

reset_state() {
    rm -rf "$STATE_DIR"
    mkdir -p "$STATE_DIR"
}

require_bins() {
    for bin in "$SPREFA_SERVER_BIN" "$SPREFA_CLI_BIN" "$SPREFA_LSP_BIN"; do
        [ -x "$bin" ] || { echo "missing binary: $bin — run \`cargo build\` in v2/" >&2; exit 2; }
    done
}

start_server() {
    "$SPREFA_SERVER_BIN" --no-tcp --foreground \
        > "$STATE_DIR/server.stderr" 2>&1 &
    SRV_PID=$!
    for _ in $(seq 1 50); do
        [ -e "$SPREFA_SERVER_INFO" ] && [ -e "$SPREFA_HTTP_UNIX" ] && return 0
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
        "$SPREFA_CLI_BIN" stop > /dev/null 2>&1 \
            || kill -TERM "$SRV_PID" 2>/dev/null \
            || true
        wait "$SRV_PID" 2>/dev/null || true
    fi
}

trap stop_server EXIT
