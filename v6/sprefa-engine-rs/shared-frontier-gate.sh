#!/usr/bin/env bash
# Rust-door parity gate: per_rel vs frontier(shared) tick logs per fixture.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
fixtures="$root/v6/tsv2/tests/shared_frontier"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cargo build --quiet --manifest-path "$here/Cargo.toml" --bin emit_rust_harness
harness="$here/target/debug/emit_rust_harness"
failed=0
for source in "$fixtures"/*.dl6; do
  name="$(basename "$source" .dl6)"
  schedule="$fixtures/$name.schedule.json"
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "compile_dl6('$source','$scratch/${name}_per.rs',[emitter(emit_rust:emit_program)])" -g halt >/dev/null
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "compile_dl6('$source','$scratch/${name}_shared.rs',[emitter(emit_rust:emit_program), frontier(shared)])" -g halt >/dev/null
  "$harness" "$scratch/${name}_per.rs" "$schedule" > "$scratch/${name}_per.out"
  "$harness" "$scratch/${name}_shared.rs" "$schedule" > "$scratch/${name}_shared.out"
  if diff -q "$scratch/${name}_per.out" "$scratch/${name}_shared.out" >/dev/null; then
    echo "PASS $name rust ticks identical ($(wc -l < "$scratch/${name}_per.out" | tr -d ' ') lines)"
  else
    echo "FAIL $name rust arms differ"
    diff "$scratch/${name}_per.out" "$scratch/${name}_shared.out" | head -6
    failed=1
  fi
done
exit "$failed"
