#!/usr/bin/env bash
# Path-index perf gates (Session 6e/6f/6g). One rule, one repo (swc pinned by
# _3_large_repo_timed.sh fixture cache). Three phases: cold, warm (same
# server), restart-warm (fresh server, preserved wt cache + pi.sqlite).
#
# Env overrides:
#   SPREFA_PROFILE          debug|release (default: debug)
#
# Gates (docs/perf_20260417.md target):
#   cold          < 4000 ms
#   warm          <  100 ms
#   restart-warm  <  200 ms
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

FIXTURE_DIR="$SCRIPT_DIR/.fixtures"
PI_CACHE_DIR="$SCRIPT_DIR/.path_index_cache"
SWC_DIR="$FIXTURE_DIR/swc"

[ -d "$SWC_DIR/.git" ] || {
    echo "swc fixture missing; run _3_large_repo_timed.sh once to clone it" >&2
    exit 2
}

# Fresh wt cache + pi.sqlite for the cold run.
rm -rf "$PI_CACHE_DIR"
mkdir -p "$PI_CACHE_DIR/wt"

export SPREFA_WT_CACHE_DIR="$PI_CACHE_DIR/wt"
export SPREFA_PATH_INDEX_DB="$PI_CACHE_DIR/pi.sqlite"
export SPREFA_TIMING=1
export SPREFA_SPAN_LOG=1
# Periscope: per-phase Chrome trace. Open in ui.perfetto.dev.

SPRF_CFG="$PI_CACHE_DIR/.sprefa.toml"
cat > "$SPRF_CFG" <<EOF
[[repo]]
slug = "swc"
path = "$SWC_DIR"
revs = ["HEAD"]
EOF

SPRF_FILE="$PI_CACHE_DIR/one_rule.sprf"
cat > "$SPRF_FILE" <<'EOF'
# Single-rule fs scan over swc rust files. Exercises path_index end-to-end.
rule(pub_fn) {
  > repo($R) > rev(HEAD) > fs(**/*.rs)
    > ast[rust](pub fn $NAME($$$ARGS) { $$$BODY })
};
EOF

require_bins

run_rule() {
    local label="$1"
    local t0 t1
    t0="$(now_ms)"
    "$SPREFA_CLI_BIN" run "$SPRF_FILE" --root "$PI_CACHE_DIR" > /dev/null
    t1="$(now_ms)"
    printf '  [%-24s] %5d ms\n' "$label" "$((t1 - t0))" >&2
}

# --- Phase 1: cold + warm ------------------------------------------------
reset_state
export SPREFA_TRACE="$PI_CACHE_DIR/trace.cold_warm.json"
start_server
run_rule "cold"
run_rule "warm (same server)"
stop_server

# --- Phase 2: restart-warm ----------------------------------------------
# reset_state nukes .state/ (server socket + store.sqlite) but leaves
# .path_index_cache/ alone, so wt + pi.sqlite survive the bounce.
reset_state
export SPREFA_TRACE="$PI_CACHE_DIR/trace.restart_warm.json"
start_server
run_rule "restart-warm"
stop_server

# --- Disk budget ---------------------------------------------------------
echo "--- disk ---"
du -sh "$PI_CACHE_DIR/wt" "$PI_CACHE_DIR/pi.sqlite" 2>/dev/null || true

# --- Periscope traces ----------------------------------------------------
echo "--- traces (open in https://ui.perfetto.dev) ---"
ls -lh "$PI_CACHE_DIR"/trace.*.json 2>/dev/null || echo "(no traces written)"

# --- Timing lines --------------------------------------------------------
echo "--- server timing ---"
grep -E '^\[timing\]' "$STATE_DIR/server.stderr" || echo "(no timing lines in last server.stderr)"
