#!/usr/bin/env bash

# Reproduces every number in REPORT-INTERN.md in one session: build release,
# regenerate both inputs, gate byte-identity, run best-of-3.
set -euo pipefail

lab_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shootout_dir="$(cd "$lab_dir/.." && pwd)"
work_dir="${1:-${TMPDIR:-/tmp}/intern_bench_work}"
runs="${RUNS:-3}"
families="${FAMILIES:-chain grid}"
scale="${SCALE:-10000}"

mkdir -p "$work_dir/harness_in" "$work_dir/gen"
results="$work_dir/results.jsonl"
: >"$results"

echo "== build ==" >&2
(cd "$shootout_dir/harness" && cargo build --release --quiet)
(cd "$shootout_dir/interp" && cargo build --release --quiet)
(cd "$lab_dir" && cargo build --release --quiet --bins)

harness_bin="$shootout_dir/harness/target/release/harness"
interp_bin="$shootout_dir/interp/target/release/interp"
gen_text_bin="$lab_dir/target/release/gen_text"
bench_bin="$lab_dir/target/release/bench"
sqlite_bin="$lab_dir/target/release/sqlite_keys"

echo "== harness inputs (committed generator, untouched) ==" >&2
"$harness_bin" --engines "$interp_bin" --scales "$scale" \
  --work "$work_dir/harness_in" \
  --standings "$work_dir/harness_in/STANDINGS.scratch.md" >/dev/null 2>&1

echo "== TEXT inputs and their int twins ==" >&2
for family in $families; do
  "$gen_text_bin" --family "$family" --scale "$scale" \
    --out "$work_dir/gen/${family}_${scale}.tin" \
    --also-int "$work_dir/gen/${family}_${scale}.in"
done

echo "== gate: the int twin must be byte-identical to the harness .in ==" >&2
for family in $families; do
  left="$work_dir/harness_in/${family}_${scale}.in"
  right="$work_dir/gen/${family}_${scale}.in"
  if ! cmp -s "$left" "$right"; then
    echo "BYTE-IDENTITY GATE FAILED: $left differs from $right" >&2
    exit 1
  fi
  echo "identical: ${family}_${scale}.in" >&2
done

echo "== runs ==" >&2
for family in $families; do
  for run in $(seq 1 "$runs"); do
    line=$("$interp_bin" --input "$work_dir/harness_in/${family}_${scale}.in" 2>/dev/null)
    while IFS= read -r event; do
      echo "${family}_${scale} interp-int $run $event" >>"$results"
    done <<<"$line"
    for mode in pair pair-flat col4; do
      line=$("$bench_bin" --input "$work_dir/gen/${family}_${scale}.tin" --mode "$mode" 2>/dev/null)
      while IFS= read -r event; do
        echo "${family}_${scale} $mode $run $event" >>"$results"
      done <<<"$line"
    done
  done
done

echo "== sqlite insert race ==" >&2
for family in $families; do
  while IFS= read -r event; do
    echo "${family}_${scale} sqlite 1 $event" >>"$results"
  done < <("$sqlite_bin" --input "$work_dir/gen/${family}_${scale}.tin" --runs 5)
done
while IFS= read -r event; do
  echo "synth sqlite 1 $event" >>"$results"
done < <("$sqlite_bin" --synth "${SYNTH:-10000,100000,1000000}" --runs 3)

echo "== best of $runs ==" >&2
awk -f "$lab_dir/best.awk" "$results"
echo "raw runs: $results" >&2
