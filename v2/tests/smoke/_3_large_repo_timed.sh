#!/usr/bin/env bash
# Large-repo timed smoke. Shallow-clones a pinned combo fixture (rust + ts in
# one tree), configures it as a sprefa repo, runs a 4-rule workload, prints
# [timing] lines (expr/stream_total/intern + sprefa run wall).
#
# Fixture cache: $SCRIPT_DIR/.fixtures/ (gitignored, preserved across runs so
# reset_state stays cheap). First invocation clones (~1-3 min network);
# subsequent runs reuse the checkout.
#
# Env overrides:
#   SPREFA_PROFILE          debug|release (default: debug)
#   SPREFA_PARSE_CACHE_DIR  persistent parse cache dir (opt-in)
#   SPREFA_LARGE_ADD_DENO   set to 1 to also clone denoland/deno for more ts
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

FIXTURE_DIR="$SCRIPT_DIR/.fixtures"
mkdir -p "$FIXTURE_DIR"

clone_pinned() {
    local name="$1" url="$2" ref="$3"
    local dst="$FIXTURE_DIR/$name"
    if [ -d "$dst/.git" ]; then
        return 0
    fi
    echo "cloning $url @ $ref into $dst (first-run, cached thereafter)"
    git clone --depth 1 --branch "$ref" "$url" "$dst"
}

clone_pinned swc  https://github.com/swc-project/swc.git  v1.10.11
if [ "${SPREFA_LARGE_ADD_DENO:-0}" = "1" ]; then
    clone_pinned deno https://github.com/denoland/deno.git v2.1.4
fi

count_ext() {
    local dir="$1"; shift
    find "$dir" \( "$@" \) -not -path '*/.git/*' -type f 2>/dev/null | wc -l | tr -d ' '
}

total_rs=0
total_ts=0
repos_toml=""
for name in swc deno; do
    dst="$FIXTURE_DIR/$name"
    [ -d "$dst/.git" ] || continue
    rs=$(count_ext "$dst" -name '*.rs')
    ts=$(count_ext "$dst" -name '*.ts' -o -name '*.tsx')
    printf '  fixture %-6s rs=%6d ts/tsx=%6d\n' "$name" "$rs" "$ts"
    total_rs=$((total_rs + rs))
    total_ts=$((total_ts + ts))
    repos_toml+=$'[[repo]]\nslug = "'"$name"$'"\npath = "'"$dst"$'"\nrevs = ["HEAD"]\n\n'
done
printf '  fixture TOTAL  rs=%6d ts/tsx=%6d\n' "$total_rs" "$total_ts"

SPRF_CFG="$FIXTURE_DIR/.sprefa.toml"
printf '%s' "$repos_toml" > "$SPRF_CFG"

SPRF_FILE="$FIXTURE_DIR/large_combo.sprf"
cat > "$SPRF_FILE" <<'EOF'
# Large-repo combo: 2 rust rules + 2 ts rules across pinned fixtures.
# Exercises parse cache, intern, and reader stack at >5k files / language.
rule(pub_fn) {
  > repo($R) > rev($V) > fs(**/*.rs)
    > ast[rust](pub fn $NAME($$$ARGS) { $$$BODY })
};

rule(pub_struct) {
  > repo($R) > rev($V) > fs(**/*.rs)
    > ast[rust](pub struct $NAME { $$$FIELDS })
};

rule(ts_fn) {
  > repo($R) > rev($V) > fs(**/*.ts)
    > ast[typescript](function $NAME($$$ARGS) { $$$BODY })
};

rule(ts_class) {
  > repo($R) > rev($V) > fs(**/*.ts)
    > ast[typescript](class $NAME { $$$MEMBERS })
};
EOF

require_bins
reset_state

t0="$(now_ms)"
SPREFA_TIMING=1 "$SPREFA_SERVER_BIN" --no-tcp --foreground \
    > "$STATE_DIR/server.stderr" 2>&1 &
SRV_PID=$!
for _ in $(seq 1 100); do
    if [ -e "$SPREFA_SERVER_INFO" ] && [ -e "$SPREFA_HTTP_UNIX" ]; then
        break
    fi
    sleep 0.1
done
t1="$(now_ms)"
printf '  [%-24s] %5d ms\n' "start_server" "$((t1 - t0))" >&2

t0="$(now_ms)"
"$SPREFA_CLI_BIN" run "$SPRF_FILE" --root "$FIXTURE_DIR" > /dev/null
t1="$(now_ms)"
printf '  [%-24s] %5d ms\n' "sprefa run" "$((t1 - t0))" >&2

"$SPREFA_CLI_BIN" stop > /dev/null 2>&1 || kill -TERM "$SRV_PID" 2>/dev/null || true
wait "$SRV_PID" 2>/dev/null || true

echo "--- server timing ---"
grep -E '^\[timing\]' "$STATE_DIR/server.stderr" || echo "(no timing lines)"
