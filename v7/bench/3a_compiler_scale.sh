#!/usr/bin/env bash
set -euo pipefail

v7_dir="$(cd "$(dirname "$0")/.." && pwd)"
max_files="${1:-8}"
types_per_file="${2:-16}"
fields_per_type="${3:-8}"
repetitions="${4:-1}"

files=1
while (( files <= max_files )); do
  timeout 3 swipl -q \
    -s "$v7_dir/bench/3_compiler_scale.pl" \
    -g main -t halt -- \
    "$files" "$types_per_file" "$fields_per_type" "$repetitions"
  files=$((files * 2))
done

