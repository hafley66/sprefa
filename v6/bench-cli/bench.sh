#!/usr/bin/env bash
# bench.sh — the language-agnostic CLI bench harness (CONTRACT.md).
#
# For every case in cases.json:
#   1. resolve (program, schedule). Scale cases generate their schedule at the
#      declared row count; program cases point at corpus artifacts.
#   2. run the ORACLE adapter once. Its stdout is the reference log.
#   3. run every other engine BENCH_RUNS times. Byte-diff run 1's stdout
#      against the reference; only an `identical` engine keeps its timings.
#   4. append one record per (case, engine) to out/records.jsonl.
# Then report.ts renders standings.csv + STANDINGS.md.
#
# The v1-asymmetry rule is enforced in step 3 and nowhere else: a `wrong`,
# `refused` or `error` engine gets N/A timings with the reason attached, never
# a number. SCALE.md's "v1 is 10x faster" was measured against an engine that
# emitted no delta log at all; under this harness that run produces no number.
#
# Env:
#   BENCH_RUNS=5        repeats per timed engine (median reported)
#   BENCH_CASES=a,b     restrict to these case names
#   BENCH_TIMEOUT=600   per-run seconds
#   TSV2_HEAP_MB=512    node old-space ceiling for the tsv2 engine
#
# Usage: cd v6 && just bench-cli     (or: bash v6/bench-cli/bench.sh)

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

OUT="$HERE/out"
mkdir -p "$OUT"
RECORDS="$OUT/records.jsonl"
: > "$RECORDS"

RUNS="${BENCH_RUNS:-5}"
TIMEOUT="${BENCH_TIMEOUT:-600}"
ONLY="${BENCH_CASES:-}"

# hyperfine is the right tool for the EXTERNAL wall column and the wrong one
# for the primary number (CONTRACT.md section 1, candidate 1). Detected, never
# required; its absence changes one column's provenance and nothing else.
if command -v hyperfine >/dev/null 2>&1; then
  EXTERNAL_TIMER="hyperfine $(hyperfine --version | awk '{print $2}')"
else
  EXTERNAL_TIMER="repeat-loop (hyperfine not installed)"
fi
echo "== bench-cli =="
echo "   runs/engine      $RUNS"
echo "   external timer   $EXTERNAL_TIMER"
echo "   peak rss         /usr/bin/time -l"
echo ""

# `perl -e alarm` is the house timeout (v6/sprefa-store/bench/engines/tsv2_gen.sh
# uses exactly this); macOS has no coreutils `timeout` by default.
run_capped() {
  perl -e "alarm shift; exec @ARGV" "$TIMEOUT" "$@"
}

engine_cmd() {
  case "$1" in
    oracle) echo "$HERE/adapters/oracle.sh" ;;
    tsv2)   echo "$HERE/adapters/tsv2.sh" ;;
    *) return 1 ;;
  esac
}

# ── walk the cases ──────────────────────────────────────────────────────────
case_count=$(node -e 'process.stdout.write(String(require("./cases.json").length))')
for index in $(seq 0 $((case_count - 1))); do
  eval "$(node -e '
    const c = require("./cases.json")[Number(process.argv[1])];
    const esc = (v) => String(v ?? "").replace(/'"'"'/g, "");
    process.stdout.write(
      `CASE_NAME='"'"'${esc(c.case)}'"'"'\n` +
      `CASE_FAMILY='"'"'${esc(c.family)}'"'"'\n` +
      `CASE_PROGRAM='"'"'${esc(c.program)}'"'"'\n` +
      `CASE_SCHEDULE='"'"'${esc(c.schedule)}'"'"'\n` +
      `CASE_SHAPE='"'"'${esc(c.shape)}'"'"'\n` +
      `CASE_ROWS='"'"'${esc(c.rows)}'"'"'\n` +
      `CASE_NOTE='"'"'${esc(c.note)}'"'"'\n`);
  ' "$index")"

  if [ -n "$ONLY" ] && [[ ",$ONLY," != *",$CASE_NAME,"* ]]; then continue; fi

  SAFE="$(echo "$CASE_NAME" | tr '/' '_')"

  # Scale cases mint their schedule; program cases already have one on disk.
  if [ "$CASE_FAMILY" = "scale" ]; then
    CASE_SCHEDULE="$OUT/$SAFE.schedule.json"
    node --experimental-transform-types schedule-gen.ts "$CASE_SHAPE" "$CASE_ROWS" "$CASE_SCHEDULE" 2>/dev/null
  fi

  if [ ! -f "$CASE_PROGRAM" ] || [ ! -f "$CASE_SCHEDULE" ]; then
    echo "SKIP $CASE_NAME (missing program or schedule)"
    continue
  fi

  # Input hash: sha256 over program bytes || NUL || schedule bytes, first 16
  # hex. PERF-REPORT's "all engines must match" identity check.
  INPUT_HASH=$(node -e '
    const { createHash } = require("node:crypto");
    const { readFileSync } = require("node:fs");
    const h = createHash("sha256");
    h.update(readFileSync(process.argv[1]));
    h.update(Buffer.from([0]));
    h.update(readFileSync(process.argv[2]));
    process.stdout.write(h.digest("hex").slice(0, 16));
  ' "$CASE_PROGRAM" "$CASE_SCHEDULE")

  echo "-- $CASE_NAME  [$INPUT_HASH]"

  # ── reference leg ─────────────────────────────────────────────────────────
  REF_LOG="$OUT/$SAFE.oracle.log"
  run_capped "$(engine_cmd oracle)" \
      --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
      --db ":memory:" --perf-out "$OUT/$SAFE.oracle.perf.json" \
      > "$REF_LOG" 2> "$OUT/$SAFE.oracle.err"
  ORACLE_STATUS=$?

  for ENGINE in oracle tsv2; do
    LOG="$OUT/$SAFE.$ENGINE.log"
    ERR="$OUT/$SAFE.$ENGINE.err"
    PERF="$OUT/$SAFE.$ENGINE.perf.json"
    TIME_FILE="$OUT/$SAFE.$ENGINE.time"
    WALLS=""
    PEAK_RSS_KB=0
    STATUS=0

    if [ "$ENGINE" = "oracle" ]; then
      if [ "$ORACLE_STATUS" -ne 0 ]; then
        VERDICT="error"
      else
        VERDICT="reference"
      fi
    fi

    for run in $(seq 1 "$RUNS"); do
      [ "$ENGINE" = "oracle" ] && [ "$VERDICT" = "error" ] && break
      TSV2_ENV=""
      [ "$ENGINE" = "tsv2" ] && TSV2_ENV="NODE_OPTIONS=--max-old-space-size=${TSV2_HEAP_MB:-512}"
      env $TSV2_ENV /usr/bin/time -l \
        "$(engine_cmd "$ENGINE")" \
          --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
          --db ":memory:" --perf-out "$PERF" \
        > "$LOG" 2> "$TIME_FILE"
      STATUS=$?
      grep -v 'maximum resident set size\|average shared\|average unshared\|page reclaims\|page faults\|swaps\|block input\|block output\|messages sent\|messages received\|signals received\|context switches\|instructions retired\|cycles elapsed\|peak memory footprint\|real  *[0-9]' "$TIME_FILE" > "$ERR" 2>/dev/null

      RSS_KB=$(awk '/maximum resident set size/{print int($1/1024)}' "$TIME_FILE" | head -1)
      [ -n "$RSS_KB" ] && [ "$RSS_KB" -gt "$PEAK_RSS_KB" ] && PEAK_RSS_KB="$RSS_KB"

      # First run decides the verdict; later runs only contribute timings.
      if [ "$run" -eq 1 ] && [ "$ENGINE" != "oracle" ]; then
        if [ "$STATUS" -eq 2 ]; then
          VERDICT="refused"
        elif [ "$STATUS" -ne 0 ]; then
          VERDICT="error"
        elif cmp -s "$REF_LOG" "$LOG"; then
          VERDICT="identical"
        else
          VERDICT="wrong"
        fi
      fi
      # Only a byte-identical engine is timed. Break out otherwise: repeating
      # a disqualified run buys nothing and the number would never be printed.
      if [ "$ENGINE" != "oracle" ] && [ "$VERDICT" != "identical" ]; then break; fi

      W=$(node -e '
        try { const p = require("node:fs").readFileSync(process.argv[1], "utf8");
              process.stdout.write(String(JSON.parse(p).wall_ms)); }
        catch { process.stdout.write("null"); }
      ' "$PERF" 2>/dev/null)
      WALLS="$WALLS $W"
    done

    node -e '
      const fs = require("node:fs");
      const [file, name, family, engine, verdict, hash, wallsText, rssKb, note, timer, runs, program, schedule] =
        process.argv.slice(1);
      let perf = {};
      try { perf = JSON.parse(fs.readFileSync(file, "utf8")); } catch { perf = {}; }
      const walls = wallsText.trim().split(/\s+/).map(Number).filter((n) => Number.isFinite(n)).sort((a, b) => a - b);
      const median = walls.length === 0 ? null : walls[Math.floor((walls.length - 1) / 2)];
      process.stdout.write(JSON.stringify({
        case: name, family, engine, verdict, input_hash: hash, note,
        wall_ms: median, wall_samples: walls,
        compile_ms: perf.compile_ms ?? "N/A",
        ticks: perf.ticks ?? "N/A",
        statements: perf.statements ?? "N/A",
        db_bytes: perf.db_bytes ?? "N/A",
        peak_rss_mb: Number(rssKb) > 0 ? Number((Number(rssKb) / 1024).toFixed(1)) : "N/A",
        engine_notes: perf.notes ?? {},
        external_timer: timer, runs: Number(runs),
        program, schedule,
      }) + "\n");
    ' "$PERF" "$CASE_NAME" "$CASE_FAMILY" "$ENGINE" "${VERDICT:-error}" "$INPUT_HASH" \
      "$WALLS" "$PEAK_RSS_KB" "$CASE_NOTE" "$EXTERNAL_TIMER" "$RUNS" \
      "$CASE_PROGRAM" "$CASE_SCHEDULE" >> "$RECORDS"

    printf '   %-8s %-11s' "$ENGINE" "${VERDICT:-error}"
    [ -n "$WALLS" ] && printf ' wall(median)=%s' "$(node -e '
      const w = process.argv[1].trim().split(/\s+/).map(Number).filter(Number.isFinite).sort((a,b)=>a-b);
      process.stdout.write(w.length ? String(w[Math.floor((w.length-1)/2)]) : "-");' "$WALLS")"
    printf '\n'
    unset VERDICT
  done
done

echo ""
node --experimental-transform-types report.ts "$RECORDS"
