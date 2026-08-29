#!/usr/bin/env bash
set -euo pipefail

lab_dir=$(cd "$(dirname "$0")" && pwd)
mode=${1:-full}
n=${2:-48}
repetitions=5

if ! [[ "$n" =~ ^[1-9][0-9]*$ ]]; then
  printf 'N must be a positive integer: %s\n' "$n" >&2
  exit 2
fi

case "$mode" in
  smoke|full) ;;
  *)
    printf 'usage: %s [smoke|full] [N]\n' "$0" >&2
    exit 2
    ;;
esac

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/dl7-runtime-shootout.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT
raw_file="$tmp_dir/measurements.jsonl"
: > "$raw_file"

expected_count() {
  local graph_case=$1
  local graph_n=$2
  case "$graph_case" in
    chain) printf '%s\n' "$((graph_n * (graph_n - 1) / 2))" ;;
    ring) printf '%s\n' "$((graph_n * graph_n))" ;;
  esac
}

edge_count() {
  local graph_case=$1
  local graph_n=$2
  case "$graph_case" in
    chain) printf '%s\n' "$((graph_n - 1))" ;;
    ring) printf '%s\n' "$graph_n" ;;
  esac
}

set_arm_command() {
  local runtime=$1
  local graph_case=$2
  local graph_n=$3
  case "$runtime" in
    sbcl) command_argv=(sbcl --noinform --disable-debugger --script "$lab_dir/1_sbcl.lisp" "$graph_case" "$graph_n") ;;
    swi) command_argv=(swipl -q -s "$lab_dir/2_swi.pl" -- "$graph_case" "$graph_n") ;;
    racket) command_argv=(racket "$lab_dir/3_racket.rkt" "$graph_case" "$graph_n") ;;
  esac
}

set_startup_command() {
  local runtime=$1
  case "$runtime" in
    sbcl) command_argv=(sbcl --noinform --disable-debugger --script /dev/null) ;;
    swi) command_argv=(swipl -q -s /dev/null -g halt) ;;
    racket) command_argv=(racket -e '(void)') ;;
  esac
}

run_arm() {
  set_arm_command "$@"
  "${command_argv[@]}"
}

run_startup() {
  set_startup_command "$1"
  "${command_argv[@]}"
}

runtime_version() {
  local runtime=$1
  case "$runtime" in
    sbcl) sbcl --version | awk '{print $2}' ;;
    swi) swipl --version | sed 's/^SWI-Prolog version \([^ ]*\).*/\1/' ;;
    racket) racket --version | sed 's/^Welcome to Racket v\([^ ]*\).*/\1/' ;;
  esac
}

validate_arm_json() {
  local runtime=$1
  local graph_case=$2
  local graph_n=$3
  local json_file=$4
  local expected_edges expected_closure
  expected_edges=$(edge_count "$graph_case" "$graph_n")
  expected_closure=$(expected_count "$graph_case" "$graph_n")
  jq -e \
    --arg runtime "$runtime" \
    --arg graph_case "$graph_case" \
    --argjson n "$graph_n" \
    --argjson edge_count "$expected_edges" \
    --argjson closure_count "$expected_closure" \
    '(.runtime == $runtime)
      and (.version | type == "string" and length > 0)
      and (.case == $graph_case)
      and (.n == $n)
      and (.edge_count == $edge_count)
      and (.closure_count == $closure_count)
      and (.setup_ms | type == "number" and . >= 0)
      and (.closure_ms | type == "number" and . >= 0)' \
    "$json_file" >/dev/null
}

measure_startup() {
  local runtime=$1
  local repetition=$2
  local output_file="$tmp_dir/startup-${runtime}-${repetition}.out"
  local time_file="$tmp_dir/startup-${runtime}-${repetition}.time"
  set_startup_command "$runtime"
  /usr/bin/time -lp "${command_argv[@]}" >"$output_file" 2>"$time_file"
  local process_ms peak_rss version
  process_ms=$(awk '/^real / { printf "%.3f", $2 * 1000 }' "$time_file")
  peak_rss=$(awk '/maximum resident set size/ { print $1 }' "$time_file")
  version=$(runtime_version "$runtime")
  jq -cn \
    --arg runtime "$runtime" \
    --arg version "$version" \
    --argjson repetition "$repetition" \
    --argjson process_ms "$process_ms" \
    --argjson peak_rss_bytes "$peak_rss" \
    '{kind:"startup", runtime:$runtime, version:$version, repetition:$repetition, process_ms:$process_ms, peak_rss_bytes:$peak_rss_bytes}' \
    >> "$raw_file"
}

measure_arm() {
  local runtime=$1
  local graph_case=$2
  local graph_n=$3
  local repetition=$4
  local output_file="$tmp_dir/${runtime}-${graph_case}-${repetition}.json"
  local time_file="$tmp_dir/${runtime}-${graph_case}-${repetition}.time"
  set_arm_command "$runtime" "$graph_case" "$graph_n"
  /usr/bin/time -lp "${command_argv[@]}" >"$output_file" 2>"$time_file"
  validate_arm_json "$runtime" "$graph_case" "$graph_n" "$output_file"
  local process_ms peak_rss
  process_ms=$(awk '/^real / { printf "%.3f", $2 * 1000 }' "$time_file")
  peak_rss=$(awk '/maximum resident set size/ { print $1 }' "$time_file")
  jq -c \
    --argjson repetition "$repetition" \
    --argjson process_ms "$process_ms" \
    --argjson peak_rss_bytes "$peak_rss" \
    '. + {kind:"closure", repetition:$repetition, process_ms:$process_ms, peak_rss_bytes:$peak_rss_bytes}' \
    "$output_file" >> "$raw_file"
}

smoke() {
  local runtime graph_case output_file
  for runtime in sbcl swi racket; do
    for graph_case in chain ring; do
      output_file="$tmp_dir/smoke-${runtime}-${graph_case}.json"
      run_arm "$runtime" "$graph_case" "$n" > "$output_file"
      validate_arm_json "$runtime" "$graph_case" "$n" "$output_file"
      jq -c . "$output_file"
    done
  done
}

generate_results() {
  local elapsed_seconds=$1
  local results_file="$lab_dir/5_RESULTS.md"
  {
    printf '# Native-logic runtime shootout results\n\n'
    printf -- '- Generated: %s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')"
    printf -- '- Machine: %s, %s\n' "$(uname -m)" "$(sw_vers -productVersion)"
    printf -- '- N: %s\n' "$n"
    printf -- '- Protocol: one warmup, five measured repetitions\n'
    printf -- '- Total measured harness wall time: %s seconds\n\n' "$elapsed_seconds"
    printf 'Algorithms are idiomatic to each runtime. These results measure the selected native logic routes rather than equal low-level algorithms.\n\n'
    printf '## Process startup\n\n'
    printf '| Runtime | Version | Median startup ms | Peak RSS bytes |\n'
    printf '| --- | --- | ---: | ---: |\n'
    jq -sr '
      def median: sort | .[length / 2 | floor];
      [.[] | select(.kind == "startup")]
      | group_by(.runtime)[]
      | "| \(.[0].runtime) | \(.[0].version) | \([.[].process_ms] | median) | \([.[].peak_rss_bytes] | max) |"
    ' "$raw_file"
    printf '\n## Closure cases\n\n'
    printf '| Runtime | Case | Edges | Closure pairs | Median setup ms | Median closure ms | Median process ms | Peak RSS bytes |\n'
    printf '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n'
    jq -sr '
      def median: sort | .[length / 2 | floor];
      [.[] | select(.kind == "closure")]
      | group_by([.runtime, .case])[]
      | "| \(.[0].runtime) | \(.[0].case) | \(.[0].edge_count) | \(.[0].closure_count) | \([.[].setup_ms] | median) | \([.[].closure_ms] | median) | \([.[].process_ms] | median) | \([.[].peak_rss_bytes] | max) |"
    ' "$raw_file"
    printf '\n## Measured records\n\n```jsonl\n'
    cat "$raw_file"
    printf '```\n'
  } > "$results_file"
}

if [[ "$mode" == smoke ]]; then
  smoke
  exit 0
fi

start_seconds=$SECONDS
for runtime in sbcl swi racket; do
  run_startup "$runtime" >/dev/null
  for repetition in $(seq 1 "$repetitions"); do
    measure_startup "$runtime" "$repetition"
  done
done

for runtime in sbcl swi racket; do
  for graph_case in chain ring; do
    run_arm "$runtime" "$graph_case" "$n" >/dev/null
    for repetition in $(seq 1 "$repetitions"); do
      measure_arm "$runtime" "$graph_case" "$n" "$repetition"
    done
  done
done

elapsed_seconds=$((SECONDS - start_seconds))
if ((elapsed_seconds >= 60)); then
  printf 'shootout exceeded the 60 second bound: %s seconds\n' "$elapsed_seconds" >&2
  exit 1
fi
generate_results "$elapsed_seconds"
printf 'wrote %s\n' "$lab_dir/5_RESULTS.md"
