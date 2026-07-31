#!/usr/bin/env bash
# bench.sh — the language-agnostic CLI bench harness (CONTRACT.md).
# Pass 1 uses swipl for reachable cases. Pass 2 uses tsv2 only when the
# corpus proof and this run's reference checks both authorize promotion.
# Each graded pair is written to out/records.jsonl, then report.ts renders the
# standings. Wrong, refused, and error cells have no timing.
#
# ── SABOTAGE RECEIPTS for the promotion rule (all three reverted) ───────────
# (a) BREADTH. `run-results.json` entry 5 (keyed_edge_head_still_replaces)
#     rewritten `bucket: "identical"` -> `"wrong"`, artifacts restored after:
#         sweep proof   false [a4a076fc980d8f1a] 1 fixture(s) WRONG vs the
#                       oracle; identical(189) + rejection(1) does not account
#                       for all 191 run records
#         == reference tier: false ==  REFUSED (breadth)
#         keyed_replace/10k  tsv2  no_reference  none
#         BENCH-CLI GATE: 1 cell(s) are ungraded because the sweep proof is
#                         INVALID ... EXIT=1
#     Same result with the artifact simply MISSING (renamed away):
#     `sweep artifacts missing or unreadable (ENOENT ...)`, exit 1. A cell is
#     never promoted on a proof that is not there.
# (b) THE REFERENCE LOG IS REALLY DIFFED. One line of the pass-2 reference log
#     mutated between writing it and grading against it
#     (`sed -i '' '3s/"tick":3/"tick":33/' "$REF_LOG"`):
#         keyed_replace/10k  tsv2  wrong  tsv2(proven)
#         BENCH-CLI GATE: tsv2 on keyed_replace/10k did not reproduce its referee
#                         (tsv2(proven)): tick log differs ... EXIT=1
#     and the wall column went blank -- a cell that fails its referee is not
#     timed, whichever referee it was.
# (c) THE THIRD CHECK GATES ON ITS OWN. Same seam, tick log left alone, only
#     the reference FINAL-STATE line mutated (`{"final"` -> `{"FINAL"`):
#         BENCH-CLI GATE: ... final-state hash 1f19d33822b870fd differs from
#                         the referee's 3880188ea7d2c956 (tick logs agreed)
#     EXIT=1. The final-state column is a check, not a decoration.
#
# Env:
#   BENCH_RUNS=5          repeats per timed engine (median reported)
#   BENCH_CASES=a,b       restrict to these case names
#   BENCH_TIMEOUT=60      per-run seconds for a TIMED engine
#   BENCH_ORACLE_BUDGET=30  seconds the swipl reference engine gets before a
#                         case is deferred to the proven referee
#   BENCH_ORACLE_PROBE=1  parallel doom probe (below); 0 = serial-only pass 1
#   TSV2_HEAP_MB=512      node old-space ceiling for the tsv2 engine
#
# Usage: cd v6 && just bench-cli     (or: bash v6/bench-cli/bench.sh)
#
# The bench-cli node_modules link must resolve to the worktree's tsv2 and store
# installs. Otherwise the measured engine can come from another worktree.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

OUT="$HERE/out"
mkdir -p "$OUT"
RECORDS="$OUT/records.jsonl"
: > "$RECORDS"
# Generated schedules are run scratch, not a cache: a schedule-gen.ts edit must
# not be able to leave a previous shape in place under the same file name.
rm -f "$OUT"/*.schedule.json
# Probe verdicts are THIS run's or nobody's: a stale 124 from a previous run
# would silently skip a live oracle (the staleness class, gen_staleness_gate).
rm -f "$OUT"/*.oracle.probe-status

# ── BENCH-TRACE: the run accounts for its own wall (self-diagnosis law) ─────
# Printed before the final line, always on. Never make the user ask what was
# slow: phase walls + invocation walls + the process-startup tax (invocation
# wall minus engine-internal wall) are the answer, from this run's own clock.
now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time()*1000'; }
TRACE_T0=$(now_ms)
TRACE_PROBE_MS=0
TRACE_ORACLE_MS=0;  TRACE_ORACLE_N=0
TRACE_TSV2_MS=0;    TRACE_TSV2_N=0
TRACE_TSV2_ENGINE_MS=0

RUNS="${BENCH_RUNS:-5}"
# 60s, not the house rig's 600s. Every case that clears the reference engine
# at all clears it in under 3s here; the cases that do not clear it do not
# clear 180s either -- keyed_replace/10k's oracle was measured at 173.58s killed, then
# again still running past 183s. So the cap only decides how long the harness
# waits to learn a fact it learns either way, and 60s keeps `just bench-cli`
# usable as a gate. Raise it if a case that SHOULD pass starts timing out.
TIMEOUT="${BENCH_TIMEOUT:-60}"
# The reference engine's BUDGET is a contract concept now (CONTRACT.md section
# 7): exceeding it is what defers a case to the proven referee. 30s, with the
# measurement that fixes it: the slowest case swipl actually reaches is two_hop_join/1k
# at ~2.2s, and the fastest case it does not reach was still running past 183s.
# Any budget between ~5s and ~180s produces the identical set of deferrals, so
# the number is chosen at the low end of that plateau to keep the gate quick.
ORACLE_BUDGET="${BENCH_ORACLE_BUDGET:-30}"
ONLY="${BENCH_CASES:-}"

# hyperfine is the right tool for the EXTERNAL wall column and the wrong one
# for the primary number (CONTRACT.md section 1, candidate 1). Detected, never
# required; its absence changes one column's source and nothing else.
if command -v hyperfine >/dev/null 2>&1; then
  EXTERNAL_TIMER="hyperfine $(hyperfine --version | awk '{print $2}')"
else
  EXTERNAL_TIMER="repeat-loop (hyperfine not installed)"
fi

# ── the breadth half of the reference-validity proof ────────────────────────
# Read once per run, before any case, and written into out/reference-proof.json
# so the standings can name the exact artifacts they leaned on. proof.ts exits
# 0 either way; `valid` is data.
PROOF_FILE="$OUT/reference-proof.json"
node --experimental-transform-types proof.ts "$PROOF_FILE" > /dev/null 2>&1
read_proof() {
  node -e '
    const fs = require("node:fs");
    try {
      const p = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      process.stdout.write(String(p[process.argv[2]] ?? ""));
    } catch { process.stdout.write(""); }
  ' "$PROOF_FILE" "$1"
}
PROOF_VALID="$(read_proof valid)"
PROOF_REASON="$(read_proof reason)"
PROOF_SHA="$(read_proof sweep_sha)"

echo "== bench-cli =="
echo "   runs/engine      $RUNS"
echo "   external timer   $EXTERNAL_TIMER"
echo "   peak rss         /usr/bin/time -l"
echo "   oracle budget    ${ORACLE_BUDGET}s"
echo "   sweep proof      ${PROOF_VALID:-false} [$PROOF_SHA] $PROOF_REASON"
echo ""

# Timeout that kills the whole PROCESS GROUP, not just the wrapper.
#
# `run_capped` NOW LIVES IN v6/tools/run-capped.sh, hoisted there by the
# timeout-gun lane (2026-07-31) so every receipt script in the repository fires
# the same gun. The behaviour is byte-identical to the copy that used to sit
# here; the receipt that motivated it stays, because it is the reason the
# process-group form exists at all.
#
# The house one-liner (`perl -e "alarm shift; exec @ARGV"`, which used to sit in
# v6/sprefa-store/bench/engines/tsv2_gen.sh; macOS ships no coreutils `timeout`)
# execs the command in the SAME process, so SIGALRM terminates the adapter shell
# -- and orphans the engine it spawned. Measured here, not assumed: at the
# keyed_replace/10k timeout the harness moved on to two_hop_join/1k while the
# timed-out swipl kept running,
#
#     03:03 swipl      <- keyed_replace/10k oracle, orphaned, past its 180s cap
#     00:01 swipl      <- two_hop_join/1k oracle, being measured beside it
#
# so every subsequent cell was timed against a stolen core. That is the same
# contamination class as the two-concurrent-runs defect, arriving by a
# different door. fork + setpgrp + `kill -KILL -$pid` takes the engine down
# with the wrapper. Exit 124 on timeout, the coreutils convention -- and that
# 124 is exactly what the promotion rule reads as "budget exceeded", which is
# why a genuine oracle CRASH (any other non-zero) must NOT promote.
. "$HERE/../tools/run-capped.sh"

engine_cmd() {
  case "$1" in
    oracle) echo "$HERE/adapters/oracle.sh" ;;
    tsv2)   echo "$HERE/adapters/tsv2.sh" ;;
    *) return 1 ;;
  esac
}

# Final-state hash, the contract's THIRD check (CONTRACT.md section 2.7). A
# missing file is N/A, never a zero hash: the whole point of the column is that
# a cell either carries the check or says it does not.
hash_file() {
  if [ -f "$1" ]; then shasum -a 256 "$1" | cut -c1-16; else echo "N/A"; fi
}

# ── per-case state, set by load_case ────────────────────────────────────────
load_case() {
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
  ' "$1")"
  CASE_INDEX="$1"
  SAFE="$(echo "$CASE_NAME" | tr '/' '_')"

  # Stale perf JSON from an earlier run would be read as THIS run's ticks and
  # statement count for a cell that never executed -- the same fabricated-cell
  # class as the `wall_ms 0` bug the record writer's comment describes.
  rm -f "$OUT/$SAFE.tsv2.perf.json" "$OUT/$SAFE.tsv2.perf.json.final.jsonl" \
        "$OUT/$SAFE.oracle.perf.json"

  # Scale cases mint their schedule; program cases already have one on disk.
  # Minted once per bench run (pass 2 reuses pass 1's file, and the run began
  # by clearing them), because generating a 100k schedule twice is a second of
  # nothing and a schedule that differed between passes would break the
  # per-case input-hash identity.
  if [ "$CASE_FAMILY" = "scale" ]; then
    CASE_SCHEDULE="$OUT/$SAFE.schedule.json"
    if [ ! -s "$CASE_SCHEDULE" ]; then
      node --experimental-transform-types schedule-gen.ts "$CASE_SHAPE" "$CASE_ROWS" "$CASE_SCHEDULE" 2>/dev/null
    fi
    # No fixture, so no swept final state to compare against.
    ORACLE_FINAL=""
  else
    ORACLE_FINAL="${CASE_SCHEDULE%.schedule.json}.oracle.final.jsonl"
  fi

  [ -f "$CASE_PROGRAM" ] && [ -f "$CASE_SCHEDULE" ] || return 1

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
  return 0
}

# record_row <engine> <verdict> <referee> <walls> <rss_kb> <perf_file> <final_hash> <detail>
record_row() {
  node -e '
    const fs = require("node:fs");
    const [file, name, family, engine, verdict, referee, hash, wallsText, rssKb, note,
           timer, runs, program, schedule, finalHash, detail, caseIndex] = process.argv.slice(1);
    let perf = {};
    try { perf = JSON.parse(fs.readFileSync(file, "utf8")); } catch { perf = {}; }
    // filter(length) BEFORE Number(): "".split(/\s+/) is [""], and
    // Number("") is 0, which is finite -- so an engine that was never run
    // reported `wall_ms 0` instead of N/A. A fabricated zero in a standings
    // table is precisely what this contract exists to prevent.
    const walls = wallsText.trim().split(/\s+/).filter((t) => t.length > 0)
      .map(Number).filter((n) => Number.isFinite(n)).sort((a, b) => a - b);
    const median = walls.length === 0 ? null : walls[Math.floor((walls.length - 1) / 2)];
    process.stdout.write(JSON.stringify({
      case: name, case_index: Number(caseIndex), family, engine, verdict, referee,
      input_hash: hash, note, detail,
      wall_ms: median, wall_samples: walls,
      compile_ms: perf.compile_ms ?? "N/A",
      ticks: perf.ticks ?? "N/A",
      statements: perf.statements ?? "N/A",
      db_bytes: perf.db_bytes ?? "N/A",
      final_hash: finalHash,
      peak_rss_mb: Number(rssKb) > 0 ? Number((Number(rssKb) / 1024).toFixed(1)) : "N/A",
      engine_notes: perf.notes ?? {},
      external_timer: timer, runs: Number(runs),
      program, schedule,
    }) + "\n");
  ' "$6" "$CASE_NAME" "$CASE_FAMILY" "$1" "$2" "$3" "$INPUT_HASH" \
    "$4" "$5" "$CASE_NOTE" "$EXTERNAL_TIMER" "$RUNS" \
    "$CASE_PROGRAM" "$CASE_SCHEDULE" "$7" "${8:-}" "$CASE_INDEX" >> "$RECORDS"

  printf '   %-8s %-22s %-8s' "$1" "$2" "$3"
  [ -n "$4" ] && printf ' wall(median)=%s' "$(node -e '
    const w = process.argv[1].trim().split(/\s+/).filter((t) => t.length > 0)
      .map(Number).filter(Number.isFinite).sort((a,b)=>a-b);
    process.stdout.write(w.length ? String(w[Math.floor((w.length-1)/2)]) : "-");' "$4")"
  printf '\n'
}

# time_engine <engine> <ref_log> <ref_final_hash> <identical_verdict>
#
# Runs the engine RUNS times, byte-diffing run 1 against <ref_log> and every
# run's final-state hash against <ref_final_hash>. Sets TE_VERDICT, TE_WALLS,
# TE_RSS_KB, TE_FINAL, TE_DETAIL. `identical_verdict` is the vocabulary the
# caller wants for a passing cell: `identical` when swipl refereed it,
# `identical_vs_reference` when the proven engine did.
time_engine() {
  local engine="$1" ref_log="$2" ref_final="$3" pass_verdict="$4"
  local log="$OUT/$SAFE.$engine.log"
  local err="$OUT/$SAFE.$engine.err"
  local perf="$OUT/$SAFE.$engine.perf.json"
  local time_file="$OUT/$SAFE.$engine.time"
  local run status rss_kb
  TE_VERDICT=""; TE_WALLS=""; TE_RSS_KB=0; TE_FINAL="N/A"; TE_DETAIL=""

  for run in $(seq 1 "$RUNS"); do
    # A stale final-state file from the previous run would be hashed as this
    # run's answer if the engine died before writing one.
    rm -f "$perf.final.jsonl"
    local tsv2_env=""
    [ "$engine" = "tsv2" ] && tsv2_env="NODE_OPTIONS=--max-old-space-size=${TSV2_HEAP_MB:-512}"
    local trace_a trace_b
    trace_a=$(now_ms)
    run_capped "$TIMEOUT" env $tsv2_env /usr/bin/time -l \
      "$(engine_cmd "$engine")" \
        --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
        --db ":memory:" --perf-out "$perf" \
      > "$log" 2> "$time_file"
    status=$?
    trace_b=$(now_ms)
    TRACE_TSV2_MS=$((TRACE_TSV2_MS + trace_b - trace_a))
    TRACE_TSV2_N=$((TRACE_TSV2_N + 1))
    grep -v 'maximum resident set size\|average shared\|average unshared\|page reclaims\|page faults\|swaps\|block input\|block output\|messages sent\|messages received\|signals received\|context switches\|instructions retired\|cycles elapsed\|peak memory footprint\|real  *[0-9]' "$time_file" > "$err" 2>/dev/null

    rss_kb=$(awk '/maximum resident set size/{print int($1/1024)}' "$time_file" | head -1)
    [ -n "$rss_kb" ] && [ "$rss_kb" -gt "$TE_RSS_KB" ] && TE_RSS_KB="$rss_kb"

    local this_final
    this_final="$(hash_file "$perf.final.jsonl")"
    [ "$run" -eq 1 ] && TE_FINAL="$this_final"

    if [ "$run" -eq 1 ]; then
      if [ "$status" -eq 2 ]; then
        TE_VERDICT="refused"
      elif [ "$status" -ne 0 ]; then
        TE_VERDICT="error"
        TE_DETAIL="exit $status"
      elif ! cmp -s "$ref_log" "$log"; then
        TE_VERDICT="wrong"
        TE_DETAIL="tick log differs from the referee's"
      else
        TE_VERDICT="$pass_verdict"
      fi
    fi
    # The third check gates, it does not decorate: a cell whose tick log
    # matches and whose FINAL STATE does not is `wrong`, and so is a cell
    # whose own repeats disagree with each other.
    if [ "$TE_VERDICT" = "$pass_verdict" ] && [ "$ref_final" != "N/A" ] && [ "$this_final" != "N/A" ] \
       && [ "$this_final" != "$ref_final" ]; then
      TE_VERDICT="wrong"
      TE_DETAIL="final-state hash $this_final differs from the referee's $ref_final (tick logs agreed)"
    fi

    # Only a matching engine is timed. Break out otherwise: repeating a
    # disqualified run buys nothing and the number would never be printed.
    [ "$TE_VERDICT" = "$pass_verdict" ] || break

    local wall
    wall=$(node -e '
      try { const p = require("node:fs").readFileSync(process.argv[1], "utf8");
            process.stdout.write(String(JSON.parse(p).wall_ms)); }
      catch { process.stdout.write("null"); }
    ' "$perf" 2>/dev/null)
    TE_WALLS="$TE_WALLS $wall"
    case "$wall" in
      ''|null) : ;;
      *) TRACE_TSV2_ENGINE_MS=$(perl -e 'printf "%d", $ARGV[0] + $ARGV[1]' "$TRACE_TSV2_ENGINE_MS" "$wall") ;;
    esac
  done
}

# ── PASS 1: every case the swipl reference engine reaches ───────────────────
case_count=$(node -e 'process.stdout.write(String(require("./cases.json").length))')
DEFERRED=""
LIVE_IDENTICAL=0
LIVE_FAILED=0

# ── pass 1a: the oracle DOOM PROBE (parallel) ───────────────────────────────
# Receipts (2026-07-31, coordinator): full run 4m48s -> 2m40s, verdict parity
# zero mismatches on (case, engine, verdict, referee) across all 32 rows,
# hash-agreement OK. Sabotage: a PLANTED stale probe-status=124 for
# callgraph_derivation was wiped by the run-start rm and the cell still
# graded `identical` vs swipl (swipl 1 / reference 0) -- the staleness rail
# holds. Note: a BENCH_CASES-filtered run rewrites standings.csv wholesale
# (1-cell file); do not commit standings from a filtered run.
# Over half of a full serial run was 5 cells x the full 30s budget, each
# re-proving that swipl cannot finish -- serially, every run. The probe runs
# EVERY oracle attempt concurrently and records ONLY the exit status. No
# number from this phase is ever reported: cells the probe clears are re-run
# serially and alone in pass 1b (clean wall, clean RSS, authoritative status),
# and no timed tsv2 run starts until run_capped has killed every prober
# (process-group KILL, same guarantee as the serial path). Concurrency note:
# on a machine with fewer cores than grinding cells, a borderline cell could
# time out under contention that would finish alone; the consequence is that
# it defers to the PROVEN referee instead of swipl -- sound, just the less
# preferred referee -- and pass 1b's serial run remains the authority for
# every cell the probe clears.
if [ "${BENCH_ORACLE_PROBE:-1}" = "1" ]; then
  echo "== pass 1a: oracle doom probe (parallel, budget ${ORACLE_BUDGET}s)"
  TRACE_PROBE_A=$(now_ms)
  PROBE_PIDS=""
  for index in $(seq 0 $((case_count - 1))); do
    load_case "$index" || continue
    if [ -n "$ONLY" ] && [[ ",$ONLY," != *",$CASE_NAME,"* ]]; then continue; fi
    (
      run_capped "$ORACLE_BUDGET" "$(engine_cmd oracle)" \
          --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
          --db ":memory:" --perf-out "$OUT/$SAFE.probe.perf.json" \
          > /dev/null 2>&1
      echo $? > "$OUT/$SAFE.oracle.probe-status"
    ) &
    PROBE_PIDS="$PROBE_PIDS $!"
  done
  wait $PROBE_PIDS 2>/dev/null
  rm -f "$OUT"/*.probe.perf.json "$OUT"/*.probe.perf.json.final.jsonl
  TRACE_PROBE_MS=$(( $(now_ms) - TRACE_PROBE_A ))
fi

for index in $(seq 0 $((case_count - 1))); do
  if ! load_case "$index"; then
    if [ -n "$ONLY" ] && [[ ",$ONLY," != *",$CASE_NAME,"* ]]; then continue; fi
    echo "SKIP $CASE_NAME (missing program or schedule)"
    continue
  fi
  if [ -n "$ONLY" ] && [[ ",$ONLY," != *",$CASE_NAME,"* ]]; then continue; fi

  echo "-- $CASE_NAME  [$INPUT_HASH]"

  REF_LOG="$OUT/$SAFE.oracle.log"
  ORACLE_PERF="$OUT/$SAFE.oracle.perf.json"
  ORACLE_TIME_FILE="$OUT/$SAFE.oracle.time"
  PROBE_STATUS="$(cat "$OUT/$SAFE.oracle.probe-status" 2>/dev/null || echo "")"
  if [ "$PROBE_STATUS" = "124" ]; then
    # The probe already burned this cell's budget; do not re-prove the
    # timeout serially. 124 is the ONE status taken from the probe: any
    # completion or crash is re-established by the serial run below.
    ORACLE_STATUS=124
    : > "$REF_LOG"; : > "$ORACLE_TIME_FILE"
  else
    TRACE_A=$(now_ms)
    run_capped "$ORACLE_BUDGET" /usr/bin/time -l "$(engine_cmd oracle)" \
        --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
        --db ":memory:" --perf-out "$ORACLE_PERF" \
        > "$REF_LOG" 2> "$ORACLE_TIME_FILE"
    ORACLE_STATUS=$?
    TRACE_ORACLE_MS=$(( TRACE_ORACLE_MS + $(now_ms) - TRACE_A ))
    TRACE_ORACLE_N=$((TRACE_ORACLE_N + 1))
  fi
  grep -v 'maximum resident set size\|average shared\|average unshared\|page reclaims\|page faults\|swaps\|block input\|block output\|messages sent\|messages received\|signals received\|context switches\|instructions retired\|cycles elapsed\|peak memory footprint\|real  *[0-9]' "$ORACLE_TIME_FILE" > "$OUT/$SAFE.oracle.err" 2>/dev/null
  ORACLE_RSS_KB=$(awk '/maximum resident set size/{print int($1/1024)}' "$ORACLE_TIME_FILE" | head -1)
  ORACLE_RSS_KB="${ORACLE_RSS_KB:-0}"

  if [ "$ORACLE_STATUS" -eq 0 ]; then
    # The oracle is the REFERENCE and is sampled once, not RUNS times. Two
    # reasons, both stated so this does not read as a corner cut: (a) its
    # wall_ms is wrapper-measured and carries swipl's startup floor, so
    # CONTRACT.md section 6 already rules it not comparable head to head with
    # tsv2's -- a tighter median buys a number nothing may be ranked on;
    # (b) the reference leg above has already run it once for the log.
    ORACLE_FINAL_HASH="$(hash_file "$ORACLE_FINAL")"
    ORACLE_WALL=$(node -e '
      try { const p = require("node:fs").readFileSync(process.argv[1], "utf8");
            process.stdout.write(String(JSON.parse(p).wall_ms)); }
      catch { process.stdout.write(""); }
    ' "$ORACLE_PERF" 2>/dev/null)
    record_row oracle reference oracle " $ORACLE_WALL" "$ORACLE_RSS_KB" "$ORACLE_PERF" "$ORACLE_FINAL_HASH" ""

    time_engine tsv2 "$REF_LOG" "$ORACLE_FINAL_HASH" "identical"
    record_row tsv2 "$TE_VERDICT" oracle "$TE_WALLS" "$TE_RSS_KB" "$OUT/$SAFE.tsv2.perf.json" "$TE_FINAL" "$TE_DETAIL"
    if [ "$TE_VERDICT" = "identical" ]; then
      LIVE_IDENTICAL=$((LIVE_IDENTICAL + 1))
    else
      LIVE_FAILED=$((LIVE_FAILED + 1))
    fi
  elif [ "$ORACLE_STATUS" -eq 124 ]; then
    # Budget exceeded, not a failure: this is the oracle_scale_ceiling, and it
    # is what the proven referee exists for. The tsv2 row is written in pass 2
    # once the proof has been read.
    record_row oracle over_budget none "" 0 "$ORACLE_PERF" "N/A" \
      "swipl exceeded the ${ORACLE_BUDGET}s reference budget on this case"
    DEFERRED="$DEFERRED $index"
  else
    # Any OTHER non-zero is not a scale ceiling and must not promote: an
    # unexplained reference failure is exactly when grading past the reference
    # is least defensible.
    record_row oracle error none "" 0 "$ORACLE_PERF" "N/A" "swipl exited $ORACLE_STATUS (not a budget timeout)"
    record_row tsv2 no_reference none "" 0 "$OUT/$SAFE.tsv2.perf.json" "N/A" \
      "the reference engine failed for a reason that is not its scale budget; nothing is graded here"
  fi
done

# ── the promotion decision, made once, from both halves of the proof ────────
TIER_OK="false"
TIER_REASON=""
if [ -z "$DEFERRED" ]; then
  TIER_REASON="no case exceeded the reference budget; the proven referee was not needed"
elif [ "$PROOF_VALID" != "true" ]; then
  TIER_REASON="REFUSED (breadth): $PROOF_REASON"
elif [ "$LIVE_IDENTICAL" -eq 0 ]; then
  TIER_REASON="REFUSED (currency): this run graded no case against swipl, so the referee is unproven HERE"
elif [ "$LIVE_FAILED" -ne 0 ]; then
  TIER_REASON="REFUSED (currency): tsv2 failed $LIVE_FAILED of the $((LIVE_IDENTICAL + LIVE_FAILED)) cases swipl refereed in this run"
else
  TIER_OK="true"
  TIER_REASON="tsv2 is the proven referee: sweep $PROOF_SHA (corpus-wide) + $LIVE_IDENTICAL/$LIVE_IDENTICAL identical vs swipl in this run"
fi

if [ -n "$DEFERRED" ]; then
  echo ""
  echo "== reference tier: $TIER_OK =="
  echo "   $TIER_REASON"
  echo ""
fi

# ── PASS 2: the deferred cases, graded against the proven referee ───────────
for index in $DEFERRED; do
  load_case "$index" || continue
  echo "-- $CASE_NAME  [$INPUT_HASH]  (reference tier)"

  if [ "$TIER_OK" != "true" ]; then
    record_row tsv2 no_reference none "" 0 "$OUT/$SAFE.tsv2.perf.json" "N/A" "$TIER_REASON"
    continue
  fi

  # The reference log is a SEPARATE invocation from the timed ones. That is
  # what makes the tier gradeable at all today: with one engine in the field,
  # diffing an engine against its own single run would be vacuous, while
  # diffing five runs against a sixth is a real repeatability check -- and it
  # is the exact seam a rust engine plugs into, unchanged.
  REF_LOG="$OUT/$SAFE.reference.log"
  REF_PERF="$OUT/$SAFE.reference.perf.json"
  rm -f "$REF_PERF.final.jsonl"
  TRACE_A=$(now_ms)
  run_capped "$TIMEOUT" env "NODE_OPTIONS=--max-old-space-size=${TSV2_HEAP_MB:-512}" \
    "$(engine_cmd tsv2)" \
      --program "$CASE_PROGRAM" --schedule "$CASE_SCHEDULE" \
      --db ":memory:" --perf-out "$REF_PERF" \
    > "$REF_LOG" 2> "$OUT/$SAFE.reference.err"
  REF_STATUS=$?
  TRACE_TSV2_MS=$(( TRACE_TSV2_MS + $(now_ms) - TRACE_A ))
  TRACE_TSV2_N=$((TRACE_TSV2_N + 1))

  if [ "$REF_STATUS" -ne 0 ]; then
    record_row tsv2 no_reference none "" 0 "$OUT/$SAFE.tsv2.perf.json" "N/A" \
      "the proven referee itself exited $REF_STATUS on this case; there is no reference log to grade against"
    continue
  fi

  REF_FINAL_HASH="$(hash_file "$REF_PERF.final.jsonl")"
  time_engine tsv2 "$REF_LOG" "$REF_FINAL_HASH" "identical_vs_reference"
  record_row tsv2 "$TE_VERDICT" "tsv2(proven)" "$TE_WALLS" "$TE_RSS_KB" \
    "$OUT/$SAFE.tsv2.perf.json" "$TE_FINAL" "$TE_DETAIL"
done

echo ""
TRACE_TOTAL=$(( $(now_ms) - TRACE_T0 ))
TRACE_OTHER=$(( TRACE_TOTAL - TRACE_PROBE_MS - TRACE_ORACLE_MS - TRACE_TSV2_MS ))
TRACE_STARTUP=$(( TRACE_TSV2_MS - TRACE_TSV2_ENGINE_MS ))
echo "BENCH-TRACE wall ${TRACE_TOTAL}ms ="
echo "  doom probe (parallel swipl timeouts)  ${TRACE_PROBE_MS}ms"
echo "  swipl answer keys, serial x${TRACE_ORACLE_N}          ${TRACE_ORACLE_MS}ms"
echo "  tsv2 invocations x${TRACE_TSV2_N}                   ${TRACE_TSV2_MS}ms"
echo "    of which engine-internal            ${TRACE_TSV2_ENGINE_MS}ms"
echo "    of which process startup+wrapper    ${TRACE_STARTUP}ms  <- node boot + type transform, per invocation"
echo "  everything else (compile, schedules, proof, hashing)  ${TRACE_OTHER}ms"
node --experimental-transform-types report.ts "$RECORDS" "$PROOF_FILE"
REPORT_EXIT=$?
[ "$REPORT_EXIT" -eq 0 ] || exit "$REPORT_EXIT"
# Floor gate: a run where NOTHING got timed is a broken rig, never a pass
# (measured 2026-07-31: a missing node_modules symlink error'd all 14 cells
# and the recipe still exited 0). It reads the csv THIS run wrote -- a filtered
# run writes into out/, and grepping the committed standings.csv instead would
# have let a partial run pass on a previous run's numbers.
GATE_CSV="$HERE/standings.csv"
[ -n "$ONLY" ] && GATE_CSV="$OUT/standings.csv"
grep -q ',tsv2,' "$GATE_CSV" && grep -qE ',identical(_vs_reference)?,' "$GATE_CSV" \
  || { echo "BENCH-CLI FLOOR: zero timed cells -- rig is broken, not slow"; exit 1; }
