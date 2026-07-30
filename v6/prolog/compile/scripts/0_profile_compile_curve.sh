#!/usr/bin/env bash
# Reproduce the .dl6 compiler scaling curve with external and per-phase clocks.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$HERE/.." && pwd)"
COMPILE_SH="$HERE/compile_dl6.sh"
REPEATS="${DL_PROFILE_REPEATS:-2}"
SIZES="${DL_PROFILE_SIZES:-20 40 60 80 100 117}"
EXEC_PROFILE_N="${DL_PROFILE_EXEC_N:-60}"
WARM="${DL_PROFILE_WARM:-1}"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/sprefa-prolog-curve.XXXXXX")"
SUMMARY_FILE="$SCRATCH/summary.tsv"

trap 'rm -rf "$SCRATCH"' EXIT

export SPREFA_CONFIG=/nonexistent/x.toml
export DL_NO_DAEMON=1
export DL_DB_PATH="$SCRATCH/scratch.sqlite"
export XDG_STATE_HOME="$SCRATCH/state"

generate_program() {
  local statements="$1"
  local destination="$2"
  local relations=$(((statements + 1) / 2))
  local rules=$((statements - relations))

  awk -v relations="$relations" -v rules="$rules" '
    BEGIN {
      for (i = 0; i < relations; i++)
        printf "rel r%d(x: text).\n", i
      for (i = 1; i <= rules; i++)
        printf "r%d(X) <- r%d(X).\n", i, i - 1
    }
  ' > "$destination"
}

for n in $SIZES; do
  generate_program "$n" "$SCRATCH/$n.dl6"
done

echo "swipl: $(swipl --version)"
echo "machine: $(uname -a)"
echo "repeats: $REPEATS"
echo "fixture: N statements; ceil(N/2) relations; floor(N/2) dependent rules"
echo "hermetic: SPREFA_CONFIG=$SPREFA_CONFIG DL_NO_DAEMON=$DL_NO_DAEMON"
echo "scratch_db: $DL_DB_PATH"

if command -v hyperfine >/dev/null 2>&1; then
  RUNNER=hyperfine
else
  RUNNER=plain-time
fi
echo "runner: $RUNNER"

if [ "$WARM" = 1 ]; then
  if [ ! -f "$SCRATCH/20.dl6" ]; then
    generate_program 20 "$SCRATCH/20.dl6"
  fi
  "$COMPILE_SH" "$SCRATCH/20.dl6" "$SCRATCH/warm.ts" >/dev/null
  echo "warm: yes; one untimed compile populated file caches"
else
  echo "warm: no"
fi

printf "%-5s %-5s %-5s %10s %10s %10s %10s %10s %10s %10s\n" \
  N rels rules wall_ms parse_ms plan_ms lower_ms boot_ms emit_ms write_ms
: > "$SUMMARY_FILE"

for n in $SIZES; do
  relations=$(((n + 1) / 2))
  rules=$((n - relations))
  log_file="$SCRATCH/$n.perf.jsonl"
  out_file="$SCRATCH/$n.ts"
  : > "$log_file"

  if [ "$RUNNER" = hyperfine ]; then
    result_file="$SCRATCH/$n.hyperfine.json"
    printf -v command_line \
      "DL_PERF_LOG=%q SPREFA_CONFIG=%q DL_NO_DAEMON=1 DL_DB_PATH=%q XDG_STATE_HOME=%q %q %q %q >/dev/null" \
      "$log_file" "$SPREFA_CONFIG" "$DL_DB_PATH" "$XDG_STATE_HOME" \
      "$COMPILE_SH" "$SCRATCH/$n.dl6" "$out_file"
    hyperfine --runs "$REPEATS" --warmup 0 --export-json "$result_file" \
      "$command_line" >/dev/null
    wall_ms="$(jq -r '.results[0].mean * 1000' "$result_file")"
  else
    wall_total=0
    for repeat in $(seq 1 "$REPEATS"); do
      time_file="$SCRATCH/$n.$repeat.time"
      DL_PERF_LOG="$log_file" /usr/bin/time -p -o "$time_file" \
        "$COMPILE_SH" "$SCRATCH/$n.dl6" "$out_file" >/dev/null
      elapsed="$(awk '$1 == "real" { print $2 }' "$time_file")"
      wall_total="$(awk -v total="$wall_total" -v elapsed="$elapsed" \
        'BEGIN { printf "%.6f", total + elapsed * 1000 }')"
    done
    wall_ms="$(awk -v total="$wall_total" -v repeats="$REPEATS" \
      'BEGIN { printf "%.3f", total / repeats }')"
  fi

  phase_mean() {
    jq -s -r --arg phase "$1" \
      '[.[] | select(.phase == $phase) | .wall_ms] |
       if length == 0 then 0 else add / length end' "$log_file"
  }

  parse_ms="$(phase_mean parse)"
  plan_ms="$(phase_mean plan)"
  lower_ms="$(phase_mean lower)"
  boot_ms="$(phase_mean boot)"
  emit_ms="$(phase_mean emit)"
  write_ms="$(phase_mean write)"

  printf "%-5d %-5d %-5d %10.2f %10.2f %10.2f %10.2f %10.2f %10.2f %10.2f\n" \
    "$n" "$relations" "$rules" "$wall_ms" \
    "$parse_ms" "$plan_ms" "$lower_ms" "$boot_ms" "$emit_ms" "$write_ms"
  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$n" "$relations" "$rules" "$wall_ms" \
    "$parse_ms" "$plan_ms" "$lower_ms" "$boot_ms" "$emit_ms" "$write_ms" \
    >> "$SUMMARY_FILE"
done

awk -F '\t' '
  NR == 1 {
    previous_rules = $3
    previous_plan = $6
    next
  }
  {
    exponent = log($6 / previous_plan) / log($3 / previous_rules)
    previous_rules = $3
    previous_plan = $6
  }
  END {
    phase_name[5] = "parse"
    phase_name[6] = "plan"
    phase_name[7] = "lower"
    phase_name[8] = "boot"
    phase_name[9] = "emit"
    phase_name[10] = "write"
    dominant_column = 5
    for (column = 6; column <= 10; column++)
      if ($column > $dominant_column)
        dominant_column = column
    printf "\ndominant_phase: %s (largest N phase wall %.2f ms)\n",
      phase_name[dominant_column], $dominant_column
    if (NR >= 2)
      printf "controlled_shape: plan scales as rules^%.3f over the final interval\n",
        exponent
    print "general_driver: repeated simple-path enumeration in clock_check:graph_reachable/4"
    print "dense_graph_shape: simple-path count can be exponential in dependency-graph connectivity"
  }
' "$SUMMARY_FILE"

if [ "$EXEC_PROFILE_N" != 0 ]; then
  if [ ! -f "$SCRATCH/$EXEC_PROFILE_N.dl6" ]; then
    generate_program "$EXEC_PROFILE_N" "$SCRATCH/$EXEC_PROFILE_N.dl6"
  fi
  echo
  echo "SWI execution profiler: warm N=$EXEC_PROFILE_N, wall sampling at 1000 Hz, all ports"
  swipl -q -l "$COMPILE_DIR/6_profile.pl" \
    -g "execution_profile_dl6('$SCRATCH/$EXEC_PROFILE_N.dl6','$SCRATCH/profile.ts')" \
    -g halt
fi
