#!/usr/bin/env bash
# Smoke k: lsp[severity](...) diagnostics surface through sprefa-run as
# SSE Diag frames. Proves end-to-end wire-up:
#   pipeline lsp op -> LspDiagEffect -> LspDiagBatcher buffer (in
#   build_seed_ctx) -> per-seed drain in run.rs -> RunEvent::Diag SSE
#   frame -> sprefa-run print_frame stderr.
#
# Without this wiring the lsp op fires effects into the void.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"

bin="$v3_dir/target/debug/sprefa-run"
if [ ! -x "$bin" ]; then
    echo "building sprefa-run..." >&2
    (cd "$v3_dir" && cargo build -p server --bin sprefa-run >&2)
fi

# Use /tmp explicitly (not the default mktemp /var/folders dir on macOS).
# macOS /var/folders dirs can mask the seed.root symlink resolution and
# leave fs(glob(...)) returning 0 rows even though the file is right
# there. /tmp -> /private/tmp is the same kind of symlink but
# DiskFileSource handles it.
mkdir -p /tmp/sprefa_lspdiag_work
work="$(mktemp -d /tmp/sprefa_lspdiag_work/run.XXXX)"
trap 'rm -rf "$work"' EXIT

cat >"$work/sample.rs" <<'EOF'
fn main() {
    let a: Option<i32> = Some(1);
    let b: Option<i32> = Some(2);
    let _x = a.unwrap();
    let _y = b.unwrap();
    todo!();
}
EOF

# Two pipes, two severities, one shared matcher (.unwrap()) plus a
# todo!() expansion. Exercises Warning + Error + Info paths.
cat >"$work/lint.sprf" <<'EOF'
fs(glob(**/sample.rs))
  > ast[rust](${V?}.unwrap())
  > lsp[warn](message = "unwrap in production path", code = "no-unwrap");

fs(glob(**/sample.rs))
  > ast[rust](${V?}.unwrap())
  > lsp[error](message = "second pass demoted", code = "no-unwrap2");

fs(glob(**/sample.rs))
  > ast[rust](todo!())
  > lsp[info](message = "todo macro shipped", code = "todo-shipped");
EOF

err_log="$work/err.log"
out_log="$work/out.log"

# Isolate from the user's running daemon (VS Code LSP, other smokes).
# Each smoke gets its own daemon in $work so cwd-dependent seed
# resolution and watcher state don't leak between tests. --no-tcp so
# the smoke does not collide on 127.0.0.1:7417 with whatever else is
# running.
export SPREFA_SERVER_INFO="$work/server.json"
export SPREFA_HTTP_UNIX="$work/sprefa.sock"

server_bin="$v3_dir/target/debug/sprefa-server"
if [ ! -x "$server_bin" ]; then
    (cd "$v3_dir" && cargo build -p server --bin sprefa-server >&2)
fi
"$server_bin" \
    --no-tcp \
    --http-unix "$SPREFA_HTTP_UNIX" \
    --info-path "$SPREFA_SERVER_INFO" \
    --log-path "$work/server.log" \
    >/dev/null 2>"$work/server.err" &
server_pid=$!
trap 'kill -TERM '"$server_pid"' 2>/dev/null; rm -rf "$work"' EXIT

# Wait for the daemon to become reachable on its unix socket.
deadline=$((SECONDS + 5))
while (( SECONDS < deadline )); do
    if [ -e "$SPREFA_HTTP_UNIX" ] && \
       python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.settimeout(0.2); s.connect(sys.argv[1])' "$SPREFA_HTTP_UNIX" 2>/dev/null; then
        break
    fi
    sleep 0.1
done

(cd "$work" && "$bin" "lint.sprf" --root .) >"$out_log" 2>"$err_log" || {
    echo "FAIL: sprefa-run exited non-zero" >&2
    echo "--- stderr ---" >&2; cat "$err_log" >&2
    echo "--- stdout ---" >&2; cat "$out_log" >&2
    exit 1
}

if ! grep -q '\[Warning no-unwrap\]' "$err_log"; then
    echo "FAIL: missing [Warning no-unwrap] diag in stderr" >&2
    echo "--- stderr ---" >&2; cat "$err_log" >&2
    exit 1
fi

if ! grep -q '\[Error no-unwrap2\]' "$err_log"; then
    echo "FAIL: missing [Error no-unwrap2] diag in stderr" >&2
    echo "--- stderr ---" >&2; cat "$err_log" >&2
    exit 1
fi

if ! grep -q '\[Info todo-shipped\]' "$err_log"; then
    echo "FAIL: missing [Info todo-shipped] diag in stderr" >&2
    echo "--- stderr ---" >&2; cat "$err_log" >&2
    exit 1
fi

if ! grep -q 'unwrap in production path' "$err_log"; then
    echo "FAIL: warn message text missing" >&2
    exit 1
fi

# Range present: format is "<file> <start>..<end>: <msg>".
if ! grep -qE 'sample.rs [0-9]+\.\.[0-9]+: unwrap in production path' "$err_log"; then
    echo "FAIL: warn diag missing file/byte_range header" >&2
    cat "$err_log" >&2
    exit 1
fi

# The unwrap()-matching pipe should fire twice (a.unwrap and b.unwrap).
warn_count="$(grep -c '\[Warning no-unwrap\]' "$err_log" || true)"
if [ "$warn_count" -lt 2 ]; then
    echo "FAIL: expected >=2 [Warning no-unwrap] diags (one per match), got $warn_count" >&2
    cat "$err_log" >&2
    exit 1
fi

echo "OK lsp[severity] diagnostics surfaced via SSE (warn=$warn_count)"
