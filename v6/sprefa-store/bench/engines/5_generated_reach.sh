#!/usr/bin/env bash
set -euo pipefail
dir=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$dir/../../../.." && pwd)
runtime="${GENERATED_REACH_RUNTIME:-tsv2}"
case "$runtime" in
  tsv2)
    exec node --max-old-space-size="${DL_MEMCAP_MB:-4096}" --experimental-transform-types \
      "$root/v6/tsv2/scripts/scale-bench.ts" reach "$1" "$2"
    ;;
  rust)
    scratch=$(mktemp -d)
    trap 'rm -rf -- "$scratch"' EXIT
    node "$dir/1_postgres_reach.mjs" fixture "$1" "$2" > "$scratch/graph.json"
    set +e
    /usr/bin/time -l "$root/v6/sprefa-engine-rs/target/release/examples/0_reach_bench" \
      "$root/v6/sprefa-store/bench/1_root_reach.program.rs" "$scratch/graph.json" 2>"$scratch/worker.log"
    status=$?
    set -e
    rss=$(awk '/maximum resident set size/ {printf "%.1f", $1/1048576}' "$scratch/worker.log")
    sed "s/RSS_FROM_TIME/${rss:-N\/A}/g" "$scratch/worker.log" >&2
    exit "$status"
    ;;
esac
