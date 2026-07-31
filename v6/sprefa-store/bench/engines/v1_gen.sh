#!/usr/bin/env bash
# v1 generated-program runner for bench/run.sh.
# s2 and s3 are emitted by the existing literal-TS Prolog emitter and run via
# evalProgramSql. s1 has no keyed-replace edge equivalent in the v1 AST.
#
# THE TIMEOUT (timeout-gun lane, 2026-07-31): the 600s cap below used to be
# `perl -e 'alarm 600; exec @ARGV'`, the ORPHANING form -- `exec` replaces perl
# with the command, so SIGALRM kills that one process and every child it
# spawned survives to steal a core from the next cell being measured
# (bench-cli's header carries the receipt). It now goes through
# v6/tools/run-capped.sh's `run_capped`: fork + setpgrp + `kill -KILL -pgid`,
# exit 124. Ledger row perl_alarm_orphan.
set -uo pipefail

sdir="$(cd "$(dirname "$0")" && pwd)"
bdir="$(cd "$sdir/.." && pwd)"
root="$(cd "$bdir/../../.." && pwd)"

. "$root/v6/tools/run-capped.sh"
worker_budget_s="${V1_GEN_BUDGET_S:-600}"
out="$bdir/out"
shape="s$1"
rows="$2"
record="$out/v1-results.jsonl"
generated="$root/v6/sprefa-store/js/src/gen/v1_scale_generated.ts"
v1_heap_mb="${V1_HEAP_MB:-512}"

mkdir -p "$out"
if [[ "$shape" != s1 && "$shape" != s2 && "$shape" != s3 ]]; then
  echo "V1_FATAL invalid shape: $shape" >&2
  exit 1
fi

if [[ "$shape" == s1 ]]; then
  printf '{"engine":"v1-gen","shape":"%s","rows":%s,"status":"N/A","reason":"v1 AST/evalProgramSql has no keyed-replace edge semantics"}\n' \
    "$shape" "$rows" >> "$record"
  echo "V1_NA $shape/$rows v1 AST/evalProgramSql has no keyed-replace edge semantics" >&2
  exit 0
fi

if ! /opt/homebrew/bin/swipl -q -s "$bdir/v1-scale-gen.pl" \
    -g "v1_scale_gen:write_program($shape,'$generated'),halt" \
    >/dev/null 2>"$out/v1-${shape}-${rows}.gen.err"; then
  echo "V1_FATAL generator failed for $shape/$rows" >&2
  exit 1
fi

run_worker() {
  local mode="$1"
  local record_path="$record"
  if [[ "$mode" != measured ]]; then record_path="/dev/null"; fi
  local stdout_path="$out/v1-${shape}-${rows}.${mode}.out"
  local stderr_path="$out/v1-${shape}-${rows}.${mode}.err"
  run_capped "$worker_budget_s" /usr/bin/time -l \
    node --max-old-space-size="$v1_heap_mb" --experimental-transform-types \
    "$root/v6/sprefa-store/js/src/bench/v1_scale_bench.ts" \
    "$shape" "$rows" "$record_path" >"$stdout_path" 2>"$stderr_path"
  local status=$?
  local line
  line="$(grep '^CSV,' "$stdout_path" | head -1)"
  if [[ -n "$line" ]]; then
    echo "$line"
    return 0
  fi
  if [[ "$status" -ne 0 ]]; then
    local reason
    # 124 is run_capped's budget-exceeded exit (the coreutils convention); 142
    # was the old orphaning form's SIGALRM code, kept so older record files
    # still read as timeouts.
    if [[ "$status" -eq 124 || "$status" -eq 142 ]]; then
      reason="$mode timeout after $worker_budget_s seconds"
    else
      reason="$mode worker exit status $status"
    fi
    printf '{"engine":"v1-gen","shape":"%s","rows":%s,"status":"DNF","observed_failure":"%s"}\n' \
      "$shape" "$rows" "$reason" >> "$record"
    echo "V1_DNF $shape/$rows $reason" >&2
    sed -n '1,12p' "$stderr_path" >&2
    return 2
  fi
  echo "V1_FATAL worker produced no CSV for $shape/$rows ($mode)" >&2
  return 1
}

warmup_line="$(run_worker warmup)"
warmup_status=$?
if [[ "$warmup_status" -eq 1 ]]; then exit 1; fi
if [[ "$warmup_status" -eq 2 ]]; then exit 0; fi

measured_line="$(run_worker measured)"
measured_status=$?
if [[ "$measured_status" -eq 1 ]]; then exit 1; fi
if [[ "$measured_status" -eq 2 ]]; then exit 0; fi

rss="$(awk '/maximum resident set size/ {print $1}' "$out/v1-${shape}-${rows}.measured.err")"
if [[ -n "$rss" ]]; then
  rss_mb="$(awk -v b="$rss" 'BEGIN{printf "%.1f", b/1048576}')"
else
  rss_mb="$(sed -n 's/.*"worker_rss_mb":\([0-9.]*\).*/\1/p' "$record" | tail -1)"
fi
host_peak_mb="$(sed -n 's/.*"host_peak_mb":\([0-9.]*\).*/\1/p' "$record" | tail -1)"
echo "${measured_line},0,${rss_mb},${host_peak_mb:-N/A},N/A,N/A" >&2
