#!/usr/bin/env bash
set -euo pipefail

v7_dir="$(cd "$(dirname "$0")/.." && pwd)"
max_files="${1:-8}"
types_per_file="${2:-16}"
fields_per_type="${3:-8}"
repetitions="${4:-1}"
trace_mode="${5:-off}"
measurements="$(mktemp "${TMPDIR:-/tmp}/dl7-scale.XXXXXX")"
trap 'rm -f "$measurements"' EXIT

files=1
while (( files <= max_files )); do
  result="$(timeout 3 swipl -q \
    -s "$v7_dir/bench/3_compiler_scale.pl" \
    -g main -t halt -- \
    "$files" "$types_per_file" "$fields_per_type" "$repetitions" \
    "$trace_mode")"
  printf '%s\n' "$result"
  printf '%s\n' "$result" >> "$measurements"
  files=$((files * 2))
done

jq -c -s -f "$v7_dir/bench/3b_compiler_scale_delta.jq" "$measurements"
