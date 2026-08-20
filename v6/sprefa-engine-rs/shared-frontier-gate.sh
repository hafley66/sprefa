#!/usr/bin/env bash
# Rust-door parity gate: per_rel vs frontier(shared) tick logs per fixture.
# The retraction cases compile from the fixture TERM and add the oracle as a
# third arm, so all three tick logs are byte-diffed.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
fixtures="$root/v6/tsv2/tests/shared_frontier"
terms="$fixtures/retraction.fixtures.pl"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cargo build --quiet --manifest-path "$here/Cargo.toml" --bin emit_rust_harness
harness="$here/target/debug/emit_rust_harness"
failed=0

report() {
  local name="$1" per="$2" shared="$3" oracle="$4"
  if ! diff -q "$per" "$shared" >/dev/null; then
    echo "FAIL $name rust arms differ"
    diff "$per" "$shared" | head -6
    failed=1
    return
  fi
  if [ -n "$oracle" ] && ! diff -q "$shared" "$oracle" >/dev/null; then
    echo "FAIL $name shared arm differs from the oracle"
    diff "$oracle" "$shared" | head -6
    failed=1
    return
  fi
  local suffix=""
  [ -n "$oracle" ] && suffix=" and oracle"
  echo "PASS $name rust ticks identical${suffix} ($(wc -l < "$per" | tr -d ' ') lines)"
}

for source in "$fixtures"/*.dl6; do
  name="$(basename "$source" .dl6)"
  schedule="$fixtures/$name.schedule.json"
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "compile_dl6('$source','$scratch/${name}_per.rs',[emitter(emit_rust:emit_program)])" -g halt >/dev/null
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "compile_dl6('$source','$scratch/${name}_shared.rs',[emitter(emit_rust:emit_program), frontier(shared)])" -g halt >/dev/null
  "$harness" "$scratch/${name}_per.rs" "$schedule" > "$scratch/${name}_per.out"
  "$harness" "$scratch/${name}_shared.rs" "$schedule" > "$scratch/${name}_shared.out"
  report "$name" "$scratch/${name}_per.out" "$scratch/${name}_shared.out" ""
done

for name in sf_retract_current sf_retract_stale sf_negation_support sf_two_rule_support; do
  schedule="$fixtures/$name.schedule.json"
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "default_intern_mode(M), compile_fixture($name,'$terms','$scratch/${name}_per.rs',emit_rust:emit_program,[intern(M)])" -g halt >/dev/null
  swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
    -g "default_intern_mode(M), compile_fixture($name,'$terms','$scratch/${name}_shared.rs',emit_rust:emit_program,[intern(M), frontier(shared)])" -g halt >/dev/null
  swipl -q -l "$root/v6/prolog/conformance/ticklog.pl" \
    -g "ensure_loaded('$terms')" -g "emit($name)" -g halt > "$scratch/${name}.oracle.out"
  "$harness" "$scratch/${name}_per.rs" "$schedule" > "$scratch/${name}_per.out"
  "$harness" "$scratch/${name}_shared.rs" "$schedule" > "$scratch/${name}_shared.out"
  report "$name" "$scratch/${name}_per.out" "$scratch/${name}_shared.out" "$scratch/${name}.oracle.out"
done

exit "$failed"
