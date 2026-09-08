#!/usr/bin/env bash
# Feasibility-lab harness for the Z-set/IVM head-to-head. Runs every engine over
# a scale sweep, collects the `CSV,...` line each example prints on stderr, then
# renders charts (gnuplot) and an auto-written REPORT.md. No plotting library,
# no bespoke chart code: the engines emit CSV, gnuplot draws, awk writes prose.
#
# Usage:  bench/run.sh            # default sweep
#         SCALES="4x500 8x20000" bench/run.sh
#         CAP=4096 bench/run.sh   # requested memory budget (scope varies by arm)
#         POSTGRES_SHOOTOUT=1 BENCH_OUT=bench/results/postgres-shared bench/run.sh
set -uo pipefail
cd "$(dirname "$0")/.."

if [[ -n "${BENCH_CELL_BUDGET_S:-}" ]]; then
  . ../tools/run-capped.sh
fi

OUT="${BENCH_OUT:-bench/out}"
if [[ -e "$OUT/results.csv" || -e "$OUT/adapter-status.tsv" ]]; then
  echo "REFUSE existing receipts in $OUT; choose a new BENCH_OUT" >&2
  exit 2
fi
if [[ "${BENCH_REPEATS:-0}" -gt 0 ]]; then
  if [[ -e "$OUT/repeat-runs.tsv" ]]; then
    echo "REFUSE existing repeat receipts in $OUT" >&2; exit 2
  fi
  mkdir -p "$OUT"
  printf 'phase\titeration\trotation\texit_status\n' > "$OUT/repeat-runs.tsv"
  repeat_failed=0
  for ((iteration=0; iteration<=BENCH_REPEATS; iteration++)); do
    if [[ "$iteration" -eq 0 ]]; then phase=warmup; else phase=measured; fi
    rotation=$((iteration * 3))
    BENCH_REPEATS=0 BENCH_ROTATION="$rotation" BENCH_OUT="$OUT/$phase-$iteration" \
      bash bench/run.sh >"$OUT/$phase-$iteration.log" 2>&1
    repeat_status=$?
    printf '%s\t%s\t%s\t%s\n' "$phase" "$iteration" "$rotation" "$repeat_status" >> "$OUT/repeat-runs.tsv"
    echo "REPEAT $phase $iteration rotation=$rotation exit=$repeat_status"
    [[ "$repeat_status" -eq 0 ]] || repeat_failed=1
    [[ "$repeat_status" -eq 3 ]] && break
  done
  node bench/7_repeat_summary.mjs "$OUT" || repeat_failed=1
  bench/chart.sh "$OUT/results.csv" "$OUT" || repeat_failed=1
  exit "$repeat_failed"
fi
mkdir -p "$OUT"
mkdir -p "$OUT/logs"
CSV="$OUT/results.csv"
STATUS_TSV="$OUT/adapter-status.tsv"
: > "$OUT/tsv2-results.jsonl"
: > "$OUT/v1-results.jsonl"
CAP="${CAP:-4096}"
failed=0
# Engines: label|binary|extra-env.
ENGINES=(
  "swi-incr|bench/engines/swi_incr.sh|"
  "swipl-pure|bench/engines/swipl_pure.sh|"
  "swi-sqlite|bench/engines/swi_sqlite.sh|"
  "swi-ts|bench/engines/swi_ts.sh|"
  "swi-emit|bench/engines/swi_emit.sh|"
  "tsv2-gen|bench/engines/tsv2_gen.sh|"
  "v1-gen|bench/engines/v1_gen.sh|"
)
if [[ "${DD_SHOOTOUT:-0}" == "1" ]]; then
  ENGINES+=("differential-dataflow|dd_reach|")
fi
if [[ "${SQLITE_SHOOTOUT:-0}" == "1" ]]; then
  for cascade in sqlite-count sqlite-count-scc sqlite-dred-loop sqlite-dred-cte sqlite-signed-delta-v2; do
    ENGINES+=("$cascade|bench/engines/4_sqlite_cascade.sh|SQLITE_CASCADE=$cascade")
  done
  ENGINES+=(
    "tsv2-runtime|bench/engines/5_generated_reach.sh|GENERATED_REACH_RUNTIME=tsv2"
    "sprefa-engine-rs|bench/engines/5_generated_reach.sh|GENERATED_REACH_RUNTIME=rust"
  )
fi
if [[ "${POSTGRES_SHOOTOUT:-0}" == "1" ]]; then
  . bench/engines/0_postgres_cluster.sh
  PG_BENCH_LOG_DIR="$OUT/logs/postgres"
  trap pg_bench_stop EXIT
  pg_bench_start bench
  ENGINES+=(
    "pglite-query|bench/engines/2_postgres_query.sh|PG_REACH_RUNTIME=pglite"
    "native-postgres-query|bench/engines/2_postgres_query.sh|PG_REACH_RUNTIME=native"
    "pglite-pg_ivm|bench/engines/3_postgres_ivm_unsupported.sh|PG_REACH_RUNTIME=pglite"
    "native-postgres-pg_ivm|bench/engines/3_postgres_ivm_unsupported.sh|PG_REACH_RUNTIME=native"
  )
fi
if [[ -n "${BENCH_ENGINE_FILTER:-}" ]]; then
  FILTERED_ENGINES=()
  for spec in "${ENGINES[@]}"; do
    IFS='|' read -r label _ _ <<< "$spec"
    if [[ " ${BENCH_ENGINE_FILTER} " == *" $label "* ]]; then
      FILTERED_ENGINES+=("$spec")
    fi
  done
  ENGINES=("${FILTERED_ENGINES[@]}")
fi
if [[ "${#ENGINES[@]}" -gt 0 ]]; then
  rotation=$(( ${BENCH_ROTATION:-0} % ${#ENGINES[@]} ))
  ENGINES=("${ENGINES[@]:rotation}" "${ENGINES[@]:0:rotation}")
fi
printf '%s\n' "${ENGINES[@]}" > "$OUT/engine-order.txt"
# Scale sweep as "layers x width". Kept medium so a laptop survives.
SCALES="${SCALES:-2x200 6x2000 8x20000 10x50000 14x80000}"

echo "engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,host_peak_mb,sqlite_hw_mb,db_mb" > "$CSV"
printf 'engine\tscale\tstatus\tsemantics_or_reason\tmemory_scope\tmemory_limit_scope\n' > "$STATUS_TSV"
printf 'engine\tscale\tactual_sha256\texpected_sha256\n' > "$OUT/input-hashes.tsv"
printf 'epoch_seconds\tengine\tscale\tavailable_percent\n' > "$OUT/resources.tsv"

for spec in "${ENGINES[@]}"; do
  IFS='|' read -r label bin env <<< "$spec"
  # entries with a slash are wrapper scripts (prolog contenders), used as-is
  if [[ "$bin" == */* ]]; then binpath="$bin"; else binpath="target/release/examples/$bin"; fi
  if [[ ! -x "$binpath" ]]; then
    printf '%s\t-\tmissing\t%s is not executable\tunavailable\tunknown\n' \
      "$label" "$binpath" >> "$STATUS_TSV"
    echo "SKIP $label ($binpath not built)"; continue
  fi
  scales="$SCALES"
  if [[ "$label" == "tsv2-gen" ]]; then
    scales="${TSV2_SCALES:-1x1000 1x10000 1x100000 2x1000 2x10000 2x100000 3x1000 3x10000 3x100000}"
  fi
  if [[ "$label" == "v1-gen" ]]; then
    scales="${V1_SCALES:-1x1000 1x10000 1x100000 2x1000 2x10000 2x100000 3x1000 3x10000 3x100000}"
  fi
  for s in $scales; do
    if [[ -n "${BENCH_MIN_FREE_PERCENT:-}" ]]; then
      for resource_sample in 1 2; do
        available=""
        if [[ -x /usr/bin/memory_pressure ]]; then
          available=$(/usr/bin/memory_pressure -Q 2>/dev/null | awk '/System-wide memory free percentage/ {gsub(/%/,"",$NF);print $NF}')
        elif [[ -r /proc/meminfo ]]; then
          available=$(awk '/MemTotal:/ {total=$2} /MemAvailable:/ {available=$2} END {if(total) printf "%.0f",100*available/total}' /proc/meminfo)
        fi
        printf '%s\t%s\t%s\t%s\n' "$(date +%s)" "$label" "$s" "${available:-unknown}" >> "$OUT/resources.tsv"
        if [[ "$available" =~ ^[0-9]+$ ]] && [[ "$available" -lt "$BENCH_MIN_FREE_PERCENT" ]]; then
          if [[ "$resource_sample" -eq 2 ]]; then
            printf '%s\t%s\tresource-blocked\tavailable memory below %s percent on two checks\tOS memory-pressure query\tno cell started\n' \
              "$label" "$s" "$BENCH_MIN_FREE_PERCENT" >> "$STATUS_TSV"
            exit 3
          fi
          sleep 5
        else break; fi
      done
    fi
    if [[ " ${BENCH_SKIP_CELLS:-} " == *" $label@$s "* ]]; then
      printf '%s\t%s\tskipped\t%s\tno process started\tprocess-time bound not entered\n' \
        "$label" "$s" "${BENCH_SKIP_REASON:-explicit BENCH_SKIP_CELLS entry}" >> "$STATUS_TSV"
      echo "SKIP $label $s (${BENCH_SKIP_REASON:-explicit BENCH_SKIP_CELLS entry})"
      continue
    fi
    layers="${s%x*}"; width="${s#*x}"
    cell_log="$OUT/logs/$label-$s.log"
    command_argv=(env)
    if [[ -n "$env" ]]; then command_argv+=("$env"); fi
    command_argv+=(DL_MEMCAP_MB="$CAP" "$binpath" "$layers" "$width")
    if [[ -n "${BENCH_CELL_BUDGET_S:-}" ]]; then
      run_capped "$BENCH_CELL_BUDGET_S" "${command_argv[@]}" >"$cell_log.stdout" 2>"$cell_log"
    else
      "${command_argv[@]}" >"$cell_log.stdout" 2>"$cell_log"
    fi
    status=$?
    printf 'EXIT_STATUS|%s\n' "$status" >> "$cell_log"
    line=$(grep '^CSV,' "$cell_log" | head -1 | cut -d, -f2-)
    status_count=0
    adapter_failed=0
    while IFS='|' read -r marker status_engine adapter_status reason memory_scope limit_scope; do
      [[ "$marker" == STATUS ]] || continue
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$status_engine" "$s" "$adapter_status" "$reason" "$memory_scope" "$limit_scope" \
        >> "$STATUS_TSV"
      status_count=$((status_count + 1))
      case "$adapter_status" in error|oom|timeout) adapter_failed=1 ;; esac
    done < <(grep '^STATUS|' "$cell_log" || true)
    if [[ "$status" -eq 124 || "$status" -eq 142 ]]; then
      printf '%s\t%s\ttimeout\tcase exceeded %s seconds\tunavailable\tprocess-time bound only\n' \
        "$label" "$s" "${BENCH_CELL_BUDGET_S:-unknown}" >> "$STATUS_TSV"
      status_count=$((status_count + 1))
    elif [[ "$status" -ne 0 ]]; then
      printf '%s\t%s\terror\tprocess exited with status %s; see logs/%s-%s.log\tunavailable\tunknown\n' \
        "$label" "$s" "$status" "$label" "$s" >> "$STATUS_TSV"
      status_count=$((status_count + 1))
    fi
    if [[ -n "$line" && "$(awk -F, '{print NF}' <<< "$line")" -eq 8 ]]; then
      line="${line},N/A,N/A,N/A"
    fi
    if [[ -n "$line" && "$status" -eq 0 && "$adapter_failed" -eq 0 && "${BENCH_REQUIRE_INPUT_HASH:-0}" == "1" ]]; then
      actual_hash=$(awk -F'|' '$1=="INPUT" {print $3; exit}' "$cell_log")
      expected_hash=$(node --input-type=module -e 'import {graphFixture} from "./bench/engines/1_postgres_reach.mjs"; console.log(graphFixture(Number(process.argv[1]),Number(process.argv[2]),Number(process.env.BENCH_BACK_STRIDE ?? 0)).input_hash)' "$layers" "$width")
      printf '%s\t%s\t%s\t%s\n' "$label" "$s" "$actual_hash" "$expected_hash" >> "$OUT/input-hashes.tsv"
      if [[ -z "$expected_hash" || "$actual_hash" != "$expected_hash" ]]; then
        printf '%s\t%s\terror\tinput hash mismatch: actual=%s expected=%s\tunavailable\tunknown\n' \
          "$label" "$s" "$actual_hash" "$expected_hash" >> "$STATUS_TSV"
        adapter_failed=1
      fi
    fi
    na_count=$(grep -c '^V1_NA' "$cell_log" || true)
    if [[ "$label" == "tsv2-gen" && "$status" -ne 0 && \
          "$(grep -c '^TSV2_ORACLE_DIFF' "$cell_log")" -gt 0 ]]; then
      cat "$cell_log"
      exit 1
    fi
    cat "$cell_log"
    if [[ "$status" -ne 0 || "$adapter_failed" -eq 1 ]]; then
      failed=1
      echo "FAILED $label $s (see $cell_log and $STATUS_TSV)"
    elif [[ -n "$line" ]]; then
      echo "$line" >> "$CSV"
      echo "OK   $label $s -> $line"
    elif [[ "$label" == "v1-gen" && "$na_count" -gt 0 ]]; then
      echo "N/A  $label $s"
    elif [[ "$status_count" -gt 0 ]]; then
      echo "STATUS $label $s (see $STATUS_TSV)"
    else
      # No numeric result or classified status is an unclassified failure.
      nodes=$(( 2 + layers * width ))
      echo "$label,$nodes,,,,WALL,,,N/A,N/A,N/A" >> "$CSV"
      printf '%s\t%s\terror\tno CSV or adapter status; see logs/%s-%s.log\tunavailable\tunknown\n' \
        "$label" "$s" "$label" "$s" >> "$STATUS_TSV"
      failed=1
      echo "WALL $label $s (hit the ${CAP}MB budget or failed)"
    fi
  done
done

echo "== wrote $CSV =="
bench/chart.sh "$CSV" "$OUT" || failed=1
bench/report.sh "$CSV" "$OUT" "$CAP" "$STATUS_TSV" > "$OUT/REPORT.md" || failed=1
echo "== wrote $OUT/REPORT.md and PNG charts in $OUT/ =="
exit "$failed"
