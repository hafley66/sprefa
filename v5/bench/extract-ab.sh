#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build --release --example extract_ab

runs="${RUNS:-7}"
sizes="${SIZES:-128 512 1024 2048}"
out="${OUT:-/tmp/sprefa-extract-ab}"
rm -rf "$out"
mkdir -p "$out"

case "$(uname -s)" in
  Darwin) time_args=(-l) ;;
  Linux) time_args=(-v) ;;
  *) echo "unsupported platform for peak-RSS measurement" >&2; exit 2 ;;
esac

for n in $sizes; do
  target/release/examples/extract_ab verify "$n" >"$out/verify-$n.json"
  for run in $(seq 1 "$runs"); do
    if (( run % 2 == 1 )); then arms=(baseline bundle); else arms=(bundle baseline); fi
    for arm in "${arms[@]}"; do
      /usr/bin/time "${time_args[@]}" -o "$out/time-$n-$run-$arm.txt" \
        target/release/examples/extract_ab "$arm" "$n" >"$out/$n-$run-$arm.json"
    done
  done
done

echo "results: $out"
