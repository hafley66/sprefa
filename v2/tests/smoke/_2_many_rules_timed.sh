#!/usr/bin/env bash
# Smoke 2 with server-side SPREFA_TIMING=1. Starts the server with the env
# var so per-expr + stream_total timings print to server.stderr, runs the
# g4_many_rules.sprf workload, then dumps the timing lines.
#
# Env overrides:
#   SPREFA_PROFILE  debug|release (default: debug)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

require_bins
reset_state

[ -d "$REPO_ROOT/.git" ] || { echo "SKIP: $REPO_ROOT is not a git repo"; exit 0; }

# Start server with SPREFA_TIMING=1 in its env.
t0="$(now_ms)"
SPREFA_TIMING=1 "$SPREFA_SERVER_BIN" --no-tcp --foreground \
    > "$STATE_DIR/server.stderr" 2>&1 &
SRV_PID=$!
for _ in $(seq 1 50); do
    if [ -e "$SPREFA_SERVER_INFO" ] && [ -e "$SPREFA_HTTP_UNIX" ]; then
        break
    fi
    sleep 0.1
done
t1="$(now_ms)"
printf '  [%-24s] %5d ms\n' "start_server" "$((t1 - t0))" >&2

t0="$(now_ms)"
"$SPREFA_CLI_BIN" run "$V2_DIR/examples/g4_many_rules.sprf" --root "$REPO_ROOT" > /dev/null
t1="$(now_ms)"
printf '  [%-24s] %5d ms\n' "sprefa run" "$((t1 - t0))" >&2

"$SPREFA_CLI_BIN" stop > /dev/null 2>&1 || kill -TERM "$SRV_PID" 2>/dev/null || true
wait "$SRV_PID" 2>/dev/null || true

echo "--- server timing ---"
grep -E '^\[timing\]' "$STATE_DIR/server.stderr" || echo "(no timing lines)"
