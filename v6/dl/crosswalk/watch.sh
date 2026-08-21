#!/usr/bin/env bash
# @comment-ok: the measurement's protocol and the two facts it exists to check.
# watch.sh -- hold crosswalk.dl6 resident and measure what it costs.
#
#   bash v6/dl/crosswalk/watch.sh grafana [seconds] [tick-seconds]
#
# THE RESIDENT DOOR IS `--socket`. `emit_rust_harness` folds the schedule first
# and only then hands the settled seam to serve.rs, so a resident process
# answers reads over an already-folded db (src/bin/emit_rust_harness.rs, the
# `if let Some(path)` tail).
#
# A FILE TOUCH DRIVES NOTHING ON THIS DOOR. `registry.pl` declares a `watch`
# bind whose executor is `live_watch`, and nothing in `sprefa-engine-rs`
# implements one: `serve.rs` has a `/arrive` route and no watcher. The touch is
# performed anyway and reported, because "the touch produced no tick" is the
# measurement, not an omission.
#
# EACH TICK POSTS A NEW `crosswalk_run` ROW. Re-posting an identical row is zero
# delta at the rel boundary and would measure nothing; a new row re-derives, and
# after the first tick every derived row is already present, so the add-set is
# empty and RSS is the thing under test.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURE="${1:-grafana}"
DURATION="${2:-300}"
TICK_EVERY="${3:-30}"
SAMPLE_EVERY="${SAMPLE_EVERY:-10}"

SOCKET="$(mktemp -u "${TMPDIR:-/tmp}/crosswalk-watch.XXXXXX.sock")"
LOG="$(mktemp -d "${TMPDIR:-/tmp}/crosswalk-watch.XXXXXX")"
CHECKOUT_ROOT="$(bash "$HERE/fixtures/$FIXTURE.sh" --print-root)"
TOUCH_TARGET="$CHECKOUT_ROOT/.crosswalk-watch-touch"

cleanup() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null || true
  rm -f "$SOCKET" "$TOUCH_TARGET"
}
trap cleanup EXIT

CROSSWALK_RELS=repo_file_count bash "$HERE/run.sh" "$FIXTURE" --socket "$SOCKET" \
  >"$LOG/server.out" 2>"$LOG/server.err" &
SERVER=$!

for _ in $(seq 1 60); do
  [ -S "$SOCKET" ] && break
  sleep 0.5
done
[ -S "$SOCKET" ] || { printf 'no socket at %s: %s\n' "$SOCKET" "$(tail -5 "$LOG/server.err")" >&2; exit 1; }

# The engine thread is a child of the harness process, so one RSS reading covers
# the seam, the memo tables and the axum runtime together.
rss_of() { ps -o rss= -p "$SERVER" | tr -d ' '; }

post() {
  timeout 30 curl -s --unix-socket "$SOCKET" -X POST http://localhost/arrive \
    -H 'content-type: application/json' -d "$1"
}

printf 'elapsed_s\tevent\trss_kb\ttick_ms\tadded_rows\n'
started="$(date +%s)"
baseline="$(rss_of)"
printf '0\tresident\t%s\t\t\n' "$baseline"

tick=0
while :; do
  elapsed=$(( $(date +%s) - started ))
  [ "$elapsed" -ge "$DURATION" ] && break
  if [ $(( elapsed % TICK_EVERY )) -lt "$SAMPLE_EVERY" ] && [ "$elapsed" -gt 0 ]; then
    tick=$(( tick + 1 ))
    date +%s > "$TOUCH_TARGET"
    at="$(date +%s%N)"
    answer="$(post "[{\"rel\":\"crosswalk_run\",\"sign\":\"add\",\"row\":[\"$FIXTURE-tick-$tick\"]}]")"
    took=$(( ( $(date +%s%N) - at ) / 1000000 ))
    added="$(printf '%s' "$answer" | tr ',' '\n' | grep -c '"add"' || true)"
    printf '%s\ttick-%s\t%s\t%s\t%s\n' "$elapsed" "$tick" "$(rss_of)" "$took" "$added"
  else
    printf '%s\tsample\t%s\t\t\n' "$elapsed" "$(rss_of)"
  fi
  sleep "$SAMPLE_EVERY"
done

final="$(rss_of)"
printf 'baseline_kb=%s final_kb=%s growth_pct=%s ticks=%s\n' \
  "$baseline" "$final" \
  "$(awk -v a="$baseline" -v b="$final" 'BEGIN { printf "%.2f", (b - a) * 100.0 / a }')" \
  "$tick"
