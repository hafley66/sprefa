#!/usr/bin/env bash
# tsv2 generated-program runner for bench/run.sh.
# Arguments use the house layers x width convention: layers selects s1/s2/s3,
# width is the ROWS value. The worker measures TickFold; this wrapper performs
# generation, compile_fixture/4, oracle gating, warmup, timeout, and RSS merge.
#
# THE TIMEOUT (timeout-gun lane, 2026-07-31): both 600s caps below used to be
# `perl -e 'alarm 600; exec @ARGV'`, which is the ORPHANING form -- `exec`
# replaces perl with the command, so SIGALRM kills that one process and every
# child it spawned survives, reparented, still burning a core beside the next
# cell being measured. bench-cli's header carries the receipt that caught it
# (an orphaned swipl running 3m past its cap while the next cell was timed).
# Both now go through v6/tools/run-capped.sh's `run_capped`: fork + setpgrp +
# SIGALRM -> `kill -KILL -pgid`, exit 124. Ledger row perl_alarm_orphan.
#
# The exit code changes with it, and the DNF reason below follows: SIGALRM
# reached the worker as signal 14 (status 142), where a group kill reports the
# coreutils convention 124.
set -uo pipefail

sdir="$(cd "$(dirname "$0")" && pwd)"
bdir="$(cd "$sdir/.." && pwd)"
root="$(cd "$bdir/../../.." && pwd)"

. "$root/v6/tools/run-capped.sh"
worker_budget_s="${TSV2_GEN_BUDGET_S:-600}"
out="$bdir/out"
shape="s$1"
rows="$2"
record="$out/tsv2-results.jsonl"
term="$out/tsv2-${shape}-${rows}.pl"
generated="$root/v6/tsv2/gen/scale_generated.ts"
oracle="$out/tsv2-${shape}-${rows}.oracle.log"
tslog="$out/tsv2-${shape}-${rows}.tsv2.log"
tsv2_heap_mb="${TSV2_HEAP_MB:-512}"

mkdir -p "$out"
if [[ "$shape" != s1 && "$shape" != s2 && "$shape" != s3 ]]; then
  echo "TSV2_FATAL invalid shape: $shape" >&2
  exit 1
fi

if ! /opt/homebrew/bin/swipl -q -s "$bdir/scale-gen.pl" \
    -g "scale_gen:write_fixture($shape,$rows,'$term'),halt" >/dev/null 2>"$out/tsv2-${shape}-${rows}.gen.err"; then
  echo "TSV2_FATAL generator failed for $shape/$rows" >&2
  exit 1
fi

if ! /opt/homebrew/bin/swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "compile_fixture(scale_bench,'$term','$generated',emit_ts:emit_program),halt" \
    >/dev/null 2>"$out/tsv2-${shape}-${rows}.compile.err"; then
  echo "TSV2_FATAL compile_fixture/4 failed for $shape/$rows" >&2
  exit 1
fi

run_worker() {
  local mode="$1"
  local log_path="${2:-/dev/null}"
  local record_path="$record"
  if [[ "$mode" != measured ]]; then record_path="/dev/null"; fi
  local stdout_path="$out/tsv2-${shape}-${rows}.${mode}.out"
  local stderr_path="$out/tsv2-${shape}-${rows}.${mode}.err"
  run_capped "$worker_budget_s" /usr/bin/time -l \
    node --max-old-space-size="$tsv2_heap_mb" --experimental-transform-types \
    "$root/v6/tsv2/scripts/scale-bench.ts" \
    "$shape" "$rows" "$record_path" "$log_path" >"$stdout_path" 2>"$stderr_path"
  local status=$?
  local line
  line="$(grep '^CSV,' "$stdout_path" | head -1)"
  if [[ -n "$line" ]]; then
    echo "$line"
    return 0
  fi
  if [[ "$status" -ne 0 ]]; then
    # 124 is run_capped's budget-exceeded exit (the coreutils convention). 142
    # was the old orphaning form's SIGALRM-to-the-worker code and is kept so an
    # older record file still reads as a timeout rather than as a mystery.
    if [[ "$status" -eq 124 || "$status" -eq 142 ]]; then
      reason="$mode timeout after $worker_budget_s seconds"
    else
      reason="$mode worker exit status $status"
    fi
    printf '{"engine":"tsv2-gen","shape":"%s","rows":%s,"status":"DNF","observed_failure":"%s"}\n' \
      "$shape" "$rows" "$reason" >> "$record"
    echo "TSV2_DNF $shape/$rows $reason" >&2
    sed -n '1,12p' "$stderr_path" >&2
    return 2
  fi
  echo "TSV2_FATAL worker produced no CSV for $shape/$rows ($mode)" >&2
  return 1
}

if [[ "$shape" == s1 && "$rows" -eq 1000 ]]; then
  if ! run_capped "$worker_budget_s" /opt/homebrew/bin/swipl -q \
      -l "$root/v6/prolog/conformance/ticklog.pl" -l "$term" \
      -g 'emit(scale_bench)' -g halt >"$oracle" 2>"$out/tsv2-s1-1000.oracle.err"; then
    echo "TSV2_FATAL oracle execution failed for s1/1000" >&2
    exit 1
  fi
  if ! run_worker oracle "$tslog" >/dev/null; then
    echo "TSV2_FATAL tsv2 oracle-cell execution failed for s1/1000" >&2
    exit 1
  fi
  if ! cmp -s "$oracle" "$tslog"; then
    echo "TSV2_ORACLE_DIFF s1/1000" >&2
    diff -u "$oracle" "$tslog" | sed -n '1,80p' >&2 || true
    exit 1
  fi
  echo "TSV2_ORACLE_OK s1/1000 byte-identical" >&2
fi

warmup_line="$(run_worker warmup)"
warmup_status=$?
if [[ "$warmup_status" -eq 1 ]]; then exit 1; fi
if [[ "$warmup_status" -eq 2 ]]; then
  # The cell itself hit the DNF limit during the discarded warmup.
  exit 0
fi

measured_line="$(run_worker measured)"
measured_status=$?
if [[ "$measured_status" -eq 1 ]]; then exit 1; fi
if [[ "$measured_status" -eq 2 ]]; then exit 0; fi

rss="$(awk '/maximum resident set size/ {print $1}' "$out/tsv2-${shape}-${rows}.measured.err")"
if [[ -n "$rss" ]]; then
  rss_mb="$(awk -v b="$rss" 'BEGIN{printf "%.1f", b/1048576}')"
else
  rss_mb="$(sed -n 's/.*"worker_rss_mb":\([0-9.]*\).*/\1/p' "$record" | tail -1)"
fi
host_peak_mb="$(sed -n 's/.*"host_peak_mb":\([0-9.]*\).*/\1/p' "$record" | tail -1)"
echo "${measured_line},0,${rss_mb},${host_peak_mb:-N/A},N/A,N/A" >&2
