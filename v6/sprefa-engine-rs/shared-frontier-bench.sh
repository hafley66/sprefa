#!/usr/bin/env bash
# Both frontier arms: emitted bytes, statements per fold, fold wall median of three.
set -uo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/v6/sprefa-engine-rs"
harness="$here/target/debug/emit_rust_harness"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT INT TERM

sources=("$@")
if [ "${#sources[@]}" = 0 ]; then
  sources=("$here"/tests/shared_frontier_wide/*.dl6)
fi

cargo build --quiet --manifest-path "$here/Cargo.toml" --bin emit_rust_harness || exit 1

median_of_three() { printf '%s\n' "$1" "$2" "$3" | sort -n | sed -n 2p; }

for source_file in "${sources[@]}"; do
  name=$(basename "$source_file" .dl6)
  schedule="$(dirname "$source_file")/$name.schedule.json"
  [ -f "$schedule" ] || { echo "SKIP $name no schedule beside it"; continue; }
  for arm in per_rel shared; do
    if [ "$arm" = shared ]; then
      options="[emitter(emit_rust:emit_program), frontier(shared)]"
    else
      options="[emitter(emit_rust:emit_program)]"
    fi
    program="$scratch/${name}_${arm}.rs"
    if ! timeout 900 swipl --stack_limit=12G -q \
      -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
      -g "compile_dl6('$source_file','$program',$options)" -g halt \
      >"$scratch/${name}_${arm}.compile.log" 2>&1 || [ ! -s "$program" ]; then
      printf 'STOP %s %s %s\n' "$name" "$arm" \
        "$(grep -m1 -o 'unsupported_construct(.*)' "$scratch/${name}_${arm}.compile.log" \
           || tail -n1 "$scratch/${name}_${arm}.compile.log")"
      continue
    fi
    walls=()
    tally=0
    for _run in 1 2 3; do
      started=$(python3 -c 'import time; print(time.time())')
      RUST_LOG=sprefa_engine_rs=info NO_COLOR=1 "$harness" "$program" "$schedule" --final \
        >"$scratch/${name}_${arm}.out" 2>"$scratch/${name}_${arm}.err"
      walls+=("$(python3 -c "import time; print(round((time.time()-$started)*1000,1))")")
      tally=$(sed $'s/\033\\[[0-9;]*m//g' "$scratch/${name}_${arm}.err" \
        | grep -o 'statements=[0-9]*' | tail -1 | cut -d= -f2)
    done
    printf 'BENCH %s %s emitted_bytes=%s statements_per_fold=%s fold_ms_median=%s runs=%s,%s,%s\n' \
      "$name" "$arm" "$(wc -c <"$program" | tr -d ' ')" "$tally" \
      "$(median_of_three "${walls[0]}" "${walls[1]}" "${walls[2]}")" \
      "${walls[0]}" "${walls[1]}" "${walls[2]}"
  done
done
